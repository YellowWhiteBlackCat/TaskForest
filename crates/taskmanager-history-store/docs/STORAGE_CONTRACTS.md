# Storage contracts

Retention, quota, clock jumps, atomic rewrite, corruption and multi-instance
locking are typed contracts. Pending samples, payload bytes, identities,
persisted file count, total directory-scan work, per-file reads and
boot-history reads have explicit global ceilings. Limit rejection is typed and
observable; bounded backpressure keeps newest arrivals.

TTL or quota retirement deletes empty/old whole series and releases revision
guards unless a concurrent pending sample still owns that key. Quota may
retire the oldest complete series when minimum one-line files cannot fit, so
`max_bytes` is an actual post-flush bound rather than a best-effort target.

## Locking

Single-writer claims fail closed on unreadable or malformed ownership and are
released only while the stored token still matches the exact owner.

The read-only probe reports a held OS lock independently from free-file
absent/live/stale/ambiguous claims. A bounded shared read lease permits claim
inspection without becoming a writer; mandatory locking never fabricates a
PID.

## Segments and ingress

Encoded stems reserve the ordinary-file suffix within the portable component
limit; rewrite temporaries use independent short names. Read ingress
privately indexes published external locations by the current typed key,
combines segments under one byte ceiling and retains corruption evidence.
Retiring one segment releases revision guards only when no segment or pending
sample remains.

## Concurrency

Production writes, flush cadence and the single-writer lock are owned by the
frontend session's app-host-composed writer generation. A query concurrent
with an append is a request-local snapshot: a transient incomplete tail is
never persisted as corruption state, and a later query re-reads the completed
file.
