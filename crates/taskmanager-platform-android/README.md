# taskmanager-platform-android

## Role

Feature-gated standalone Android platform-provider seam for TaskForest. The
crate keeps Android composition separate from the Linux adapter while allowing
the platform-neutral core, application contracts, and runtime to be reused.

## Boundary

The `android-provider` feature is opt-in and disabled by default. It is a
composition marker, not a hardware SKU and not a claim that Android telemetry
is available. The current runtime returns the shared capability-absent handle;
requests therefore produce typed unsupported outcomes rather than Linux-shaped
facts or fabricated values.

This crate has no Android SDK/JNI dependency, native ABI, windowing backend,
tray implementation, or UI dependency. It must not depend on the Linux,
OpenHarmony, GPUI, Iced, Ratatui, Wayland, or desktop privilege adapters.

## Android source contract

An Android provider may publish a capability only after its source, permission,
identity, lifecycle, timeout, and failure mapping are verified on a target
device. Candidate source families are:

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

## Contract and verification

The provider is not connected to `taskmanager-platform-native`, the desktop
app-host, or a frontend. An Android Activity/Service and APK packaging layer
must own Android lifecycle and context/path injection before product wiring.

```text
cargo nextest run --locked -p taskmanager-platform-android --all-targets -j 4
cargo check --locked --target aarch64-linux-android \
  -p taskmanager-platform-android --features android-provider
```

Target compilation proves only the Rust target seam. It does not prove Android
permissions, runtime values, background execution, or APK behavior.

## Module map

```text
src/lib.rs                     android-provider feature-gated seam (ADR-044);
                               currently capability absence (typed Unsupported)
```
