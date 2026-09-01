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
serialization, registration completeness and dependency direction. A typed
request declares whether runtime lifecycle belongs to the whole capability, a
stable opaque target scope, or an existing job's sideband control; the runtime
may compare a target scope but never reinterpret it as an OS locator. Target
scope construction is fallible and preserves the exact UTF-8 identity up to the
public 4 KiB transport bound; empty or oversized identities are typed errors,
never truncated or replaced with a lossy hash. Sideband admission is denied by
default and requires an explicit idempotent request-family policy.

`CapabilityScheduler` exposes only an implementation-neutral bounded snapshot:
counters, fixed eight-domain rollups, admission reasons, and at most 64 recent
stall transitions. It also reports configured resource ceilings, current
target/scope/delivery use, and control/observation/terminal queue high-water.
Delivery diagnostics retain per-class use and rejection counts plus the
configured control reserve, without exposing runtime queue implementations.
ECS types and an unbounded event log never cross this crate.

`window_capture.rs` owns the platform-neutral receipt, backend provenance and
bounded failure vocabulary for one accepted PNG. It contains no Wayland, D-Bus,
PipeWire or provider-specific object, leaving those details to the native adapter.
The current native implementation selects a Linux one-shot provider only;
Portal Screenshot and continuous ScreenCast/PipeWire remain explicit future
backend values and must not be inferred as supported from this contract alone.

Physical-device producers use the constrained `DeviceDiscovery` constructor so
IDs, discovery outcome, and item count are derived together; it is the only
public construction route. `RequiresEscalation` survives
provider-to-operation mapping as its own outcome rather than becoming a generic
permission denial.

## Module map

```text
src/capability.rs                 capability/request/outcome vocabulary
src/port.rs  scheduler.rs         port and scheduling vocabulary
src/envelope.rs                   typed event envelope (EventSequence)
src/failure.rs                    unified failure vocabulary
src/instance.rs  source.rs  tray.rs  window_capture.rs
                                   instance, source, tray and PNG-capture vocabulary
```

Consumed by application (event ingress) and implemented by platform adapters.
