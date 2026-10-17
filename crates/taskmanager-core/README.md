# taskmanager-core

Platform-neutral domain models, policies, and algorithms for TaskForest and system monitoring tools.

[![Crates.io](https://img.shields.io/crates/v/taskmanager-core.svg)](https://crates.io/crates/taskmanager-core)
[![Documentation](https://docs.rs/taskmanager-core/badge.svg)](https://docs.rs/taskmanager-core)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

## Highlights

- **100% Safe Rust**: Strictly `#![forbid(unsafe_code)]`.
- **Zero System I/O**: Performs no filesystem access, network operations, or process spawning.
- **Clock-Independent & Deterministic**: Never reads the system wall clock or monotonic clock directly. All temporal functions accept explicit timestamps.
- **Rich Domain Models**: Process lineage and anomaly detection, SMART health scoring, alert rule engine, sensor topology, and canonical byte/bit unit formatting.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
taskmanager-core = "0.2"
```

### Example: Canonical Unit Formatting

```rust
use taskmanager_core::units::{format_byte_rate, format_memory, UnitPreferences};

let prefs = UnitPreferences::default();

// Memory: Base-2 bytes ladder
assert_eq!(format_memory(1_073_741_824, &prefs), "1.0 GiB");

// Network: Base-10 bits rate ladder
assert_eq!(format_byte_rate(125_000_000, &prefs), "1.0 Gb/s");
```

### Example: Alert Rule Engine

```rust
use std::time::Duration;
use taskmanager_core::alerts::{AlertEngine, AlertMetric, AlertRule, AlertSeverity};

let rule = AlertRule::new(
    "high-memory-pressure",
    AlertMetric::MemoryUsagePercent,
    AlertSeverity::Warning,
    90.0,
    Duration::from_secs(3),
    2.0, // 2% Hysteresis
);
let engine = AlertEngine::new([rule]);
assert_eq!(engine.rules().len(), 1);
```

## Role

Platform-neutral domain facts, availability semantics, identity/lifecycle,
history, configuration and pure shared rules.

## Boundary

Core owns no filesystem, process, command, window, widget, OS path, or native
handle, and it never reads the wall or monotonic clock. Time-dependent rules
receive an explicit timestamp; `time::unix_millis`/`unix_micros` only convert
an injected `SystemTime`. Business code remains `#![forbid(unsafe_code)]`; platform extensions
enter through typed capability models and `Unsupported`/permission outcomes.

`time::LocalTimeRules` validates bounded TZif into typed offset/DST transitions;
details in [`docs/MODULE_OWNERSHIP.md`](https://github.com/YellowWhiteBlackCat/TaskForest/tree/main/crates/taskmanager-core/docs/MODULE_OWNERSHIP.md).

## Contract and verification

`ScalarObservation`, stable identity, generation, gap-aware history,
formatters, filters, and control vocabulary are canonical here. Pure
functions are deterministic and verified with core tests, wire invariants, and the
platform boundary firewall. Clock tests use fixed supplied instants rather than
host-time plausibility.

Observation value/state fields are private with named constructors; wire DTOs
are the only external-payload ingress and canonicalize immediately. Per-domain
rules live in [`docs/OBSERVATION_SEMANTICS.md`](https://github.com/YellowWhiteBlackCat/TaskForest/tree/main/crates/taskmanager-core/docs/OBSERVATION_SEMANTICS.md)
and [`docs/WIRE_INGRESS.md`](https://github.com/YellowWhiteBlackCat/TaskForest/tree/main/crates/taskmanager-core/docs/WIRE_INGRESS.md).

## Module Map

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
