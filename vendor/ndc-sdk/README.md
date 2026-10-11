# NDC SDK telemetry backport

This library is copied from `crates/sdk/src` in Hasura ndc-sdk-rs v0.5.2,
commit `91ebe56cfb61a59db379e7495d43b5058d5d5257` (Apache-2.0).
The upstream example binary is omitted. `ndc-sdk-core` and NDC models remain
pinned to upstream v0.5.2 and v0.1.6, respectively.

Only the manifest, tracing implementation, and tracer shutdown are adapted
for patched OpenTelemetry and Rustls dependencies. Keep the upstream HTTP
routes, CLI, authentication, and NDC 0.1.6 behavior when updating this copy.
Endpoint precedence, HTTP trace paths, seconds-based timeout environment
variables, native certificate roots, propagation, and clean shutdown follow
v0.5.2 behavior. The original crate version remains 0.5.0; it is not a new upstream release.

Validate from the repository root with `cargo test --workspace --all-features
--locked` and `cargo fmt --manifest-path vendor/ndc-sdk/Cargo.toml --check`.
