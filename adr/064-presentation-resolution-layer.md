# ADR-064: Presentation resolution layer

## Status

Current. Extends ADR-020/026/061/063 and amends the "存异边界" clause of
ADR-027/ARCH § 8.2.

## Decision

The shared UI contract gains one presentation layer above the semantic
contracts. It owns what every frontend must AGREE on about measurement and
motion: the layout vocabulary and classification folds, the measured-band
registry, the control-role matrix and hit-target floor, the motion-role/curve
matrix and forbidden animation sites, and the cross-shape probe grid that proves
a differing verdict is explained by a registered band.

Each frontend owns one resolver binding — GPUI window rem, Iced scaled logical
pixels, Bevy scene rem, TUI terminal cells — and one motion-engine binding. A
frontend supplies what only it can know (the chrome it actually spends, its
unit conversion) and consumes the shared verdict. It never re-derives a bucket,
a padding, a rung, an admission rule, a control measurement, or a motion class.

`taskmanager-theme` remains the only owner of design NUMBERS (ADR-063). The
presentation layer owns the ROLES and FOLDS; a cross-crate parity gate welds the
two numeric tables, so neither can drift. A deliberate per-shape difference is
an explicit row in `presentation::thresholds` with an architecture driver, or
an `unrecorded:` debt cell in the gate-checked census. "(GPUI parity)" prose is
no longer evidence (ARCH § 8.3).
The theme's numbers enter the layer as plain value points (`UiSizePoint`,
`DensityPoint`, `MotionDurations`) lifted at each frontend's binding seam, so
the publishable contract crate carries zero token-codec dependency and stays
publishable without widening the published surface to the theme.

## Consequences

- The GPUI and Iced local layout enums, breakpoints and slot-arbitration
  implementations are deleted in the same change; no alias, wrapper, or
  migration wait-list survives (hard cutover).
- A new frontend writes one resolver binding and fills the threshold/binding
  registries; it does not copy a layout budget or a motion table.
- Layout geometry, hit targets, cached projection values and virtualized-row
  state are never animated. Only opacity/color/progress roles may tween, and
  `NoMotion` paints the final state without a zero-duration animation.
- Real capture receipts must carry a `presentation_resolution` block whose
  source digest and resolution digest match the current build; a PNG alone is
  no longer proof that the shared measurement contract was painted.
- `taskmanager-ui-contract` is publishable, so this new public API is subject
  to the recorded semver baseline (ADR-060).

## Decision chain

ADR-020 → ADR-026 → ADR-051 → ADR-063 → ADR-064. ADR-064 amends the
"layout budget belongs to the render权" clause of ARCH § 8.2: the budget
vocabulary and folds are now shared, while the measured chrome input and the
unit realization remain per-shape.
