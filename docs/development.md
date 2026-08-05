# Development

This document details how to build and test this connector locally, and how to run the manual local-testing stack for exercising it end to end against a real Pinot cluster.

## Prerequisites

- [Rust Toolchain](https://www.rust-lang.org/tools/install)
- [Docker](https://docs.docker.com/get-docker/) (for the local-testing stack)
- [Hasura DDN CLI](https://hasura.io/docs/3.0/cli/installation) (optional, only needed to wire the connector into a DDN project)

## Clone the repository

```sh
git clone https://github.com/hasura/ndc-pinot.git
cd ndc-pinot
```

All subsequent commands assume they're run from the repository root.

## Building

```sh
cargo build
```

This builds the workspace: the `ndc-pinot` server binary, the `hasura-pinot` CLI binary, and the `ndc-pinot-core`/`common` library crates.

## Testing

```sh
cargo test --all-features
```

Formatting is checked separately, and is expected to already be clean before you push:

```sh
cargo fmt --check
```

CI (see `.github/workflows/`) runs exactly these two commands — `cargo test --all-features` and `cargo fmt --check` — on every pull request. It does **not** spin up the Docker Compose stack described below; that stack is a manual, local-only convenience for exercising the connector against a real Pinot cluster and Hasura engine, not part of the automated test suite.

## Running the manual local-testing stack

A `docker-compose.yaml` at the repository root brings up a full local stack: a Pinot cluster, this connector, the Hasura v3 engine, and supporting services. It's intended for manual, interactive testing during development — use `cargo test --all-features` for anything that needs to run unattended or in CI.

The stack is made up of:

| Service | Purpose |
| --- | --- |
| `zookeeper` | Coordination service required by the Pinot cluster. |
| `pinot-controller` | Pinot Controller — manages cluster metadata, table/schema definitions. |
| `pinot-broker` | Pinot Broker — the query entry point this connector talks to. |
| `pinot-server` | Pinot Server — stores segments and executes queries. |
| `ndc-pinot` | This connector, built from the local source tree. |
| `engine` | The Hasura v3 engine, serving the GraphQL API on top of the connector. |
| `auth-hook` | A dev auth webhook used by the engine for local testing. |
| `jaeger` | Collects traces emitted by the connector and engine for local debugging. |

Table and segment setup for the local Pinot cluster is seeded by the scripts under [`tests/pinot-db/init/`](../tests/pinot-db/init/), which run against the Controller once it's up.

Start the stack:

```sh
docker compose up -d
```

The first build may take a while; subsequent builds are faster thanks to layer caching.

To restart the connector (required for changes to `configuration.json` to take effect):

```sh
docker compose restart ndc-pinot
```

To rebuild the connector (required for changes to connector source code to take effect):

```sh
docker compose up -d --build ndc-pinot
```

See also: [configuration instructions](./configuration.md) for how `configuration.json` and the `PINOT_CONTROLLER_URL`/`PINOT_BROKER_URL`/`PINOT_AUTH_HEADER` environment variables fit together, and the [Hasura V3 documentation](https://hasura.io/docs/3.0) for wiring a running connector into a DDN project.
