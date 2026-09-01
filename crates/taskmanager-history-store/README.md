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
independent collector's app-host-composed writer generation. A query concurrent with an
append is a request-local snapshot: a transient incomplete tail is never
persisted as corruption state, and a later query re-reads the completed file.

## Contract and verification

Retention, quota, clock jumps, atomic rewrite, corruption and multi-instance
locking are typed contracts. Verify store/query behavior independently from
the live telemetry runtime and keep the real sustained-run receipt open until
it exists. Pending samples, payload bytes, identities, persisted file count,
total directory-scan work, per-file reads and boot-history reads have explicit
global ceilings. Limit rejection is typed and observable; bounded
backpressure keeps newest arrivals.
TTL or quota retirement deletes empty/old whole series and releases revision
guards unless a concurrent pending sample still owns that key. Quota may retire
the oldest complete series when minimum one-line files cannot fit, so
`max_bytes` is an actual post-flush bound rather than a best-effort target.
Single-writer claims fail closed on unreadable or malformed ownership and are
released only while the stored token still matches the exact owner.
The read-only claim probe distinguishes absent/live/stale/ambiguous without
acquiring or replacing ownership; frontend startup uses it only for a bounded
collector handshake.

## Module map

```text
src/store.rs (+ store/{lock,pending,tmp_sweep,retention_io}.rs)   lock, pending, sweeps
src/records.rs                   typed history records (JSONL)
src/query.rs                     read-only queries
src/retention.rs  boot_history.rs  bounded_io.rs   retention, boot segments, bounded I/O
```

Single writer: the app-host persistence generation; all other consumers are read-only.
