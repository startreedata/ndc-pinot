# Pinot NDC Usage

This native data connector implements the following Hasura Data Domain Specification features:

| Feature | Support |
| --- | --- |
| [Simple Queries](https://hasura.io/docs/3.0/graphql-api/queries/simple-queries/) | ✅ |
| [Query Filter: Value Comparison](https://hasura.io/docs/3.0/graphql-api/queries/filters/comparison-operators/) | ✅ |
| [Query Filter: Boolean Expressions](https://hasura.io/docs/3.0/graphql-api/queries/filters/boolean-operators/) | ✅ |
| [Query Result Sorting](https://hasura.io/docs/3.0/graphql-api/queries/sorting/) | ✅ |
| [Query Result Pagination](https://hasura.io/docs/3.0/graphql-api/queries/pagination/) | ✅ |
| [Aggregate Queries](https://hasura.io/docs/3.0/graphql-api/queries/aggregates/) — `count`, `count(distinct)`, `sum`, `avg`, `min`, `max` | ✅ |
| [Variables, Aliases, Fragments, Directives](https://hasura.io/docs/3.0/graphql-api/queries/variables-aliases-fragments-directives/) | ✅ |
| [Explain](https://hasura.io/docs/3.0/graphql-api/queries/explain/) | ✅ |
| Native Queries | ✅ |
| [Relationships](https://hasura.io/docs/3.0/data-modeling/relationships/) (connector-level joins) | ❌ (v1 — see [README limitations](../README.md#scope--limitations-v1); use engine-level remote joins instead) |
| [Mutations](https://hasura.io/docs/3.0/graphql-api/mutations/) | ❌ (v1 — Pinot is read-only from this connector's perspective; see [README limitations](../README.md#scope--limitations-v1)) |
| [Query Filter: Nested Objects](https://hasura.io/docs/3.0/graphql-api/queries/filters/nested-objects/) | ❌ (v1) |
| Nested-field ordering | ❌ (v1) |
| Nested-field aggregation | ❌ (v1) |
| [Nested Field Selection](https://hasura.io/docs/3.0/graphql-api/queries/nested-fields/) | ❌ (v1) |

`min`/`max` are additionally available on `STRING` columns; `sum`/`avg` are only available on numeric columns (`INT`, `LONG`, `FLOAT`, `DOUBLE`, `BIG_DECIMAL`, `TIMESTAMP`). `count` and `count(distinct)` are available on every column.

Aggregate queries and row (`fields`) queries within the same request are each executed as their own Pinot SQL statement — see the `PlannedQuery` doc comment in `ndc-pinot-core` for why a single combined statement isn't used.
