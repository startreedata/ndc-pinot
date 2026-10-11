# Development

This document details how to build and test this connector locally, and how to run the manual local-testing stack for exercising it end to end against a real Pinot cluster.

## Prerequisites

- [Rust Toolchain](https://www.rust-lang.org/tools/install)
- [Docker](https://docs.docker.com/get-docker/) (for the local-testing stack)
- [Hasura DDN CLI](https://hasura.io/docs/3.0/cli/installation) (optional, only needed to wire the connector into a DDN project)

## Clone the repository

```sh
git clone https://github.com/startreedata/ndc-pinot.git
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

`cargo test --all-features` is a pure unit/snapshot suite: it exercises SQL generation, schema derivation, and config handling in isolation and never contacts a real Pinot cluster.

## Integration test (end-to-end)

In addition to the unit suite, there's an end-to-end integration test that stands up a real Pinot cluster, seeds the sample `orders` table, runs the connector against it, and drives real NDC queries through the connector's HTTP API — proving the full path `NDC request → generated Pinot SQL → Pinot → mapped NDC response`, not just that SQL generation is correct in isolation.

The assertions live in a single dependency-light script ([`ci/integration/smoke-test.sh`](../ci/integration/smoke-test.sh), just `bash` + `curl` + `jq`) so the same checks run in CI and locally. The script is self-gating: it waits for the connector's `/health` and for the seeded rows to actually be queryable before asserting anything, so you can launch it right after backgrounding the connector — it won't race connector startup or Pinot's segment-load lag.

To run it locally against the Docker Compose stack:

```sh
# 1. bring up the Pinot cluster (constrained JVM heaps so it fits a laptop/CI runner)
docker compose -f docker-compose.yaml -f docker-compose.ci.yaml up -d \
  zookeeper pinot-controller pinot-broker pinot-server

# 2. seed the sample orders table (waits for the controller first)
./tests/pinot-db/init/seed.sh

# 3. start the connector against the cluster.
#    HASURA_CONNECTOR_PORT=5000 makes the connector listen on the port the
#    smoke test targets by default (the ndc-sdk's own default is 8080).
HASURA_CONFIGURATION_DIRECTORY="$PWD/tests/config" \
HASURA_CONNECTOR_PORT=5000 \
PINOT_CONTROLLER_URL=http://localhost:9000 \
PINOT_BROKER_URL=http://localhost:8099 \
  cargo run --package ndc-pinot -- serve &

# 4. run the smoke test (it waits for the connector + seeded data to be ready)
./ci/integration/smoke-test.sh
```

> If port 5000 is taken (on macOS, Control Center's AirPlay Receiver listens
> there), start the connector on another port and point the smoke test at it,
> e.g. `HASURA_CONNECTOR_PORT=5055 … serve &` then
> `CONNECTOR_URL=http://localhost:5055 ./ci/integration/smoke-test.sh`.

## CI

CI includes the following workflows (see [`.github/workflows/`](../.github/workflows/)):

| Workflow | What it runs |
| --- | --- |
| `build_and_test.yaml` | `cargo test --all-features` and `cargo fmt --check` — fast, no Docker. |
| `integration_test.yaml` | The end-to-end integration test above: brings up the Pinot cluster via Docker Compose, waits for a server to register, seeds it, starts the connector, and runs `ci/integration/smoke-test.sh`. The smoke test also drives a `useMultistageEngine=true` join straight against the broker, which confirms the stack is a genuine distributed cluster (a join has to be planned across the server) rather than a single-process shim. |
| `release.yaml` | On a version tag, runs formatting and unit tests, builds the CLI for Linux/macOS (AMD64 and ARM64) and Windows (AMD64), publishes a multi-platform connector image to GHCR, and publishes the Hasura connector definition, CLI manifest, binaries, and checksums as a GitHub release. |

The `docker-compose.ci.yaml` override used by the integration workflow shrinks each Pinot JVM's heap (the base image defaults to `-Xms4G -Xmx4G` per process, which OOMs a standard CI runner when a controller, broker and server all start at once). It's a drop-in override — layer it on top of the base file with a second `-f` as shown above.

### Publishing a release

Set `workspace.package.version` in `Cargo.toml` to the release version, run `cargo check` to update `Cargo.lock`, and commit both files. Tag the merged commit:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The tag must match the workspace version; prerelease tags such as `v0.2.0-rc.1` are supported and create GitHub prereleases. The workflow uses Rust `1.88.0`, matching the Dockerfile, and the repository's `GITHUB_TOKEN` with job-specific release/package write permissions. Native AMD64 and ARM64 runners build the Linux images, then combine their digests into `ghcr.io/startreedata/ndc-pinot:v0.1.0`. The combined image digest is pinned in `connector-metadata.yaml` inside `connector-definition.tgz`. Make the GHCR package public after its first publication so Hasura users can pull it without credentials.

To retry a failed release, rerun its workflow from the Actions tab. If an asset upload failure left a draft GitHub release, delete that draft before retrying. To use an updated workflow without moving an existing release tag, dispatch the workflow from `main` with its `tag` input (for example, `gh workflow run release.yaml --ref main -f tag=v0.1.0`). All release sources are checked out at the verified tag commit, including the CLI, image, and packaging templates.

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
