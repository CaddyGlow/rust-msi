# MSI media refactoring qualification — 2026-10-09

Candidate: caddy-msi 0.10.2, ms-cabinet 0.1.4 and ms-package 0.2.2.
The implementation follows [the requested plan](msi-media-refactoring-plan.md).

The locked full workspace tests, including FFI, all-feature doctests, strict
all-target/all-feature Clippy, rustfmt, database-only workspace check and optional
media wasm32-unknown-unknown library check passed. Direct media tests cover
sequence coverage, empty boundaries, overrides, schema/stream limits, explicit
resolvers, source bounds, cabinet errors and partial emission/finalization.
Existing database/parser fixtures and licensing notices remain intact.

Consumer qualification compared all 63 MSI/media artifacts from 18 baseline and
edited cases with the previous implementation: no byte differences. All 18
Windows install/delete/repair/uninstall lifecycles passed; all 28 Windows Rust
test executables from both workspaces passed. Both archive Worker checks and
fuzz smoke passed; each of three instrumented fuzz targets completed 129
iterations with no crashes or timeouts.

Portable raw consumer evidence is retained in the ms-package repository under
`docs/evidence/msi-media-refactor-20261009/`, with the report
`docs/msi-media-refactoring-validation-20261009.md`. No new compression profiles,
spanning support, signature verification, general installability or upgrade
behavior is claimed. Reserved cabinet fields remain readable without a trust
claim. The FFI crate is not published.
