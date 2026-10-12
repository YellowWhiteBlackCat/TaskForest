# Builder rules

## Staging

Observation assembly is staged at compile time: a builder may install an
optional whole-group base once, then apply named overrides. The first named
override closes the base stage, so a later whole-group replacement is not a
legal method call. The process scalar and metadata groups advance
independently.

## Composition discipline

Fixture values are composed per consumer: builders expose named setters, not
frozen canonical snapshots (`rich_memory()`-style value presets are rejected
— every real consumer asserts its own distinct values, so a preset would only
create an unused parameter surface). A shared value spec may be added only
when a consumer demonstrably needs value-agnostic rich fixtures.

Builders must use the same typed constructors and apply operations as
production providers. They must not recreate writable mirrors or a second
domain model.

## Builder inventory

- `DiskMetricsFixtureBuilder` / `DiskPartitionFixtureBuilder` — disk rows and
  partitions.
- `NetworkMetricsFixtureBuilder` — network rows.
- `ProcessItemFixtureBuilder` — process rows.
- `MemoryMetricsFixtureBuilder` — memory rows; both whole-group bases stay
  legal until the first named override, and `from_item` keeps the prior
  groups so a fixture can layer one family (e.g. the zram `mm_stat` depth)
  onto an existing snapshot.

## Out of scope

Schema-v1 field names, sentinel hydration, OS I/O, and source-text assertions
do not belong here. Old payload compatibility is tested directly against the
private serde boundaries in the owning domain crate.

```bash
cargo check -p taskmanager-test-support --all-targets
cargo nextest run -p taskmanager-test-support -j 4
```
