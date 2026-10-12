# Telemetry status

CPU and memory telemetry are explicitly deferred. The published `sysinfo`
support list does not include OpenHarmony, so a successful Rust compilation
must not be treated as evidence of valid runtime observations. The available
community OpenHarmony bindings are unofficial and still incomplete for the
HiDebug component. See [ADR-043](../../../adr/043-defer-openharmony-native-telemetry.md).

Until a maintained safe wrapper or a separately audited OHOS native boundary
exists, this crate keeps those capabilities absent and returns typed
unsupported outcomes.

## Admission rule

An OHOS provider may be added here only after its native source, permission
behavior, identity rules, and failure mapping are verified on the target
device.

## Verification

```bash
cargo nextest run --locked -p taskmanager-platform-ohos --all-targets -j 4
cargo check --locked --target aarch64-unknown-linux-ohos -p taskmanager-platform-ohos
```

The crate is not yet connected to `taskmanager-platform-native` or any
frontend. That connection belongs after the first verified OHOS provider and
the application-data path/context boundary exist.
