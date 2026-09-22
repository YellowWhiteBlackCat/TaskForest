# ADR-053: Product-expected capability surface with typed absences

Status: accepted

## Context

The runtime capability catalog used to publish one descriptor per registered
route only. A product capability that no adapter on the current platform
registered simply had no entry: a consumer could see "no such capability" but
never a typed reason. That shape is indistinguishable from a bug and lets a
product promise disappear silently, and the contract explicitly tolerated it
for optional capabilities ("an absent provider simply leaves this capability
absent").

Two local practices already pointed the other way, but neither was a product
invariant:

- ADR-009 gives a fully unimplemented platform an absent-capability handle that
  answers typed `Unsupported` instead of pretending to work; ADR-043/044 treat
  Android/OpenHarmony providers as typed capability absence.
- macOS and Windows expressed individual gaps as registered `Pending*Provider`
  routes whose initial status is `Unsupported`/`TemporarilyUnavailable`.

Those conventions are per-capability and can be omitted by the next
contributor. Meanwhile the deep-surface families (`telemetry.pressure`,
`memory.vma-map`, `ipc.dbus`, `profiling.pmu`, …) make "no source on this
platform" the normal case rather than an exception. The product needs one
mechanical answer to "which capabilities must appear in a runtime catalog at
all" plus a single honest shape for "this platform has no qualified source".

## Decision

1. `CapabilityId::EXPECTED_SURFACE` (81 identities today) is the product
   authority for the capability identities the product promises to answer for
   on every platform, whether the answer is a real observation or a typed
   absence. It is a product fact, not a platform claim. The extensible
   `CapabilityId` stays open to vendor/diagnostic identities, but those are
   deliberately outside the promise.
2. The runtime catalog seeds one
   `CapabilityDescriptor::typed_absence` per expected identity before applying
   platform routes. A real route replaces only its own entry with provider
   attribution and its initial status; an expected identity is never omitted
   from a snapshot.
3. The absence shape has one authority,
   `CapabilityDescriptor::typed_absence`: `Unsupported`, empty provider
   attribution, no observation time, no last success. It is never a fabricated
   zero, never a claim about transient runtime conditions, and never a
   registered-pending lane - `is_typed_absence` distinguishes the two, because
   a registered lane always names its provider.
4. `CapabilitySnapshot::registered` and `typed_absences` are the two catalog
   projections. Membership in the expected surface is identity only:
   `CapabilityId::is_expected` never fabricates availability and never panics
   on unknown, malformed, or vendor identities.
5. The product surface census is closed and bidirectional: every product
   capability constant must be listed in `EXPECTED_SURFACE`, and the contract
   census plus the per-platform catalog contracts fail when a registered
   identity is not product-expected.
6. Scope: the three product platforms compose the seeded runtime. Adapters
   outside the product platform axis (the Android/OpenHarmony placeholder
   handles, ADR-043/044) may still compose the empty capability-absent handle;
   whether they must publish the expected surface is a separate owner decision
   and is not settled here.
7. No compatibility path: "capability exists but has no descriptor" is no
   longer a representable product state. Consumers answer from the descriptor
   status, never from entry presence.

## Consequences

- Silent absence is gone for the product surface. Every expected capability
  appears exactly once with either a real route/status or the typed absence; a
  missing descriptor for an expected identity is a defect, and descriptor
  presence is mechanically checkable.
- "Not implemented on this platform" and "registered but currently failing or
  absent" stay distinguishable: the former has no provider attribution, the
  latter keeps its provider identity and typed statuses such as
  `TemporarilyUnavailable`, `Degraded`, or a pending `Unsupported`.
- Adding a capability to the promised surface adds its typed-absence descriptor
  on every platform at once, so frontends render an honest state without
  platform conditionals and shared layers stay free of `cfg(target_os)`.
- The catalog is not a coverage claim: typed absences are expected on every
  platform and are not parity failures. Frontends must still render a typed
  result; letting the whole surface disappear silently remains a defect
  (`docs/ARCH.md` §8.4).
- The decision is irreversible in practice: descriptor presence is now part of
  the published capability contract that consumers, conformance tests, and
  gates rely on, and reverting would reintroduce a state where an expected
  promise has no typed answer, which the product contract forbids.

## Verification

- `crates/taskmanager-platform-contract/tests/headless/capability_surface.rs`
  pins the identity census in both directions, proves the typed-absence shape
  is honest and distinguishable from a registered-pending lane, and proves
  unknown/vendor identities stay outside the surface.
- `crates/taskmanager-platform-runtime/tests/headless/delivery/catalog.rs`
  proves an unregistered expected capability keeps its typed absence and that a
  platform registration replaces only its own entry.
- The per-platform catalog contracts
  (`crates/taskmanager-platform-linux/tests/headless/ports_contract/provider_registration.rs`,
  `crates/taskmanager-platform-windows/tests/headless/contract.rs`,
  `crates/taskmanager-platform-macos/tests/headless/contract.rs`) assert that
  every registered capability is product-expected, every typed absence is
  product-expected, and every expected identity stays addressable.
- `crates/taskmanager-platform-runtime/tests/headless/runtime_composition_tests.rs`
  and `runtime_channel_construction_tests.rs` prove an absent binding creates
  no port lane while keeping the typed absence.
- `crates/taskmanager-platform-runtime/src/absent.rs` documents the
  empty-catalog handle as exclusive to non-product placeholder adapters.

## References

- Current rules: `docs/CROSSPLATFORM_STRATEGY.md` (统一能力规则),
  `docs/TELEMETRY_MANIFEST.md` §3, `docs/ARCH.md` §8.4.
- Related decisions: ADR-007 (one facet per capability, never fabricate zeros),
  ADR-009 (native adapter selection and absent-capability handle), ADR-043 and
  ADR-044 (typed capability absence for placeholder platforms), ADR-047
  (owner-direct cross-crate imports).
