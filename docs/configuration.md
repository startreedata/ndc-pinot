# Configuration

## Initializing & updating a configuration directory

The connector requires a configuration directory to run: a `configuration.json` file, plus any `.sql` files referenced by native queries.

When part of a DDN project, this configuration directory lives in the project.

### Using the Hasura DDN CLI

The Hasura DDN CLI will initialize and update the configuration directory for you. Follow the instructions on how to add a connector in the [Hasura V3 documentation](https://hasura.io/docs/3.0).

### Using the `hasura-pinot` CLI executable

You can also run the `hasura-pinot` CLI executable directly. It introspects a Pinot cluster's Controller and Broker REST APIs and writes/updates `configuration.json` accordingly.

```sh
hasura-pinot --connector-context-path ./config --controller-url "http://localhost:9000" --broker-url "http://localhost:8099" update
```

Available subcommands:

| Command | Purpose |
| --- | --- |
| `init` | Create a new configuration directory, introspecting the connected Pinot cluster's tables and schemas. |
| `update` | Re-introspect the cluster and update an existing configuration directory. Hand-edited fields (aliases, comments, native queries) are preserved. |
| `validate` | Check an existing configuration directory for errors without contacting Pinot. |
| `print-schema-and-capabilities` | Print the NDC schema and capabilities the connector would expose for a given configuration, without starting the server. |

See also: [development instructions](./development.md)

## Connection configuration (environment variables)

Connection details are deployment-specific secrets, so they're read from the environment rather than checked into `configuration.json`:

| Variable | Required | Description |
| --- | --- | --- |
| `PINOT_CONTROLLER_URL` | Yes | Base URL of the Pinot Controller REST API, e.g. `http://localhost:9000`. |
| `PINOT_BROKER_URL` | Yes | Base URL of the Pinot Broker REST API, e.g. `http://localhost:8099`. |
| `PINOT_AUTH_HEADER` | No | The verbatim value to send as the `Authorization` header on every Controller/Broker request. Use `Basic <base64>` for Pinot's built-in basic auth, or `Bearer <token>` for something like StarTree Cloud. Omit it entirely for clusters with no auth configured. |

## `configuration.json`

The configuration file has two top-level maps: `tables` and `queries`. Both map a **collection alias** (the name exposed in the graph) to its definition.

```json
{
  "$schema": "./configuration.schema.json",
  "tables": {
    "orders": {
      "name": "orders",
      "comment": null,
      "primary_key": null,
      "columns": {
        "orderId": { "scalar_type": "LONG", "is_multi_value": false, "nullable": false },
        "customerId": { "scalar_type": "STRING", "is_multi_value": false, "nullable": false },
        "tags": { "scalar_type": "STRING", "is_multi_value": true, "nullable": true },
        "isPaid": { "scalar_type": "BOOLEAN", "is_multi_value": false, "nullable": true },
        "orderTime": { "scalar_type": "TIMESTAMP", "is_multi_value": false, "nullable": true },
        "amount": { "scalar_type": "DOUBLE", "is_multi_value": false, "nullable": true }
      }
    }
  },
  "queries": {}
}
```

### Tables

Tables are added by introspecting the database provided during `init`/`update` of the configuration directory. Most users don't need to further alter this configuration, but there are a couple of fields worth knowing about:

- **Table alias.** The key under `tables` (e.g. `"orders"` above) is the alias the table is exposed under in the graph. This can be renamed and must remain unique; it does not need to match the underlying Pinot table name.
- **`name`** is the real Pinot table name, without the `_OFFLINE`/`_REALTIME` type suffix Pinot appends internally.
- **`comment`** is an optional free-text description, surfaced as documentation in the NDC schema.
- **`primary_key`** is only set for Pinot [upsert tables](https://docs.pinot.apache.org/basics/data-import/upsert): it lists the table's `primaryKeyColumns`, as reported by the Controller's table config. Leave it `null` for non-upsert tables.
- **`columns`** maps each column name to its type, as reported by the Controller's `/tables/{table}/schema` endpoint. Pinot has no views/parameterized-table concept, so table configs carry no `arguments`, and there's no return-type-sharing mechanism between tables — every collection declares its own flat column set.

### Column types

Each entry in `columns` (and in a native query's `arguments`/`columns`) is a `PinotColumnType`:

```json
{ "scalar_type": "STRING", "is_multi_value": false, "nullable": true }
```

| Field | Default | Description |
| --- | --- | --- |
| `scalar_type` | — | One of Pinot's dimension/metric/dateTime data types: `INT`, `LONG`, `FLOAT`, `DOUBLE`, `BIG_DECIMAL`, `BOOLEAN`, `TIMESTAMP`, `STRING`, `JSON`, `BYTES`. Pinot's complex types (`STRUCT`, `MAP`, `LIST`) are folded into `JSON` on introspection, since they have no stable JSON round-trip guarantee in query results. |
| `is_multi_value` | `false` | Whether this is a Pinot "multi-value" (array) field. Multi-value columns are always flat arrays of scalars — Pinot has no concept of arrays-of-structs. |
| `nullable` | `true` | Whether the column may contain nulls. |

## Native queries

This connector supports native queries: hand-written Pinot SQL treated as read-only collections (virtual tables).

This is an alternative to writing views in Pinot, which is usually preferable when available, but native queries can be useful for ad hoc analytical queries or to iterate on a shape before it exists as a real table.

Native queries here are a lightweight mechanism: there's no SQL parsing or type inference. You write a plain `.sql` file with `{{argument_name}}` placeholders, and declare the argument types and the result column shape yourself.

Write the query as a `.sql` file in your configuration directory, typically in a dedicated subdirectory:

```sql
-- queries/OrdersByCustomer.sql
SELECT orderId, amount, orderTime
FROM orders
WHERE customerId = {{customerId}}
LIMIT 100
```

Placeholders are substituted with escaped SQL literals at query time — there's no need (and no support) for Pinot's own `?`/named-parameter binding syntax.

Then add the query to `configuration.json`:

```json
{
  "tables": {},
  "queries": {
    "ordersByCustomer": {
      "exposed_as": "collection",
      "comment": "Orders placed by a given customer",
      "file": "queries/OrdersByCustomer.sql",
      "arguments": {
        "customerId": { "scalar_type": "STRING", "is_multi_value": false, "nullable": false }
      },
      "columns": {
        "orderId": { "scalar_type": "LONG", "is_multi_value": false, "nullable": false },
        "amount": { "scalar_type": "DOUBLE", "is_multi_value": false, "nullable": true },
        "orderTime": { "scalar_type": "TIMESTAMP", "is_multi_value": false, "nullable": true }
      }
    }
  }
}
```

A few rules:

- `exposed_as` is currently always `"collection"`.
- Every argument declared in `arguments` must appear as a `{{placeholder}}` somewhere in the referenced `.sql` file.
- `columns` describes the shape of a single result row exactly as you'd describe a table's columns — the connector does not infer it from the query.
- A collection name (the key under `queries`) may not collide with a key under `tables`.
