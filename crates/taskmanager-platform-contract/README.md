# taskmanager-platform-contract

## Role

Platform-neutral capability, request, event, provider and port contracts.

## Boundary

The crate defines what an adapter may provide and how outcomes are correlated;
it does not read an OS, spawn workers, own a UI, or implement a vendor API.
Core identity, failure and source-status facts remain owned by
`taskmanager-core`; this crate does not forward them through a compatibility
facade.

## Contract and verification

Capability identity, typed availability, sequence, generation and failure
metadata are part of the wire contract. Keep the API exhaustive and verify
serialization, registration completeness and dependency direction.

## Module map

```text
src/capability.rs                 capability/request/outcome vocabulary
src/platform.rs                   platform axis identity (linux/windows/macos)
src/platform_surface.rs           static per-platform registration surface
src/port.rs  scheduler.rs         port and scheduling vocabulary
src/envelope.rs                   typed event envelope (EventSequence)
src/failure.rs                    unified failure vocabulary
src/instance.rs  source.rs  tray.rs  window_capture.rs
                                   instance, source, tray and PNG-capture vocabulary
```

Consumed by application (event ingress) and implemented by platform adapters.

## Docs

- [`docs/REQUEST_SCOPES.md`](docs/REQUEST_SCOPES.md) — typed capability/target/sideband
  scopes and the bounded `CapabilityScheduler` snapshot.
- [`docs/SURFACE_AND_DISCOVERY.md`](docs/SURFACE_AND_DISCOVERY.md) — capture receipt,
  `DeviceDiscovery`, failure→capability-status projection and the static platform surface.
- [ADR-016](../../adr/016-typed-observation-wire-invariants.md) — typed wire invariants.
