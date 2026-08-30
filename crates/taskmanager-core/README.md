# taskmanager-core

## Role

Platform-neutral domain facts, availability semantics, identity/lifecycle,
history, configuration and pure shared rules.

## Boundary

Core owns no filesystem, process, command, window, widget, OS path or native
handle, and it never reads the wall or monotonic clock. Time-dependent rules
receive an explicit timestamp; `time::unix_millis`/`unix_micros` only convert
an injected `SystemTime`. Business code remains `#![forbid(unsafe_code)]`; platform extensions
enter through typed capability models and `Unsupported`/permission outcomes.

`time::LocalTimeRules` validates bounded TZif into typed offset/DST transitions.
Variable rules carry an explicit last-transition horizon; a projection beyond
that horizon is unavailable because POSIX-footer recurrence is not yet parsed.
`LocalTimeRulesObservation` distinguishes current rules from provider failure;
its cache key holds the cheap-cloned full rule value, so semantic equality is
the only reuse and change authority.

## Key modules

- `src/core/metrics/` owns domain measurements and availability joins.
- `src/core/process/`, `src/core/services/`, `src/core/startup/` and
  `src/core/export/` own typed product facts.
- `src/core/process/identity.rs` owns the validated `ProcessLiveKey` used to
  distinguish a provider-issued process incarnation from a reused PID;
  `src/core/process/aggregate.rs` owns availability-preserving scalar folds;
  `src/core/process/group_aggregate.rs` owns the typed application, user, and
  process-class group aggregates whose totals never collapse unavailable
  members into a fabricated value.
- `src/core/alerts/` owns alert rules, active-set transitions and the bounded,
  versioned event export; event history is session-local domain state, not a
  renderer cache.
- `src/core.rs` is the owner module index and `src/lib.rs` contains only the
  core crate's own aggregate API, never a cross-layer forwarding facade;
  consumers should prefer the explicit domain module that owns each fact.
  Pure behavior belongs in nearby tests.

## Contract and verification

`ScalarObservation`, stable identity, generation, gap-aware history,
formatters, filters and control vocabulary are canonical here. Keep pure
functions deterministic and verify with core tests, wire invariants and the
platform boundary firewall. Clock tests use fixed supplied instants rather than
host-time plausibility. Notification cooldown identity is retained only
for the interval in which it can still affect a verdict. Providers may report
their full CPU fact vector, while every derived per-core history fan-out shares
the canonical `MAX_TRACKED_LOGICAL_CPUS` cardinality ceiling.

Core changes are hard cutovers. A new typed contract requires all production
callers, renderer state, tests, fixtures, demo/capture paths and exports to move
in the same change; old aliases, wrappers, deprecated methods, fallback APIs and
dual semantic paths are deleted. A private decoder may parse a mandatory
published external payload and immediately canonicalize it, but no consumer may
depend on that decoder or observe the old vocabulary.

Generic scalar, optional, grouped and process-metadata observations keep their
value/state, availability and success-time fields private. Production writers
use named constructors and transitions; consumers choose explicit current or
last-known accessors. Partial scalar groups accept only typed current, partial
or unavailable slots and stamp all current slots atomically, so callers cannot
assemble Unknown/Stale children or mismatched refresh times. External old-format
hydration exists only at a private serde ingress and is immediately converted to
the current typed observations.

`ServiceDeps` and `ServiceItem` each own one private typed
`ServiceRelationGraph`; consumers receive only read-only relation accessors.
Their private serde ingress/serializer handles four published dependency-string
fields; the domain only stores the typed relation graph. Input is canonicalized
immediately, and unknown typed relation kinds round-trip unchanged.

`BatteryInfo` owns a private `BatteryScalarObservations` group exposed through
typed read accessors and one apply operation. Its four schema-v1 scalar options
live only in `BatteryInfoWire`: trusted legacy rows hydrate Unknown observations,
while only currently Available typed values project back to those keys.

`CpuMetrics` owns one private `CpuScalarObservations` group; its typed per-core
groups are the only per-core authority. `MemoryMetrics` likewise owns private
scalar and optional observation groups that are replaced and retained
atomically. Their private wire DTOs hydrate legacy values only into Unknown
truth behind CPU identity/topology or a positive memory denominator, and emit
legacy success keys only for currently Available observations. Confirmed-empty
CPU groups remain distinct from failed groups, and memory percentage accessors
return `Option` instead of collapsing an unknown denominator to zero.

Disk/partition and network schema-v1 numbers, `transport`/`removable`,
`is_wireless`, SSID and signal live only in private wire DTOs. Domain rows
expose read-only typed observation, connection, attachment-capability and
adapter-class accessors. Providers use named typed apply operations; failed
facts never serialize as zero, empty or false success.
Cross-crate fixtures live in the dev-only `taskmanager-test-support` crate and
assemble canonical observations through named builders. Core exports no
fixture DSL or test mutation surface to product dependencies.

`GpuMetrics` owns private `GpuScalarObservations` plus an independent typed
throttle observation; engine failure and field provenance remain separate facts.
Its private wire DTO imports schema-v1 values only for an identified device and
the historical field sentinel, and emits legacy keys only from currently
Available truth. Failed GPU reads therefore cannot serialize as idle, cold,
empty-memory or unthrottled success.

`ProcessResourceSnapshot` stores only private state, sources and canonical
`ProcessResourceObservations`. Its private wire DTO imports non-empty schema-v1
mirrors only when typed truth is `Unknown` and a success time exists; typed
conflicts win. Serialization projects legacy success fields only from typed
`Current`, while confirmed empty, partial, stale and unavailable remain
distinguishable in the typed observation.

`ProcessItem` owns private metadata and application-identity observations.
Its private wire DTO imports legacy owner/path only for a trustworthy nonzero
process identity when typed truth is `Unknown`; current accessors are the only
consumer surface, and serialization projects legacy owner/path from current
typed truth.

External saved-view input is read-only at the private ingress: recognized old
tokens are canonicalized into `ProcessViewPresetConfig`. The canonical write
DTO has no obsolete mode field, so saving a parsed configuration cannot
republish the old vocabulary.
