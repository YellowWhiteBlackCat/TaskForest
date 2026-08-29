# ADR-044: Feature-gated Android provider seam

Status: accepted

## Context

The Android Rust target is available as `aarch64-linux-android`, and the
platform-neutral `taskmanager-core`, application, shell, contract, runtime,
telemetry-store, and history-store layers cross-compile without Android-only
source changes. `taskmanager-core` has no OS or UI dependency and is therefore
the strongest reuse boundary.

The current desktop composition is not an Android build: the native selector
has no Android branch, the tray and single-instance seams have no Android
fallback, and the pinned GPUI 0.2.2 source contains only Linux, macOS, and
Windows platform implementations. The portable battery provider depends on a
crate without an Android platform branch. Android also does not provide
desktop-equivalent foreign-process visibility and control to ordinary apps.

## Decision

1. Add `taskmanager-platform-android` as a standalone provider seam, following
   the independent-composition pattern used by
   `taskmanager-platform-ohos`. It must not select or depend on the Linux
   adapter.
2. Gate the seam with the opt-in Cargo feature `android-provider`. The feature
   is disabled by default, is not part of `hardware-all`, and is not a product
   or hardware SKU. Enabling it only opts into the Android provider boundary;
   it does not make a capability `Available`.
3. Keep the initial Android runtime capability-absent. Until a target-device
   source has a verified permission, identity/generation, lifecycle, timeout,
   and failure mapping, requests return typed `Unsupported` outcomes.
4. Reuse `taskmanager-core`, application contracts, shell projections, and the
   bounded runtime where their Android lifecycle and path/context contracts
   remain valid. Android app-private paths must be injected by the Android
   host; desktop home-directory discovery is not reused.
5. Android system APIs are separate provider authorities: memory through
   `ActivityManager.MemoryInfo`, battery through `BatteryManager`, storage and
   usage through the corresponding Android managers, and sensors through
   `SensorManager`. CPU, process, GPU, service, tray, and desktop-control
   capabilities remain independently classified and may stay unsupported.
6. Do not connect this seam to `taskmanager-platform-native`, the desktop
   app-host, or a frontend until an Android Activity/Service lifecycle and APK
   packaging boundary exists.

## Consequences

The shared domain and application model have a supported compile-time reuse
path without weakening the current three-platform release contract. Android
can grow capability by capability, while unavailable data remains visible as
typed absence instead of Linux-shaped success.

The first Android product cannot promise desktop process-management parity.
Its provider will need a separate Android permission model and target-device
receipts. A future JNI or other native bridge must be isolated behind a typed,
owned boundary and reviewed under the repository's safe-Rust policy.

## Verification

```text
cargo nextest run --locked -p taskmanager-platform-android --all-targets -j 4
cargo check --locked --target aarch64-linux-android \
  -p taskmanager-platform-android --features android-provider
```

The target check proves compilation only; it does not prove Android API
availability, permission grants, background execution, or APK behavior.

## References

- [Rust Android target support](https://doc.rust-lang.org/rustc/platform-support.html)
- [Android NDK toolchains](https://developer.android.com/ndk/guides/other_build_systems)
- [Android ActivityManager](https://developer.android.com/reference/android/app/ActivityManager)
- [Android BatteryManager](https://developer.android.com/reference/android/os/BatteryManager)
- [Android StorageStatsManager](https://developer.android.com/reference/android/app/usage/StorageStatsManager)
- [Android services and background work](https://developer.android.com/develop/background-work/services)
