# ADR-011: Independent system observation facets and compatibility projection

Status: accepted

## Context

The public `TelemetryObservationProvider` currently returns one `SystemSnapshot`.
Linux assembles that snapshot on one background thread by running CPU, memory,
storage, network, GPU, host-uptime, and process-count work in sequence. The internal
collector already has domain provider traits, but they share one schedule, one
backpressure boundary, and one completion time.

Those domains no longer have equivalent execution characteristics:

- CPU frequency, thermal, and energy sources have different availability from the
  base utilization source;
- memory composition and hardware metadata fail independently from current totals;
- storage discovery, diskstats, mounts, and throttled SMART work have different
  latency and authority;
- network discovery, counters, addresses, and wireless enrichment maintain their
  own rate baselines and hot-plug lifecycle;
- GPU discovery and runtime-selected DRM, Intel, AMD, NVIDIA, and future vendor
  enrichers may load optional libraries or block independently;
- uptime and process/thread counts are host-runtime facts, not CPU observations.

A slow SMART command or vendor GPU call can therefore delay unrelated CPU and memory
facts even though source status inside the eventual aggregate remains typed. Splitting
only the Linux helper traits, then calling the same full collector from several public
providers, would preserve that hidden coupling.

## Decision

System observation becomes six independently correlated application capabilities:

- host runtime facts;
- CPU telemetry;
- memory telemetry;
- storage telemetry;
- network telemetry;
- GPU telemetry.

Each capability has its own stable `CapabilityId`, application request/event payload,
provider SPI trait, injected runtime executor, bounded lane, deadline, health, and
recovery tests. Native providers execute only their domain I/O. A domain provider may
not call an aggregate collector and select one field from its result.

The domains remain modules inside the existing core, application, provider, runtime,
and native-adapter crates. They do not become one crate per metric family. A new crate
is justified only when it creates a real dependency firewall, native composition
edge, independently reusable state owner, or excludes a heavy outer dependency from
an inner build.

### Scheduling and control

Telemetry pause and interval remain one control capability because they mutate one
user policy. Applying the policy updates independently owned sampler schedules; it
does not make their next observations atomic. A control acknowledgement means that
the policy was accepted, not that all six domains completed another tick.

Every sampler owns its rate baseline, retry/backoff state, source freshness, and
device lifecycle where applicable. Sharing immutable clock/configuration inputs is
allowed. Sharing a worker, completion latch, mutable full-system collector, or
blocking refresh call is not.

Storage, network, and GPU keep separate discovery authorities and lifecycle
generations. CPU and memory cannot confirm device absence on their behalf. SMART and
vendor enrichers remain runtime-selected providers in the standard platform artifact;
they never create vendor-specific product binaries.

### Application projection

`taskmanager-application` owns a `SystemTelemetryProjection` that accepts correlated
domain events. It:

- tracks a monotonic request revision per refresh generation;
- rejects duplicate, older, or mismatched events;
- publishes a new partial projection after each accepted domain event instead of
  waiting for all domains;
- keeps Pending, current, partial, stale, and unavailable domain state explicit;
- preserves each domain's own observed time and source outcomes;
- prevents an older device generation from replacing a newer storage, network, or
  GPU projection.

`SystemSnapshot` remains a compatibility read model during migration. It is derived
from the application projection and is not the native provider contract. Its single
`timestamp_ms` cannot be interpreted as proof that every field was sampled
atomically. Legacy fields may be populated only from current typed observations;
stale or unavailable values cannot reappear as believable current zeroes.

GPUI and TUI consume the application projection or its per-domain events. They do not
own fan-out, completion barriers, retry policy, or cross-domain identity logic.

Telemetry histories are appended only after the corresponding domain event passes the
application correlation and generation checks. Native collectors do not directly
publish a full-system history tick whose validity depends on unrelated domains.

### Migration

The transition is performed in this order:

1. add domain snapshot types, capability IDs, ports, provider traits, executors,
   lanes, and fake-provider isolation tests;
2. add `SystemTelemetryProjection` and derive a complete `SystemSnapshot` read model;
3. split Linux state ownership and physical I/O so each provider performs only its
   domain work;
4. migrate GPUI/TUI and history ingestion to partial domain updates;
5. remove the aggregate provider/request/lane after no production consumer uses it.

This migration is complete: no aggregate request/event, raw batch queue, frontend
fallback, or aggregate lifecycle authority remains. A complete `SystemSnapshot` is
materialized only from the six-domain projection.

## Consequences

CPU and memory updates continue when SMART, wireless, eBPF, or a vendor GPU backend is
slow or unavailable. Capability health reflects the exact domain event that was
published. Cross-platform adapters can implement domains incrementally without
manufacturing a full Linux-shaped snapshot.

There are more ports and lanes, but not more release SKUs and not one crate per trait.
The application projection becomes the sole correlation and complete-read-model owner, so
frontends and native adapters do not duplicate concurrency policy.

This decision supersedes ADR-007's earlier provisional statement that a cohesive
`SystemSnapshot` cadence did not yet justify public CPU, memory, storage, and network
facets. The current sequential collector, throttled SMART work, device-specific
lifecycle, and runtime vendor providers are the concrete scheduling, availability,
and backpressure differences that now justify the split.
