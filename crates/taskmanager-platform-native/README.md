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
shared contract types remain owned by `taskmanager-platform-contract` and are
not forwarded through this selector.
OS-specific process liveness and native path knowledge remain in the selected
adapter; the application host receives only safe probes and owned paths.
Linux currently provides validated local-time rules. macOS and Windows return
typed `Unsupported` until their native time-zone adapters exist; the selector
never substitutes UTC for an unavailable local zone.
Current-window PNG capture is selected here as well: Linux delegates to its
Wayland-capable adapter, while other targets return typed `Unsupported`.
Verify cfg edges, feature closure and reverse dependency firewalls whenever an
adapter or composition dependency changes.

## Module map

```text
src/instance.rs   compile-time selection of the one native OS adapter (ADR-009)
src/tray.rs       tray adapter selection
src/lib.rs        executable composition boundary and native capture selection
```
