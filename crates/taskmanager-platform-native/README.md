# taskmanager-platform-native

## Role

Compile-time selection and re-export of the native OS adapter consumed by the
application host and frontend entrypoints.

## Boundary

This is the only supported composition route from a product binary to an OS
provider. It contains no renderer policy, business rule, direct UI state or
parallel provider registry.

## Contract and verification

Every target must expose the same typed platform facade with honest fallbacks.
OS-specific process liveness and native path knowledge remain in the selected
adapter; the application host receives only safe probes and owned paths.
Linux currently provides validated local-time rules. macOS and Windows return
typed `Unsupported` until their native time-zone adapters exist; the facade
never substitutes UTC for an unavailable local zone.
Verify cfg edges, feature closure and reverse dependency firewalls whenever an
adapter or composition dependency changes.
