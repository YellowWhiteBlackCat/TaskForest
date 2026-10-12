# Android source contract

An Android provider may publish a capability only after its source,
permission, identity, lifecycle, timeout, and failure mapping are verified on
a target device. Candidate source families are:

| Capability | Android source direction | Initial contract |
|---|---|---|
| Host memory | `ActivityManager.MemoryInfo` | typed current/partial/unavailable |
| Battery | `BatteryManager` | separate Android provider; do not use `starship-battery` |
| Storage | `StorageStatsManager` | app-private data by default; usage access is explicit |
| Network | `NetworkStatsManager` and verified interface sources | bucket/device/app scope must remain explicit |
| Sensors | `SensorManager` | lifecycle-aware sampling and typed absence |
| CPU/processes | Android APIs or verified `/proc` access | no assumption of desktop-equivalent visibility |

Ordinary Android applications cannot promise unrestricted foreign-process
memory inspection or control. Such operations remain typed `Unsupported` or
permission-gated until an Android-specific authority is verified.

## Composition rules

The provider is not connected to `taskmanager-platform-native`, the desktop
app-host, or a frontend. An Android Activity/Service and APK packaging layer
must own Android lifecycle and context/path injection before product wiring.

```bash
cargo nextest run --locked -p taskmanager-platform-android --all-targets -j 4
cargo check --locked --target aarch64-linux-android \
  -p taskmanager-platform-android --features android-provider
```

Target compilation proves only the Rust target seam. It does not prove
Android permissions, runtime values, background execution, or APK behavior.
