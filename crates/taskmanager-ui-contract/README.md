# taskmanager-ui-contract

## Role

Toolkit-neutral UI contracts: pages, descriptors, commands, focus targets,
semantic snapshots, icon identities, user-facing message keys, the
cross-frontend keybinding coverage matrix, the CORE-04 product-intent matrix,
and the component/surface capability parity registry (CORE-08:
`taskmanager-ui` is the reference shape - GPUI-05; every deliberate frontend
difference is a reasoned, gate-checked registry entry, never prose parity
claims), and the `presentation` layer (ADR-064): the one layout vocabulary
and budget fold, the per-shape threshold registry, the control-role
measurement matrix with a 44px hit-target floor, the motion-role/curve
contract (including the chart-slide ramp and the stacked statistics band),
and the cross-shape probe grid that computes parity instead of
asserting it. Defines explicit, toolkit-neutral semantic specifications and
the first-class divergence quality baselines under "求同存异" standards governance.

## Boundary

The crate has no renderer, OS API, provider, window object or business I/O. It
expresses intent and semantics that GPUI, Iced, TUI, and Bevy adapt independently.
Process semantic rows retain core-owned `ScalarObservation` and
`AggregateMetric` values so availability, failure reasons, retained values,
and aggregate coverage survive the shared projection.

## Contract and verification

`navigation::NavOrientation` is the single horizontal/vertical vocabulary. A native
control changes presentation while application page identity, selection and telemetry
remain under their existing authorities; each renderer budgets its own bounded rail.

`focus::InputModality` is the single input-origin vocabulary. No toolkit attaches an
input origin to focus state, so every graphical frontend tracks the origin itself and
asks this contract whether the focus ring should be painted: only keyboard modality
paints one. The frontends own only their capture wiring, never a second rule.

Keep command IDs, page order, focus reachability, semantic roles, localized
message keys and product intents exhaustive and stable. Verify enum coverage,
cross-frontend command parity, and explicit functional surface decisions.
Component capability registration is likewise exhaustive: every capability
defines an explicit, toolkit-neutral semantic specification (`CapabilitySemanticSpec`)
covering user-facing behavior, keyboard/pointer semantics, and invariant expectations;
`ColumnDragResize` is a component-level contract whose GPUI reference is
`taskmanager-ui/src/data/table/resize.rs`; page identity and persistence stay
outside this crate. Registry and coverage detail lives in
[`docs/CAPABILITY_REGISTRY.md`](docs/CAPABILITY_REGISTRY.md).

## Module map

```text
src/functional.rs                CORE-04 registry: product intent × frontend surface
│                                decision (shared/local/accepted-difference/unsupported)
src/command.rs  keybindings.rs  navigation.rs  focus.rs  message.rs
                                 shared interaction vocabulary
src/columns.rs  icon.rs  capabilities.rs   column, icon and capability contracts
src/feature_coverage.rs          FeatureId/FeatureArea coverage registry and the
│                                feature_coverage_findings gate: one explicit
│                                declaration per frontend, proving declaration
│                                discipline only, never behavioral equivalence
src/feature_coverage/            delivery semantic specs, per-feature platform
                                 binding, the read-only three-axis ledger fold,
                                 and the 225-slot blueprint disposition census
                                 with its bidirectional slot gate (ADR-059)
src/accessibility/               neutral accessibility model and snapshots
src/conformance.rs               conformance suite contracts and test helpers
src/presentation.rs              ADR-064: vocabulary → thresholds → budget →
                                 card_flow → card_width → control_ink →
                                 dashboard → parity; sizes/motion/resolution/
                                 load_tiers/dashboard in src/presentation/
```

`control_ink` owns the interaction-ink precedence every control resolves:
`Disabled > Pressed > Focused > Hovered > Selected > Idle`, plus the
selection-dominant variant a control whose selection IS its own ink (a
segmented control's active segment) resolves instead.

## Docs

- [`docs/CAPABILITY_REGISTRY.md`](docs/CAPABILITY_REGISTRY.md) — capability semantic specs,
  navigation vocabulary and the feature-coverage ledger.
- [`docs/ACCESSIBILITY_PROCESS_ROWS.md`](docs/ACCESSIBILITY_PROCESS_ROWS.md) — typed process
  observations, aggregate coverage, and tree action identity.
- [`docs/PRESENTATION.md`](docs/PRESENTATION.md) — presentation resolution charter:
  shared folds, the theme-value lift, divergence registry and capture evidence.
- [`UI_COMPONENT_ARCHITECTURE`](../../docs/UI_COMPONENT_ARCHITECTURE.md) and
  [ADR-046](../../adr/046-bevy-fourth-shared-contract-shape.md) — shared contract shape.
- [ADR-064](../../adr/064-presentation-resolution-layer.md) — presentation
  resolution layer and the capture-resolution evidence gate.
