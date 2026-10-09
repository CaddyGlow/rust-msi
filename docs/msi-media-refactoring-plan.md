# caddy-msi reusable media refactoring plan

Status: implemented for caddy-msi 0.10.2; release qualification recorded in
[media-refactoring-validation.md](media-refactoring-validation.md).

## Objective and boundaries

Own reusable MSI media semantics in this fork: File sequences, Media boundaries,
compressed/uncompressed flags, cabinet references, embedded streams and explicit
external/loose media access. Consumers must be able to use this without
ms-package. Cabinet algorithms remain in ms-cabinet; installer authoring profiles,
GUID policy, aggregate package reports and final package publication stay with
callers. Preserve existing database and FFI APIs and licensing notices.

## API audit and design

Inventory the reusable logic currently in ms-package installer_media.rs,
installer_builder.rs and installer.rs. Separate MSI representation constraints
from its flat ASCII, one-feature and one-file-per-component profile restrictions.

Add typed backend-owned layouts, sequence inputs, bounded validated plans,
limits and errors. Keep cabinet dependencies behind an optional media feature;
audit whether pure table planning can be available without that feature.
Preserve the default database-only dependency graph. No ms-package dependency,
automatic filesystem lookup, temporary files or installer execution is allowed.

Planning validates complete ordered sequence coverage, LastSequence boundaries,
reference/stream consistency, schema ranges, media names and collisions before
opening any destination. Keep validated plans immutable during execution.
Explicit sink/resolver contracts bound actual I/O, not only declared sizes.

## Implementation batches

1. Establish direct baseline tests from all supported ms-package media layouts.
2. Implement reusable typed layout/sequence planning with independent schema and
   reference validation; keep current database serialization unchanged.
3. Integrate optional ms-cabinet serialization and extraction. Keep borrowed
   payloads where possible and account for framing/codec and retained buffers.
4. Implement explicit media emission: write_all, flush, then caller finalization.
   Record completion only after finalization succeeds. Errors retain completed
   artifacts, failing name, actual partial bytes and the original cause.
5. Add bounded embedded/external/loose reading and interpretation. Keep missing
   media explicit; never discover host paths implicitly. Do not admit unsupported
   spanning/signature structures as a side effect of the migration.
6. Provide a standalone example without ms-package, then test package adapters
   through an isolated development override. Publish an audited compatible fork
   version before consumers switch to its registry dependency.

## Validation

Test invalid/duplicate/missing sequences, empty partitions, overflow, cabinet
references, stream naming, loose resolution, path collisions and exact limits.
Exercise failing resolvers, short/partial writes, flush and finalization failures.
Preserve string-pool physical-key ordering and every historical parser regression.
Run rustfmt, locked fork workspace tests (including FFI), strict all-target/all-
feature Clippy, database-only builds and WASM builds of the optional media path.

Coordinate 63-artifact byte equivalence, the qualified 18-case native lifecycle
matrix and native/Worker parity with ms-package before publication. New
serialization differences require investigation and fresh native qualification.
Keep source revisions, registry checksums, licenses and evidence intact.

## Completion

Media operations work and have direct regressions without ms-package; database
and FFI compatibility hold; package adapters pass their independent gates; the
new backend is published with registry provenance. No general installability,
upgrade behavior or new cabinet/compression profile is implied.
