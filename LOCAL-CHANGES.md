# MSI parser fork

Based on upstream `mdsteele/rust-msi` commit
`3a7e4a00b344e909ebba86629a19b9a6b284db51` (msi 0.10.0).
The upstream MIT license and public APIs are retained.

Required schema metadata cells are checked instead of unwrapped. Malformed
`_Tables`, `_Columns`, and `_Validation` rows return `InvalidData` rather than
panicking. This preserves the patch previously vendored by `ms-package`.

The unchanged synthetic fuzz finding from 2026-10-05 is retained as
`tests/data/msi-null-column-type.msi`, with provenance and SHA-256 in
`tests/data/README.md`. `tests/malformed_metadata.rs` checks that the parser
returns `InvalidData` without panicking.

Clippy compatibility: table row counting uses checked division, retaining
zero rows for zero-width tables. The signature test fixture uses `write_all`
so partial writes cannot silently truncate its stream contents.
