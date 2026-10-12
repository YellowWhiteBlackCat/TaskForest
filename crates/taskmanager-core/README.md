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

`time::LocalTimeRules` validates bounded TZif into typed offset/DST transitions;
details in [`docs/MODULE_OWNERSHIP.md`](docs/MODULE_OWNERSHIP.md).

## Contract and verification

`ScalarObservation`, stable identity, generation, gap-aware history,
formatters, filters and control vocabulary are canonical here. Keep pure
functions deterministic and verify with core tests, wire invariants and the
platform boundary firewall. Clock tests use fixed supplied instants rather than
host-time plausibility.

Core evolution follows the repository hard-cutover rule
([AGENTS.md](../../AGENTS.md), Mission and architecture): a new typed contract
moves every caller, fixture and export in the same change, with no aliases,
wrappers or dual semantic paths left behind.

Observation value/state fields are private with named constructors; wire DTOs
are the only external-payload ingress and canonicalize immediately. Per-domain
rules live in [`docs/OBSERVATION_SEMANTICS.md`](docs/OBSERVATION_SEMANTICS.md)
and [`docs/WIRE_INGRESS.md`](docs/WIRE_INGRESS.md).

## Module map

```text
src/core.rs                     owner-module index for every fact below
├── metrics/                    domain measurements and availability joins
│   └── */wire.rs               private ingress DTOs (legacy payloads canonicalize here)
├── process/                    process facts: identity.rs, aggregate.rs, group_aggregate.rs
├── process_telemetry/          per-process GPU/network/resource telemetry facts
├── process_batch_history.rs    batched process history snapshots
├── services/  startup/         service and startup-item facts
├── alerts/                     alert rules, active-set transitions, bounded event export
├── export/                     typed export formatting (CSV and friends)
├── sensors/                    thermal and fan sensor facts
├── smart/                      SMART health, attribute and self-test capability facts
├── time/                       TZif parsing, local time rules, time-zone transitions
├── config/  config.rs          configuration facts and runtime config groups
├── diagnostics/                diagnostic snapshot and bundle facts
├── history.rs                  gap-aware history facts
├── storage.rs  storage_health.rs  directory_usage.rs   storage and health facts
├── power.rs  npu.rs            power/RAPL and NPU accelerator facts
├── cpu_codename.rs  cpu_features.rs                    CPU identity facts
├── appearance.rs  text.rs  units.rs  tray.rs           presentation helpers
├── device_state.rs  system_health.rs  kernel_log.rs    system-level facts
├── session.rs  setup.rs  source.rs  target.rs          session/setup/source identity
└── identity.rs  hardware.rs  failure.rs                core identity and failure types
```

Pure rules and typed facts only: no I/O, no clock reads, no toolkit types.

## Docs

- [`docs/OBSERVATION_SEMANTICS.md`](docs/OBSERVATION_SEMANTICS.md) — scalar/group observation
  privacy, named transitions, availability-preserving folds and fixture policy.
- [`docs/WIRE_INGRESS.md`](docs/WIRE_INGRESS.md) — per-domain wire DTO hydration and
  serialization rules (CPU, memory, disk, network, GPU, process, services, battery, saved views).
- [`docs/MODULE_OWNERSHIP.md`](docs/MODULE_OWNERSHIP.md) — module ownership detail and
  local-time rules.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) and
  [`SCALAR_AVAILABILITY`](../../docs/SCALAR_AVAILABILITY.md) — cross-crate ledgers;
  [ADR-015](../../adr/015-optional-observation-semantics.md),
  [ADR-016](../../adr/016-typed-observation-wire-invariants.md) — availability and wire invariants.
