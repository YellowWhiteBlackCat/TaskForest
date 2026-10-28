# ADR-059: Blueprint disposition ledger for the full 225-slot census

Status: accepted

## Context

The product roadmap is the private 225-item four-frontend blueprint (15 areas x
15 items). The public `FeatureId` registry is the typed delivery registry: a
feature enters it only with a semantic specification, a platform binding, and
four explicit frontend declarations. It carries 107 features: 106 that
each cite one blueprint slot (including the ten Wave 3/4 deliveries, the four
promoted features: LTTB downsampling, saturation grade, D-state ratio, and
native alert integration, plus nine delivered native/shared slots: efficiency mode,
compound search, socket state tracking, filesystem mount audit, no-new-privs,
and four service diagnostics facets, eight hardware/telemetry slots, and eight further
delivered slots: freeze/thaw control, ZSWAP/compression, firmware/secure boot, GPU
throttle reasons, GPU driver/firmware API, process user credentials, service state machine,
and service cgroup limits), one deliberate extra (`memory.process-swap-charge`),
and one declared alias slot (`pressure.memory-thrashing-health` covering slot 179).

Two problems followed from treating `FeatureId::ALL` as the whole blueprint:

- The remaining slots had no public census. A previous audit arithmetic
  subtracted the ten Wave items twice and reported 159 unregistered slots; the
  real number is 150 (five adopted slots per area, ten undelivered). A count
  that lives only in prose can drift.
- Adding the 150 undelivered items as typed `FeatureId` variants would add 150
  enum variants and 600 frontend declarations for surfaces nobody has committed
  to deliver. That makes the delivery registry unmaintainable without making a
  single product decision, and it would tempt a blanket "Unsupported" arm that
  hides future omissions.

The blueprint itself is private planning material and must stay in `.private/`.
The public repository may state that a slot is dispositioned, but it must not
publish the private item titles or mechanisms.

ADR-053 established the analogous answer for capabilities: the product declares
the expected identity surface, and the runtime seeds a typed absence for every
unregistered expectation, so "no entry" is never a product answer.

## Decision

1. The roadmap has two layers. `FeatureId` remains the typed delivery registry.
   A new `BlueprintDispositionTable` is the complete 225-slot product census.
   Neither layer is a second delivery claim: the typed layer carries semantics
   and per-frontend decisions; the census carries adoption status.
2. Every slot `1..=225` carries exactly one disposition:
   `Adopted(FeatureId)` when the slot is committed through a typed feature, or
   `Unsupported` with a public reason when the slot is not delivered in the
   current product contract. An undispositioned slot is a defect.
3. `FeatureId::blueprint_slot()` is the authored typed-to-slot mapping. The
   census gate checks it in both directions: every adopted slot resolves to a
   feature in the same area, and every typed feature's slot is adopted by that
   feature. `MemoryProcessSwapCharge` is the sole typed feature outside the
   census and must be named explicitly.
4. The committed ledger is `scripts/parity/blueprint_dispositions.tsv`. It is
   public data with five columns: slot, area, disposition, feature id, public
   reason. It never contains the private blueprint title, mechanism, or plan
   prose.
5. `Unsupported` in this ledger means "not delivered in the current product
   contract". It is not a permanent product rejection and it never becomes a
   `Ready` claim. Promoting a slot is one change: add its `FeatureId` (semantic
   spec, platform binding, four frontend declarations) and flip the slot to
   `Adopted` in the same change. Removing a typed feature flips the slot back.
6. Two fail-closed gates protect the census. The Rust parser in
   `feature_coverage/blueprint_dispositions` resolves feature ids and enforces
   the bidirectional slot mapping; the quick-gate guard
   `scripts/quality/blueprint_scope_guard.py` enforces the public structure
   (225 unique slots, canonical area order, 107 adopted, 118 unsupported,
   reason/dash rules) without compiling the workspace.
7. The census counts are current-state data, not a ceiling. Adopting a slot
   raises the adopted count and lowers the unsupported count; the contract and
   quick-gate counts move in the same change. `BLUEPRINT_ITEM_COUNT` stays 225.

## Consequences

- The roadmap gap is closed as a product ledger: 225 dispositions, 107 adopted,
  118 explicitly unsupported, zero undispositioned. The earlier 159 figure is
  retired; the ten Wave items are among the 225 slots, not extra.
- No fake delivery is introduced. The 118 unsupported slots are visible gaps
  with public reasons; none carries a `Ready` cell, evidence anchor, or
  fabricated platform source.
- The typed registry stays maintainable. Only adopted features carry semantic
  specifications, platform bindings and four frontend decisions; the census
  carries the rest as data.
- The private blueprint remains private. The public ledger proves completeness
  by slot identity, not by publishing plan content.
- Adoption is still a product decision. The ledger makes the remaining work
  countable and forces every promotion or demotion to update both layers in the
  same change.
