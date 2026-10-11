# Pinot Connector

The [Apache Pinot](https://pinot.apache.org/) Native Data Connector allows you to connect to a Pinot cluster (self-hosted, or [StarTree Cloud](https://www.startree.ai/)) and get an instant GraphQL API on top of your Pinot data.

This connector is built on the [Rust Data Connector SDK](https://github.com/hasura/ndc-hub#rusk-sdk) from the [Data Connector Hub](https://github.com/hasura/ndc-hub) and implements the [Data Connector Spec](https://github.com/hasura/ndc-spec).

- [Hasura V3 Documentation](https://hasura.io/docs/3.0)

In order to use this connector you will need:

- Access to an Apache Pinot cluster's Controller and Broker REST APIs (self-hosted, or StarTree Cloud)
- A [Hasura CLI](https://hasura.io/docs/3.0/cli/overview/) session

## For Hasura Users

This connector should be used via the Hasura DDN CLI.

See the [configuration instructions](./docs/configuration.md) for details on `configuration.json`, environment variables, and native queries.

## For Developers

See the [development instructions](./docs/development.md) for how to build, test, and run this connector locally.

## Documentation

See which Hasura OpenDD Spec features are supported by the connector [here](./docs/usage.md).

View other documentation for the Pinot connector [here](./docs/index.md).

## Scope & limitations (v1)

This connector is intentionally narrow in scope for v1. The limitations below are deliberate design decisions, not gaps to be closed opportunistically — read the reasoning, not just the "no":

- **No relationships / joins.** Apache Pinot's SQL engine has limited, non-correlated `JOIN` support and no equivalent of the correlated subqueries a single-statement join resolution strategy would need to resolve NDC relationships inside one query. Rather than emulate relationships with fragile N+1 query stitching, this connector declares no relationship capability. Model cross-collection joins as Hasura engine-level [remote joins](https://hasura.io/docs/3.0/data-modeling/relationships/) across separate connector queries instead.
- **No mutations.** Pinot is a read-only OLAP query engine from this connector's perspective. Data gets into Pinot tables through ingestion pipelines (batch or streaming), not through SQL `INSERT`/`UPDATE`/`DELETE` — so this connector declares no mutation capability.
- **No nested-field filtering, ordering, or aggregation.** Selecting, filtering, sorting, or aggregating by fields nested inside a struct/object column isn't supported. Pinot's own complex types (`STRUCT`, `MAP`, `LIST`) are surfaced as opaque `JSON` scalars rather than structured object types, so there's nothing to nest into in the first place.
- **`TIMESTAMP` columns are opaque strings.** Pinot serializes `TIMESTAMP` values in query results as `yyyy-MM-dd HH:mm:ss.S`-style strings by default, not ISO-8601. To avoid implying a formatting guarantee this connector can't back, `TIMESTAMP` columns are exposed with a plain `String` type representation rather than NDC's `Timestamp` representation.
- **`BOOLEAN` columns are coerced back to real booleans.** Pinot's query result serialization has no native JSON boolean for `BOOLEAN` columns — it returns the strings `"true"`/`"false"`. This connector coerces those strings back into real JSON `true`/`false` values so callers see the type they declared, rather than passing the string through as-is.
- **Native queries are a lightweight escape hatch, not a SQL dialect.** Native queries are plain `.sql` files with `{{argument_name}}` placeholders that get substituted with escaped SQL literals at query time. There's no query parsing or type inference — you declare the argument types and the result columns yourself in `configuration.json`.

See [`docs/usage.md`](./docs/usage.md) for the full feature-support checklist.

## License

Licensed under the [Apache License, Version 2.0](./LICENSE).
