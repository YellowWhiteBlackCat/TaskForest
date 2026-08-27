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
- `src/core.rs` and `src/lib.rs` expose the neutral facade; pure behavior belongs in nearby tests.

## Contract and verification

`ScalarObservation`, stable identity, generation, gap-aware history,
formatters, filters and control vocabulary are canonical here. Keep pure
functions deterministic and verify with core tests, wire invariants and the
platform boundary firewall. Clock tests use fixed supplied instants rather than
host-time plausibility. Notification cooldown identity is retained only
for the interval in which it can still affect a verdict. Providers may report
their full CPU fact vector, while every derived per-core history fan-out shares
the canonical `MAX_TRACKED_LOGICAL_CPUS` cardinality ceiling.

Generic scalar, optional, grouped and process-metadata observations keep their
value/state, availability and success-time fields private. Production writers
use named constructors and transitions; consumers choose explicit current or
last-known accessors. Partial scalar groups accept only typed current, partial
or unavailable slots and stamp all current slots atomically, so callers cannot
assemble Unknown/Stale children or mismatched refresh times. Lenient Unknown
hydration exists only behind private serde migration helpers.

`ServiceDeps` and `ServiceItem` each own one private typed
`ServiceRelationGraph`; consumers receive only read-only relation accessors.
Their private serde DTOs share one compatibility helper: four historical
dependency strings are derived from the graph on write and hydrate only
relation kinds absent from a typed payload on read. Unknown typed relation
kinds round-trip unchanged.

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

Process-view preset compatibility is read-only: a private read DTO accepts the
retired mode tokens and maps recognized rows into `ProcessViewPresetConfig`.
The separate canonical write DTO has no mode field, so saving a migrated
configuration cannot republish obsolete view state.
