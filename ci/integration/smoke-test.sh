#!/usr/bin/env bash
#
# End-to-end smoke test for the Pinot NDC connector.
#
# Assumes:
#   * a Pinot cluster is up and has been seeded with the sample "orders" table
#     (see tests/pinot-db/init/seed.sh), and
#   * the ndc-pinot connector is already serving against that cluster.
#
# It drives the connector purely through its public NDC HTTP API - the same
# surface the Hasura engine talks to - and asserts on the results, so a green
# run means "a GraphQL/NDC query actually made it through the connector, into
# Pinot, and back with the right data".
#
# This is the single source of truth for the integration assertions: CI runs
# it (.github/workflows/integration_test.yaml), and developers can run it
# locally against the docker-compose stack. It is deliberately dependency-light
# (bash + curl + jq) so it works the same everywhere.
#
# The script is self-gating: it waits for the connector to answer and for the
# seeded rows to actually be queryable before it asserts anything, so it's safe
# to run immediately after backgrounding the connector / seeding (locally or in
# CI) without racing startup or Pinot's segment-load lag.
#
# Configuration (env vars, all optional):
#   CONNECTOR_URL   NDC connector base URL   (default http://localhost:5000)
#   BROKER_URL      Pinot broker base URL    (default http://localhost:8099)
#   READY_TIMEOUT   seconds to wait for connector + data readiness (default 120)
#
# Note: 5000 matches the port the docker-compose stack maps and CI sets via
# HASURA_CONNECTOR_PORT. The ndc-sdk's own default is 8080, so if you start the
# connector with a bare `serve` and no port override, point CONNECTOR_URL at
# http://localhost:8080 (or start it with HASURA_CONNECTOR_PORT=5000).
#
# Usage:
#   ./ci/integration/smoke-test.sh

set -euo pipefail

CONNECTOR_URL="${CONNECTOR_URL:-http://localhost:5000}"
BROKER_URL="${BROKER_URL:-http://localhost:8099}"
READY_TIMEOUT="${READY_TIMEOUT:-120}"

pass=0
fail=0

# --- tiny assertion helpers ------------------------------------------------

# green/red only when attached to a terminal, so CI logs stay clean.
if [ -t 1 ]; then
  GREEN=$'\033[32m'; RED=$'\033[31m'; DIM=$'\033[2m'; RESET=$'\033[0m'
else
  GREEN=''; RED=''; DIM=''; RESET=''
fi

ok()   { pass=$((pass + 1)); echo "  ${GREEN}✓${RESET} $1"; }
bad()  { fail=$((fail + 1)); echo "  ${RED}✗${RESET} $1"; }

# assert_eq <description> <actual> <expected>
assert_eq() {
  local desc="$1" actual="$2" expected="$3"
  if [ "$actual" = "$expected" ]; then
    ok "$desc"
  else
    bad "$desc"
    echo "      expected: ${expected}"
    echo "      actual:   ${actual}"
  fi
}

# POST an NDC query body to the connector and echo the response body.
ndc_query() {
  curl -sS -X POST "${CONNECTOR_URL}/query" \
    -H 'Content-Type: application/json' \
    --data-binary "$1"
}

# count(*) over the seeded table, via the broker. Echoes 0 (not an error) when
# the segment hasn't loaded yet, so callers can poll on it.
orders_count() {
  curl -sS -H 'Content-Type: application/json' \
    -d '{"sql":"select count(*) from orders"}' \
    "${BROKER_URL}/query/sql" 2>/dev/null | jq -r '.resultTable.rows[0][0] // 0'
}

echo "==> Connector: ${CONNECTOR_URL}"
echo "==> Broker:    ${BROKER_URL}"

# --- readiness -------------------------------------------------------------
# Make the script robust to being launched the instant the connector is
# backgrounded and the segment is pushed: wait for the connector's /health and
# for the seeded rows to be queryable before asserting. seed.sh returns as soon
# as the segment *push* is accepted, but the Pinot server needs a moment to
# load that segment before its rows show up - poll rather than race it.
echo
echo "[wait] connector /health and seeded data become ready (timeout ${READY_TIMEOUT}s)"
deadline_steps=$(( READY_TIMEOUT / 2 ))
connector_ready=""
for _ in $(seq 1 "$deadline_steps"); do
  if [ "$(curl -s -o /dev/null -w '%{http_code}' "${CONNECTOR_URL}/health" 2>/dev/null)" = "200" ]; then
    connector_ready=1
    break
  fi
  sleep 2
done
if [ -z "$connector_ready" ]; then
  echo "  ${RED}✗${RESET} connector did not become healthy at ${CONNECTOR_URL} within ${READY_TIMEOUT}s"
  exit 1
fi
data_ready=""
for _ in $(seq 1 "$deadline_steps"); do
  if [ "$(orders_count)" = "5" ]; then
    data_ready=1
    break
  fi
  sleep 2
done
if [ -z "$data_ready" ]; then
  echo "  ${RED}✗${RESET} seeded rows not queryable via the broker within ${READY_TIMEOUT}s (last count=$(orders_count))"
  exit 1
fi
echo "  ${GREEN}✓${RESET} connector healthy and seeded data queryable"

# --- 0. sanity: broker actually has the seeded data ------------------------
echo
echo "[data] sample table is populated"
row_count="$(orders_count)"
assert_eq "orders has 5 rows in Pinot" "$row_count" "5"

# --- 1. capabilities -------------------------------------------------------
echo
echo "[capabilities] connector advertises the expected query capabilities"
caps="$(curl -sS "${CONNECTOR_URL}/capabilities")"
assert_eq "exposes query.aggregates" \
  "$(echo "$caps" | jq 'has("capabilities") and (.capabilities.query|has("aggregates"))')" "true"
assert_eq "exposes query.explain" \
  "$(echo "$caps" | jq '.capabilities.query|has("explain")')" "true"

# --- 2. schema -------------------------------------------------------------
echo
echo "[schema] connector exposes the orders collection"
schema="$(curl -sS "${CONNECTOR_URL}/schema")"
assert_eq "orders collection present" \
  "$(echo "$schema" | jq '[.collections[].name] | index("orders") != null')" "true"
assert_eq "orders object type has createdAt column" \
  "$(echo "$schema" | jq '.object_types.orders.fields|has("createdAt")')" "true"

# --- 3. row query with predicate + order_by + limit ------------------------
# The core of the connector: NDC request -> Pinot SQL -> mapped rows.
echo
echo "[query] filter (status = COMPLETED), order by total desc"
rows="$(ndc_query '{
  "collection": "orders",
  "arguments": {},
  "collection_relationships": {},
  "query": {
    "fields": {
      "orderId": {"type":"column","column":"orderId","fields":null},
      "status":  {"type":"column","column":"status","fields":null},
      "total":   {"type":"column","column":"total","fields":null}
    },
    "predicate": {
      "type":"binary_comparison_operator",
      "column":{"type":"column","name":"status","field_path":null,"path":[]},
      "operator":"_eq",
      "value":{"type":"scalar","value":"COMPLETED"}
    },
    "order_by": {"elements":[{"order_direction":"desc","target":{"type":"column","name":"total","path":[]}}]},
    "limit": 10
  }
}')"
assert_eq "returns exactly 3 COMPLETED orders" \
  "$(echo "$rows" | jq '.[0].rows | length')" "3"
assert_eq "ordered by total desc (orderIds)" \
  "$(echo "$rows" | jq -c '[.[0].rows[].orderId]')" '["order-5","order-3","order-1"]'
assert_eq "no non-COMPLETED rows leaked through" \
  "$(echo "$rows" | jq -c '[.[0].rows[].status] | unique')" '["COMPLETED"]'

# --- 4. multi-value column round-trips as an array -------------------------
echo
echo "[query] multi-value 'tags' column comes back as a JSON array"
tags="$(ndc_query '{
  "collection": "orders",
  "arguments": {},
  "collection_relationships": {},
  "query": {
    "fields": {"tags": {"type":"column","column":"tags","fields":null}},
    "predicate": {
      "type":"binary_comparison_operator",
      "column":{"type":"column","name":"orderId","field_path":null,"path":[]},
      "operator":"_eq",
      "value":{"type":"scalar","value":"order-1"}
    }
  }
}')"
assert_eq "order-1 tags = [gift, priority]" \
  "$(echo "$tags" | jq -c '.[0].rows[0].tags')" '["gift","priority"]'

# --- 5. aggregates ---------------------------------------------------------
echo
echo "[aggregate] star_count + sum + avg over all orders"
aggs="$(ndc_query '{
  "collection": "orders",
  "arguments": {},
  "collection_relationships": {},
  "query": {
    "aggregates": {
      "num_orders": {"type":"star_count"},
      "total_sum":  {"type":"single_column","column":"total","function":"sum"}
    }
  }
}')"
assert_eq "count(*) = 5" \
  "$(echo "$aggs" | jq '.[0].aggregates.num_orders')" "5"
assert_eq "sum(total) = 424.74" \
  "$(echo "$aggs" | jq '.[0].aggregates.total_sum')" "424.74"

# --- 6. explain ------------------------------------------------------------
echo
echo "[explain] connector returns the generated Pinot SQL"
explain="$(curl -sS -X POST "${CONNECTOR_URL}/query/explain" \
  -H 'Content-Type: application/json' \
  --data-binary '{
    "collection":"orders","arguments":{},"collection_relationships":{},
    "query":{"fields":{"orderId":{"type":"column","column":"orderId","fields":null}},"limit":1}
  }')"
assert_eq "explain output contains a SELECT ... FROM \"orders\"" \
  "$(echo "$explain" | jq -r '.details["Rows SQL"] | test("SELECT .* FROM \"orders\""; "i")')" "true"

# --- 7. multi-stage query engine (Pinot v2) --------------------------------
# The connector itself emits single-table, single-stage SQL (it declares no
# relationships and leaves cross-collection joins to the Hasura engine), so we
# exercise Pinot's multi-stage engine directly against the broker to prove the
# CI/dev stack is a genuine distributed cluster - a JOIN forces the broker to
# plan across the *server*, which only works once the server has registered and
# the segment is ONLINE. A single-stage engine rejects this JOIN outright.
#
# Self-join on customerId: cust-1 and cust-2 each have 2 orders (2 ordered
# pairs each = 4); cust-3 has 1 order (no pair). So 4 pairs, and every row must
# share a customerId across the two sides.
echo
echo "[multi-stage] SET useMultistageEngine=true self-join runs on the cluster"
ms="$(curl -sS -H 'Content-Type: application/json' \
  -d '{"sql":"SET useMultistageEngine=true; SELECT a.customerId, count(*) FROM orders a JOIN orders b ON a.customerId = b.customerId WHERE a.orderId <> b.orderId GROUP BY a.customerId ORDER BY a.customerId"}' \
  "${BROKER_URL}/query/sql")"
assert_eq "multi-stage join executes with no exceptions" \
  "$(echo "$ms" | jq -c '.exceptions')" '[]'
assert_eq "multi-stage join returns 4 same-customer order pairs" \
  "$(echo "$ms" | jq '[.resultTable.rows[][1]] | add')" "4"
assert_eq "multi-stage join pairs only within cust-1 and cust-2" \
  "$(echo "$ms" | jq -c '[.resultTable.rows[][0]] | sort')" '["cust-1","cust-2"]'

# --- summary ---------------------------------------------------------------
echo
echo "----------------------------------------"
echo "  ${GREEN}${pass} passed${RESET}, ${RED}${fail} failed${RESET}"
echo "----------------------------------------"

if [ "$fail" -ne 0 ]; then
  exit 1
fi
