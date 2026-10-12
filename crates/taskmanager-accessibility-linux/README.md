# taskmanager-accessibility-linux

## Role

Linux AT-SPI bridge for the shared semantic UI contract. It adapts typed
semantic snapshots to AccessKit/Unix without owning page state or rendering.

## Boundary

The crate owns platform accessibility publication, node identity, roles,
names, states, focus and notifications. It does not read telemetry, choose
providers, or expose toolkit widgets to core/application.

## Contract and verification

Unavailable desktop accessibility support is typed and visible. Bridge
ownership, the typed-unavailability rule and verification live in
[`docs/AT_SPI_BRIDGE.md`](docs/AT_SPI_BRIDGE.md).

## Module map

```text
src/bridge.rs    AT-SPI (D-Bus) connection and lifecycle
src/mapping.rs   typed semantic snapshots → AccessKit/Unix roles, states, focus
```

## Docs

- [`docs/AT_SPI_BRIDGE.md`](docs/AT_SPI_BRIDGE.md) — bridge/mapping ownership, typed
  unavailability and verification.
