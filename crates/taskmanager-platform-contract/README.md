# taskmanager-platform-contract

## Role

Platform-neutral capability, request, event, provider and port contracts.

## Boundary

The crate defines what an adapter may provide and how outcomes are correlated;
it does not read an OS, spawn workers, own a UI, or implement a vendor API.

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

Physical-device producers use the constrained `DeviceDiscovery` constructor so
IDs, discovery outcome, and item count are derived together. The raw constructor
is an explicitly deprecated compatibility seam. `RequiresEscalation` survives
provider-to-operation mapping as its own outcome rather than becoming a generic
permission denial.
