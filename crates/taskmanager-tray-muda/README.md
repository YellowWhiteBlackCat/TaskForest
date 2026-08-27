# taskmanager-tray-muda

## Role

Shared tray/menu bridge for Windows and macOS adapters using `muda` and
`tray-icon`.

## Boundary

It maps toolkit-neutral tray intent to native menu events. It does not own
telemetry, application reducers, authorization or renderer window state.

## Contract and verification

Menu identity, show/hide/quit actions and unavailable native tray behavior are
typed. Verify target cfg compilation and keep platform-specific event loops at
the composition edge.
