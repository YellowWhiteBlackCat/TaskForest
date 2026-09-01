# taskmanager-test-support

## Role

Dev-only typed fixture assembly for cross-crate behavior tests. The crate
depends on `taskmanager-core` and exposes builders that write canonical
observations through named domain assembly. It is never a product dependency.
`pin_english()` pins the shared i18n global to English for tests that assert
catalog strings — the app correctly follows the host locale, so such tests
must not; every consumer depends on this crate only through dev-dependencies.

## Boundary

Schema-v1 field names, sentinel hydration, OS I/O, and source-text assertions
do not belong here. Old payload compatibility is tested directly against the
private serde boundaries in the owning domain crate.

Fixture values are composed per consumer: builders expose named setters, not
frozen canonical snapshots (`rich_memory()`-style value presets are rejected
— every real consumer asserts its own distinct values, so a preset would only
create an unused parameter surface). A shared value spec may be added only
when a consumer demonstrably needs value-agnostic rich fixtures.

## Contract and verification

Builders must use the same typed constructors and apply operations as production
providers. They must not recreate writable mirrors or a second domain model.
Observation assembly is staged at compile time: a builder may install an
optional whole-group base once, then apply named overrides. The first named
override closes the base stage, so a later whole-group replacement is not a
legal method call. The process scalar and metadata groups advance independently.

Builder inventory: `DiskMetricsFixtureBuilder` and
`DiskPartitionFixtureBuilder` (disk rows/partitions), `NetworkMetricsFixtureBuilder`
(network rows), `ProcessItemFixtureBuilder` (process rows), and
`MemoryMetricsFixtureBuilder` (memory rows — both whole-group bases stay legal
until the first named override; `from_item` keeps the prior groups so a fixture
can layer one family, e.g. the zram `mm_stat` depth, onto an existing snapshot).

Check with `cargo check -p taskmanager-test-support --all-targets` and its
downstream behavior tests.

## Module map

```text
src/lib.rs                     dev-only typed fixture builders
src/memory.rs  metrics.rs  process.rs
```

Consumed only through dev-dependencies; never a product dependency.
