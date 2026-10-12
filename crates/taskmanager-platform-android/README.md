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

An Android provider may publish a capability only after its source,
permission, identity, lifecycle, timeout, and failure mapping are verified on
a target device. The candidate source-family table and composition rules
live in [`docs/ANDROID_SOURCE_CONTRACT.md`](docs/ANDROID_SOURCE_CONTRACT.md).

## Contract and verification

The provider is not connected to `taskmanager-platform-native`, the desktop
app-host, or a frontend. An Android Activity/Service and APK packaging layer
must own Android lifecycle and context/path injection before product wiring.
Verification commands are in the doc above.

## Module map

```text
src/lib.rs                     android-provider feature-gated seam (ADR-044);
                               currently capability absence (typed Unsupported)
```

## Docs

- [`docs/ANDROID_SOURCE_CONTRACT.md`](docs/ANDROID_SOURCE_CONTRACT.md) — candidate source
  families, composition rules and verification commands.
- [ADR-044](../../adr/044-feature-gated-android-provider-seam.md) — feature-gated Android
  provider seam.
