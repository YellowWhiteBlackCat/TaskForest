# Capability registry and coverage

## Capability semantics

Component capability registration is exhaustive: every capability defines an
explicit, toolkit-neutral semantic specification (`CapabilitySemanticSpec`)
covering user-facing behavior, keyboard/pointer semantics, and invariant
expectations. `taskmanager-ui` is the reference shape (GPUI-05); every
deliberate frontend difference is a reasoned, gate-checked registry entry,
never a prose parity claim.

`ColumnDragResize` is a component-level contract whose GPUI reference is
`taskmanager-ui/src/data/table/resize.rs`; page identity and persistence stay
outside this crate.

## Feature coverage

`src/feature_coverage.rs` owns the FeatureId/FeatureArea coverage registry and
the feature_coverage_findings gate: one explicit declaration per frontend,
proving declaration discipline only, never behavioral equivalence.
`src/feature_coverage/` carries delivery semantic specs, per-feature platform
binding, and the read-only three-axis ledger fold. `src/functional.rs` is the
CORE-04 registry: product intent × frontend surface decision
(shared/local/accepted-difference/unsupported).

## Verification

Enum coverage, cross-frontend command parity and explicit functional surface
decisions are asserted by the crate tests:

```bash
cargo nextest run -p taskmanager-ui-contract -j 4
```
