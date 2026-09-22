# taskmanager-platform-conformance

## Role

Host-neutral conformance scenarios shared by Linux, macOS and Windows adapter
tests and the live smoke gate.

## Boundary

Scenarios assert capabilities, process-row invariants, live drain ownership,
typed failure semantics, wrong-identity zero-side-effect rejection and device
discovery coherence. They contain no OS I/O, target-specific paths or
`cfg(target_os)` provider logic.

## Contract and verification

Keep assertions portable across minimal runners. Each adapter runs the same
contract on its native host; the suite proves shared semantics, not hardware
coverage or pixel acceptance.

## Capability-surface same-source scenario

`assert_fresh_surface_descriptors` proves the M3.2 fresh-runtime face: the
declared lane set, the typed initial status, the provider attribution, and the
typed absence for every unregistered product-expected identity. The stricter
live-catalog companion is `assert_capability_surface_matches_catalog`: it
cross-checks one platform's layer-B declaration
(`PlatformCapabilitySurface`) against the running catalog in both directions —
every declared `Present` lane is really registered under the adapter's
provider prefix, every registered lane is declared, and no product-expected
identity answers `Undeclared`. Whether a registered lane is a real source or a
registered-pending one is the adapter declaration's own split, because the
catalog publishes both as a registered descriptor; the adapter's typed-outcome
contract test proves the pending lanes answer `Unsupported`.

## Escalation/permission capability invariant

An escalatable denial and a hard permission denial are distinct capability
states, and an adapter must never fold one onto the other. The contract
projection that every provider must satisfy:

| Provider failure | Capability status |
| --- | --- |
| `RequiresEscalation` (the per-feature seam can still reach the data) | `RequiresEscalation` |
| `PermissionDenied` (hard denial, no offer) | `PermissionRequired` |
| `TimedOut` / `TemporarilyUnavailable` / `Rejected` (transient, no offer) | `TemporarilyUnavailable` |

`assert_capability_failure_status` checks this projection over synthetic input,
`admits_escalation` names the classification (only `RequiresEscalation` proves an
offer exists), and `projected_capability_status` re-enters the single authority
`ProviderFailure::capability_status` owned by `platform-contract` — the same
function the runtime catalog delegates to, so there is no second projection to
keep in sync. A future Windows or macOS provider that denies a capability must
report the escalation-aware variant whenever its per-feature seam can still
prompt, and the plain gate only for a hard denial. Adapters that project directly
from an escalation probe must publish the same states.

## Module map

```text
src/capability.rs  escalation.rs  identity.rs   host-agnostic assertions
src/process.rs     source.rs      smoke.rs      row/live-drain scenarios
```

Run against real hosts by each adapter's tests/conformance.rs and the root live smoke.
