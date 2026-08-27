# ADR-015: Optional observations keep presence and freshness orthogonal

Status: accepted

## Context

Portable telemetry contains facts that are not universal:

- an Ethernet adapter has no SSID or wireless signal;
- a Wi-Fi adapter may be present but unassociated;
- firmware may expose memory slots but not their configured speed;
- compressed memory, a compressed swap device, and a compressed swap cache are
  independent capabilities;
- a provider may have reported a value previously and then fail.

One `Option<T>` cannot distinguish these states. `None` has historically meant
all of “old payload”, “not applicable”, “confirmed absent”, “provider does not
support this”, and “the current refresh failed”. `ScalarObservation<Option<T>>`
does not repair the wire contract: an outer missing value and an observed inner
`None` both serialize as JSON `null`, so a round trip can erase the distinction.

Treating every missing optional fact as `Unsupported` is also incorrect. An
unassociated Wi-Fi interface is a successful observation, not a provider
failure; SSID on Ethernet is not applicable; a failed `iw` call is neither.

## Decision

The shared core owns one reusable optional-observation contract. It represents
two independent axes:

1. semantic state: `Unknown`, `Present(T)`, `Absent`, or `NotApplicable`;
2. freshness: `Unknown`, current, current-but-partial, stale, or unavailable,
   using the shared typed failure vocabulary.

Constructors establish valid combinations. Current accessors return a value
only for a current `Present(T)`. Separate state accessors allow a consumer to
distinguish a current `Absent` or `NotApplicable` observation without
fabricating a value. A failed refresh may retain the prior semantic state only
as stale and must preserve the last observation timestamp.

The following rules apply to every migration:

- `ScalarObservation<Option<T>>` is forbidden.
- A default observation is `Unknown`; default zero, `false`, an empty string,
  and an empty collection are not measurements.
- `Absent` means the authoritative provider successfully confirmed that the
  optional fact is not currently present.
- `NotApplicable` means the fact has no meaning for the identified entity.
- `Unavailable(failure)` means no trustworthy current semantic state exists.
- `Stale(failure)` may carry the last `Present`, `Absent`, or `NotApplicable`
  state, but current accessors hide it.
- `Partial(failure)` requires a current semantic state and records failure of a
  contributing source.
- legacy `Option<T>` fields remain serialization projections while schema-v1
  compatibility is required. They are consulted only while typed truth is
  `Unknown` and never override typed absent, not-applicable, stale, or failed
  states.

For lifecycle-bound data, native adapters key retained observations by stable
`DeviceId` and `DeviceGeneration`. A new generation cannot inherit a prior
optional state. Provider recovery within the same generation may replace stale
state with a current state. Renaming a native interface without changing its
stable identity does not create a new device.

Initial migrations are:

- Network totals, rates, link capacity, wireless signal, association and SSID;
- Memory composition, commitment, compression and physical-module enrichment.

The type remains provider- and OS-neutral. Linux maps sysfs, procfs, DMI,
wireless tools and other native evidence at its adapter boundary. Future
Windows and macOS providers map their own evidence into the same semantic
states rather than importing Linux terminology.

## Consequences

Frontends can render “not applicable”, “not connected”, “unsupported” and
“temporarily unavailable” honestly. History and export code can distinguish a
measured empty state from a gap. Retained values no longer look current after a
provider failure.

The contract adds explicit state handling to provider assemblers and consumers.
That cost is intentional: collapsing the states would recreate ambiguity at
every UI and export boundary. Existing schema-v1 fields remain until their
compatibility window closes, so migrations must keep typed truth as the sole
new authority and test the legacy projection separately.
