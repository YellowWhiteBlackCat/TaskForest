# Observation semantics

Notification cooldown identity is retained only for the interval in which it
can still affect a verdict. Providers may report their full CPU fact vector,
while every derived per-core history fan-out shares the canonical
`MAX_TRACKED_LOGICAL_CPUS` cardinality ceiling.

## Private state, named transitions

Generic scalar, optional, grouped and process-metadata observations keep their
value/state, availability and success-time fields private. Production writers
use named constructors and transitions; consumers choose explicit current or
last-known accessors. Partial scalar groups accept only typed current, partial
or unavailable slots and stamp all current slots atomically, so callers cannot
assemble Unknown/Stale children or mismatched refresh times. External
old-format hydration exists only at a private serde ingress and is immediately
converted to the current typed observations.

Cross-crate fixtures live in the dev-only `taskmanager-test-support` crate and
assemble canonical observations through named builders. Core exports no
fixture DSL or test mutation surface to product dependencies.

## Availability-preserving folds

`src/core/process/aggregate.rs` owns availability-preserving scalar folds;
`src/core/process/group_aggregate.rs` owns the typed application, user, and
process-class group aggregates whose totals never collapse unavailable members
into a fabricated value. `src/core/process/identity.rs` owns the validated
`ProcessLiveKey` used to distinguish a provider-issued process incarnation
from a reused PID.

## Retained display values

A provider may sample an expensive source at a bounded cadence. Between samples
the observation is `Stale` and still carries the last real measurement. An
accessor named for the value a row displays (`memory_uss_bytes`,
`memory_anon_huge_pages_bytes`, and the shared/proportional/ratio/breakdown
figures derived from them) reads that retained value, so a decimated column and
everything computed from it stay put instead of blanking on deferred ticks.
Accessors prefixed `current_` read only a fresh measurement. `None` from either
kind means no measurement exists — never a fabricated zero — and freshness
remains readable from the observation's own availability.
