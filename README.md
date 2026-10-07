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
msi = { package = "caddy-msi", version = "0.10.0" }
```

## License

rust-msi is made available under the
[MIT License](http://spdx.org/licenses/MIT.html).

