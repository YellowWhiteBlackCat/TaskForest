# taskmanager-platform-native

## Role

Compile-time selection of the native OS adapter consumed by the application
host and frontend entrypoints.

## Boundary

This is the only supported composition route from a product binary to an OS
provider. It contains no renderer policy, business rule, direct UI state or
parallel provider registry.

## Contract and verification

Every target must expose the same typed platform surface with honest fallbacks;
shared contract types remain owned by `taskforest-contract` and are
not forwarded through this selector.
OS-specific process liveness and native path knowledge remain in the selected
adapter; the application host receives only safe probes and owned paths. The
per-target adapter, local-time and capture roster is the selection matrix in
[`docs/SELECTION_MATRIX.md`](docs/SELECTION_MATRIX.md); the selector never
substitutes UTC for an unavailable local zone, and capture never falls back to
a fabricated image. Runtime invocation order (renderer hook first) is owned by
the app-host worker. Verify cfg edges, feature closure and reverse dependency
firewalls whenever an adapter or composition dependency changes.

## Module map

```text
src/instance.rs   compile-time selection of the one native OS adapter (ADR-009)
src/tray.rs       tray adapter selection
src/lib.rs        executable composition boundary and native capture selection
```

## Docs

- [`docs/SELECTION_MATRIX.md`](docs/SELECTION_MATRIX.md) — the per-target adapter, local-time
  and capture selection matrix with fallback rules.
- [ADR-009](../../adr/009-native-os-adapter-selection.md) — native OS adapter selection.
