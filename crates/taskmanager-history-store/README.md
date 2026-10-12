# taskmanager-history-store

## Role

Opt-in persistent telemetry history using bounded JSONL and serde-only data
structures.

## Boundary

Paths and clocks are injected at composition. The crate performs bounded
storage I/O but no platform-fact collection, provider selection, UI work or
implicit persistence; default configuration is off and rejected observations
never reach the sink.

`HistoryQuery` is a storage primitive, not a frontend port. Production replay
queries are owned by `taskmanager-app-host`'s bounded worker; renderers receive
only application request/completion values.

Production writes, flush cadence and the single-writer lock are owned by the
frontend session's app-host-composed writer generation. A query concurrent with an
append is a request-local snapshot: a transient incomplete tail is never
persisted as corruption state, and a later query re-reads the completed file.

## Contract and verification

Retention, quota, clock jumps, atomic rewrite, corruption and multi-instance
locking are typed contracts. Verify store/query behavior independently from
the live telemetry runtime and keep the real sustained-run receipt open until
it exists. Pending samples, payload bytes, identities, persisted file count,
total directory-scan work, per-file reads and boot-history reads have explicit
global ceilings; limit rejection is typed and observable, and bounded
backpressure keeps newest arrivals. Ceiling, lock, segment and concurrency
rules are specified in [`docs/STORAGE_CONTRACTS.md`](docs/STORAGE_CONTRACTS.md).

## Module map

```text
src/store.rs (+ store/{lock,pending,tmp_sweep,retention_io}.rs)   lock, pending, sweeps
src/records.rs                   typed history records (JSONL)
src/query.rs                     read-only queries
src/retention.rs  boot_history.rs  bounded_io.rs   retention, boot segments, bounded I/O
```

Single writer: the app-host persistence generation; all other consumers are read-only.

## Docs

- [`docs/STORAGE_CONTRACTS.md`](docs/STORAGE_CONTRACTS.md) — retention/quota ceilings,
  single-writer locking, segment ingress and snapshot-read concurrency.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) and
  [ADR-036](../../adr/036-in-process-history-persistence.md) — state ledger and in-process
  history persistence.
