# taskmanager-telemetry-store

## Role

Platform-neutral bounded in-memory telemetry history and correlated read model.
`LiveGraphHistory` is the single renderer-neutral live-chart authority consumed
by Shell, Iced and TUI; GPUI's direct track reads the same store semantics.

## Boundary

The store accepts validated facts with identity, generation, revision and gap
metadata. It does not read OS sources, schedule providers, persist implicitly,
or format toolkit-specific charts. The write handle is separate from the cloned
read model so frontends cannot create a competing append path.

## Contract and verification

History preserves gaps, stale transitions, revision and device lifecycle while
enforcing memory bounds. A healthy authoritative battery or sensor enumeration
retires histories for identities no longer present; unavailable/partial source
states never masquerade as removal. Verify append/query/revision behavior,
retirement and generation isolation without relying on live hardware.

Ingestion enters only through named transaction inputs.

## Module map

```text
src/retention.rs                 physical host/detail budgets
src/live_graph.rs                revision-keyed immutable series; per-device reads must
│                                carry generation (double-sided discipline)
src/system_history.rs
├── ingest.rs                    CorrelatedSystemTelemetryIngestor: the only write entry
│   └── ingest/{dynamic,storage_network}.rs
├── device.rs  dynamic_history.rs  gpu.rs   per-device curves and dynamic history
```

Writers: composition-owned ingestor only. Readers: frontend replay clients (read-only).

## Docs

- [`docs/RETENTION_BUDGETS.md`](docs/RETENTION_BUDGETS.md) — physical budgets, dynamic
  identity ceilings and the persistence commit contract.
- [`docs/INGESTION_RULES.md`](docs/INGESTION_RULES.md) — per-domain ingestion and
  generation-keyed read rules, including split rings.
- [`docs/SERIES_RESOLUTION.md`](docs/SERIES_RESOLUTION.md) — the `SeriesScope` model and
  typed wrong-domain errors.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) and
  [ADR-014](../../adr/014-correlated-telemetry-history-and-refresh-policy.md) — history
  and refresh policy.
