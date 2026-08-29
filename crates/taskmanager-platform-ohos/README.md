# taskmanager-platform-ohos

## Role

Standalone OpenHarmony platform-adapter seam for TaskForest. The crate is
deliberately separate from `taskmanager-platform-linux`: Rust's OpenHarmony
targets expose `target_os = "linux"` together with `target_env = "ohos"`, so
`target_os` alone is not a safe platform-selection rule.

## Boundary

This first milestone contains no provider, native ABI, command execution,
windowing backend, or UI dependency. `OhosPlatformRuntime::spawn()` returns the
shared capability-absent handle. Requests therefore produce the existing typed
unsupported outcome instead of claiming Linux data sources or fabricating
telemetry.

The shared `taskmanager-core`, application contracts, and runtime remain the
only reusable lower layers. An OHOS provider may be added here only after its
native source, permission behavior, identity rules, and failure mapping are
verified on the target device.

## Telemetry status

CPU and memory telemetry are explicitly deferred. The published `sysinfo`
support list does not include OpenHarmony, so a successful Rust compilation
must not be treated as evidence of valid runtime observations. The available
community OpenHarmony bindings are unofficial and still incomplete for the
HiDebug component. See [ADR-043](../../adr/043-defer-openharmony-native-telemetry.md).

Until a maintained safe wrapper or a separately audited OHOS native boundary
exists, this crate keeps those capabilities absent and returns typed
unsupported outcomes.

## Contract and verification

The crate must remain safe Rust and must not depend on GPUI, Iced, Ratatui,
Wayland, Linux procfs helpers, or Linux privilege helpers. The current checks
are:

```text
cargo nextest run --locked -p taskmanager-platform-ohos --all-targets -j 4
cargo check --locked --target aarch64-unknown-linux-ohos -p taskmanager-platform-ohos
```

The crate is not yet connected to `taskmanager-platform-native` or any
frontend. That connection belongs after the first verified OHOS provider and
the application-data path/context boundary exist.
