# taskmanager-platform-ohos

## Role

Standalone OpenHarmony platform-adapter seam for TaskForest. The crate is
deliberately separate from `taskmanager-platform-linux`: Rust's OpenHarmony
targets expose `target_os = "linux"` together with `target_env = "ohos"`, so
`target_os` alone is not a safe platform-selection rule.

## Boundary

This crate contains no provider, native ABI, command execution,
windowing backend, or UI dependency. `OhosPlatformRuntime::spawn()` returns the
shared capability-absent handle. Requests therefore produce the existing typed
unsupported outcome instead of claiming Linux data sources or fabricating
telemetry. CPU and memory telemetry are explicitly deferred per ADR-043
(details in [`docs/TELEMETRY_STATUS.md`](docs/TELEMETRY_STATUS.md)).

The shared `taskmanager-core`, application contracts, and runtime remain the
only reusable lower layers. An OHOS provider may be added here only after its
native source, permission behavior, identity rules, and failure mapping are
verified on the target device.

## Contract and verification

The crate must remain safe Rust and must not depend on GPUI, Iced, Ratatui,
Wayland, Linux procfs helpers, or Linux privilege helpers.

## Module map

```text
src/lib.rs                     OpenHarmony placeholder; CPU/memory telemetry deferred as
                               capability absence per ADR-043 (typed Unsupported)
```

## Docs

- [`docs/TELEMETRY_STATUS.md`](docs/TELEMETRY_STATUS.md) — deferral reasons, provider
  admission rule and verification commands.
- [ADR-043](../../adr/043-defer-openharmony-native-telemetry.md) — deferred OpenHarmony
  native telemetry.
