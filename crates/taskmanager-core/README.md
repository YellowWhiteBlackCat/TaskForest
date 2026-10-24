# taskmanager-core

## Role

Platform-neutral domain facts, availability semantics, identity/lifecycle,
history, configuration and pure shared rules for TaskForest.

## Boundary

Core owns no filesystem, process, command, window, widget, OS path, or native
handle, and it never reads the wall or monotonic clock. Time-dependent rules
receive an explicit timestamp; `time::unix_millis`/`unix_micros` only convert
an injected `SystemTime`. Business code remains `#![forbid(unsafe_code)]`; platform extensions
enter through typed capability models and `Unsupported`/permission outcomes.

`time::LocalTimeRules` validates bounded TZif into typed offset/DST transitions;
details in [`docs/MODULE_OWNERSHIP.md`](docs/MODULE_OWNERSHIP.md).

## Contract and verification

`ScalarObservation`, stable identity, generation, gap-aware history,
formatters, filters, and control vocabulary are canonical here. Pure
functions are deterministic and verified with core tests, wire invariants, and the
platform boundary firewall. Clock tests use fixed supplied instants rather than
host-time plausibility.

Observation value/state fields are private with named constructors; wire DTOs
are the only external-payload ingress and canonicalize immediately. Per-domain
rules live in [`docs/OBSERVATION_SEMANTICS.md`](docs/OBSERVATION_SEMANTICS.md)
and [`docs/WIRE_INGRESS.md`](docs/WIRE_INGRESS.md).

## Module map

```text
src/core.rs                     owner-module index for every fact below
├── metrics/                    domain measurements and availability joins
│   ├── sample.rs               the measured sample: value, measurement instant, gap
│   └── */wire.rs               private ingress DTOs (legacy payloads canonicalize here)
├── process/                    process facts: identity.rs, aggregate.rs, group_aggregate.rs
│   └── collateral.rs           the frozen tree closure: root, members, depth
├── process_telemetry/          per-process GPU/network/resource telemetry facts
│   └── quota.rs                the container's declared cgroup ceilings (absent = undeclared)
├── process_batch_history.rs    batched process history snapshots
├── services/  startup/         service and startup-item facts
│   └── startup/timeline.rs     the per-phase boot split (each phase optional)
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

## Docs

- [`docs/MODULE_OWNERSHIP.md`](docs/MODULE_OWNERSHIP.md) — domain module ownership and pure function rules.
- [`docs/OBSERVATION_SEMANTICS.md`](docs/OBSERVATION_SEMANTICS.md) — observation value and state semantics.
- [`docs/WIRE_INGRESS.md`](docs/WIRE_INGRESS.md) — wire DTO ingress and immediate canonicalization.
