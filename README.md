# caddy-msi
[![Build Status](https://github.com/CaddyGlow/rust-msi/actions/workflows/tests.yml/badge.svg)](https://github.com/CaddyGlow/rust-msi/actions/workflows/tests.yml)
[![Crates.io](https://img.shields.io/crates/v/caddy-msi.svg)](https://crates.io/crates/caddy-msi)
[![Documentation](https://docs.rs/caddy-msi/badge.svg)](https://docs.rs/caddy-msi)

A pure Rust library for reading/writing [Windows
Installer](https://en.wikipedia.org/wiki/Windows_Installer) (MSI) files.

Documentation: https://docs.rs/caddy-msi

Fork of [mdsteele/rust-msi](https://github.com/mdsteele/rust-msi), retaining
its public API and upstream attribution. See [LOCAL-CHANGES.md](LOCAL-CHANGES.md)
for patch provenance. Requires Rust 1.99 or newer.

```toml
[dependencies]
msi = { package = "caddy-msi", version = "0.10.2" }
```

## Optional MSI media support

Enable `features = ["media"]` for bounded cabinet preparation, emission and
embedded/external/loose payload extraction using ms-cabinet 0.1.4. The default
build remains database-only. `msi::media` exposes typed table relationships and
limits without enabling cabinet I/O. Resolvers and sinks are explicit; the
library never discovers host files or runs an installer.

See [the standalone example](examples/media_authoring.rs), [the refactoring plan](docs/msi-media-refactoring-plan.md)
and [qualification](docs/media-refactoring-validation.md). Installer profiles,
GUID policy and final package publication remain caller responsibilities.

## License

rust-msi is made available under the
[MIT License](http://spdx.org/licenses/MIT.html).


Media authoring code adapted from ms-package retains its MIT notice in
[licenses/ms-package/LICENSE](licenses/ms-package/LICENSE).
