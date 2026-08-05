#!/usr/bin/env bash
#
# Convenience script for local/manual testing only - NOT used by CI.
#
# Creates the sample "orders" table in the Pinot cluster started by the root
# docker-compose.yaml, and loads a handful of rows into it. Run this *after*
# `docker compose up` has brought the cluster up (give the controller a few
# seconds to finish leader election first).
#
# Usage (from the repository root):
#   ./tests/pinot-db/init/seed.sh
#
# What it does, step by step:
#   1. Waits for the Pinot Controller's REST API to answer.
#   2. POSTs schema.json (the "orders" table schema) to the Controller.
#   3. POSTs table.json (the OFFLINE table config referencing that schema).
#   4. Writes a small sample CSV of order rows, and a Pinot batch-ingestion
#      "job spec" describing how to turn that CSV into a segment and push it
#      to the cluster.
#   5. Runs that ingestion job with `pinot-admin.sh LaunchDataIngestionJob`,
#      executed *inside* the pinot-controller container (via `docker compose
#      exec`) since that's where the Pinot admin tooling lives. This is
#      Pinot's standard "standalone" segment-creation-and-push flow, just
#      pointed at a couple of local files instead of a real data lake.
#
# This is a manual-testing convenience script, not production ingestion code:
# it favors the simplest thing that works over robustness (no retries beyond
# the initial health check, no incremental/upsert handling, etc).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTROLLER_URL="${PINOT_CONTROLLER_URL:-http://localhost:9000}"

echo "==> Waiting for Pinot Controller at ${CONTROLLER_URL}..."
until curl -s -o /dev/null "${CONTROLLER_URL}/health"; do
  printf '.'
  sleep 2
done
echo " up"

echo "==> Creating schema 'orders'"
curl -sf -X POST \
  -H "Content-Type: application/json" \
  -d @"${SCRIPT_DIR}/schema.json" \
  "${CONTROLLER_URL}/schemas" \
  || echo "    (schema may already exist - continuing)"

echo "==> Creating table 'orders_OFFLINE'"
curl -sf -X POST \
  -H "Content-Type: application/json" \
  -d @"${SCRIPT_DIR}/table.json" \
  "${CONTROLLER_URL}/tables" \
  || echo "    (table may already exist - continuing)"

# --- Sample data -------------------------------------------------------
#
# A handful of rows is enough to exercise the connector manually. `tags` is a
# multi-value STRING column, and Pinot's CSV reader expects multi-value fields
# to be delimited with ';' by default.
DATA_DIR="${SCRIPT_DIR}/data"
mkdir -p "${DATA_DIR}"
cat > "${DATA_DIR}/orders.csv" <<'CSV'
orderId,customerId,status,tags,total,createdAt
order-1,cust-1,COMPLETED,gift;priority,42.50,1700000000000
order-2,cust-2,PENDING,,19.99,1700000100000
order-3,cust-1,COMPLETED,priority,105.00,1700000200000
order-4,cust-3,CANCELLED,gift,7.25,1700000300000
order-5,cust-2,COMPLETED,,250.00,1700000400000
CSV

# --- Batch ingestion job spec -------------------------------------------
#
# Tells Pinot's standalone segment generation tool to read the CSV above,
# build a segment for the "orders" table, and push that segment straight to
# the Controller. All paths here are as seen *inside* the pinot-controller
# container, where docker-compose.yaml mounts this directory at
# /tmp/pinot-init.
SEGMENTS_DIR="${DATA_DIR}/../segments"
mkdir -p "${SEGMENTS_DIR}"
cat > "${SCRIPT_DIR}/job-spec.yaml" <<'YAML'
executionFrameworkSpec:
  name: standalone
  segmentGenerationJobRunnerClassName: org.apache.pinot.plugin.ingestion.batch.standalone.SegmentGenerationJobRunner
  segmentTarPushJobRunnerClassName: org.apache.pinot.plugin.ingestion.batch.standalone.SegmentTarPushJobRunner
jobType: SegmentCreationAndTarPush
inputDirURI: /tmp/pinot-init/data
outputDirURI: /tmp/pinot-init/segments
overwriteOutput: true
pinotFSSpecs:
  - scheme: file
    className: org.apache.pinot.spi.filesystem.LocalPinotFS
recordReaderSpec:
  dataFormat: csv
  className: org.apache.pinot.plugin.inputformat.csv.CSVRecordReader
tableSpec:
  tableName: orders
pinotClusterSpecs:
  - controllerURI: http://pinot-controller:9000
pushJobSpec:
  pushAttempts: 2
  pushRetryIntervalMillis: 1000
YAML

echo "==> Running segment creation + push job inside the pinot-controller container"
docker compose exec -T pinot-controller \
  bin/pinot-admin.sh LaunchDataIngestionJob \
  -jobSpecFile /tmp/pinot-init/job-spec.yaml

echo "==> Done. Query the table with:"
echo "    curl -s -H 'Content-Type: application/json' \\"
echo "      -d '{\"sql\": \"select * from orders limit 10\"}' \\"
echo "      \${PINOT_BROKER_URL:-http://localhost:8099}/query/sql | jq ."
