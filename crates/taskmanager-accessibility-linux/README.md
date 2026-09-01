# taskmanager-accessibility-linux

## Role

Linux AT-SPI bridge for the shared semantic UI contract. It adapts typed
semantic snapshots to AccessKit/Unix without owning page state or rendering.

## Boundary

The crate owns platform accessibility publication, node identity, roles,
names, states, focus and notifications. It does not read telemetry, choose
providers, or expose toolkit widgets to core/application.

## Contract and verification

Unavailable desktop accessibility support is typed and visible. Keep the
semantic tree aligned with the renderer projection; verify with the crate
tests and the real AT-SPI receipt described in `../../docs/screenshots/README.md`.

## Module map

```text
src/bridge.rs    AT-SPI (D-Bus) connection and lifecycle
src/mapping.rs   typed semantic snapshots → AccessKit/Unix roles, states, focus
```
