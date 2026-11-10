# Presentation resolution charter

The shared layer ADR-064 adds above the semantic contracts. It answers "求同存异"
for measurement and motion: which facts all four frontends must agree on, and
which facts each frontend owns. Current state; no history.

## One-way layering

```text
vocabulary   buckets, classification folds, shared bands (no per-shape numbers)
   ↓
thresholds   each shape's registered bands + the reason-bearing debt census
   ↓
budget       measured slot → typed page and slot allocation
   ↓
parity       probe grid and the unexplained-difference rule
sizes        control-role matrix and hit-target floor
motion       motion-role/curve matrix and forbidden sites
resolution   the frame's resolved facts and the re-deriving verifier
load_tiers   the per-core load-tier band edges + the clamp-and-bucket fold
```

Intent, metric and classification are shared ("求同"). Units, chrome deduction
and realization belong to the shape ("存异"): GPUI window rem, Iced scaled
pixels, Bevy scene rem, TUI terminal cells. A frontend consumes the verdict;
it never re-derives a bucket, padding, rung, control measurement or motion class.

## Numbers have one owner; roles have another

ADR-063 keeps every design number and axis token codec in `taskmanager-theme`.
This crate is the publishable ROLE owner and stays independent of that codec:
the theme's values enter the fold as `UiSizePoint`, `DensityPoint` and
`MotionDurations` lifted at each frontend's `presentation_binding.rs`. The
frontend applies the theme's `MotionPolicy` to a duration class before the
contract sees it. This keeps the released crate publishable without widening
the published surface to the non-published theme crate, and a gate welds the
lift to the real ladder on every frontend.

## Divergence is a registered fact

A deliberate per-shape difference is an explicit row in `thresholds` with an
architecture driver, or an `unrecorded:` debt cell in the census. The probe
grid computes `unexplained_differences(left, right)`; "(GPUI parity)" prose is
not evidence (ARCH § 8.3).
`load_tiers` applies the same discipline to the per-core load band: Iced, the
TUI and Bevy all paint the reference edges, and GPUI declares the axis absent
because its matrix tints by CPU type. The ink each tier wears and the
missing-sample convention stay per-shape bindings with one settled rule: the
calm tier binds the theme's success ink and an unobserved reading binds the
dim missing ink, because the theme reserves the accent for interactive states
and a positive state never borrows it.

The census is answered cell by cell, never shape by shape. A cell closes when
the declaration says what the shape really spends: the number it subtracts is
bound from the registry into its own `const`, the mechanism that replaces a
band is named, or the row stays `unrecorded:` debt. Bevy now spends its rail
width (`HorizontalChromeReserve`, which its own `RAIL_WIDTH` binds) and
charges no rail inset (`NavigationHorizontalInset = 0.0`, because the
scaffold's page padding already separates rail from body), and names what
stands in for the three bands it does not spend: the flex-composed chrome row
behind a declared scroll owner, the pinned scroll-owned statistics rail with
no stacked form, and the layout engine's own chrome spending instead of a
subtracted reserve. Bevy now resolves the same measured page slot through
`ContentSlot` / `PageLayoutBudget` / `PerformancePageBudget`: the shared
Constrained/Generous buckets decide its vertical whole-group admission and the
Charts rung decides the extended chart inventory, so those three axes are
`Shared` instead of `unrecorded:`. The frame's typed verdict is the only input
the page markers consume; the raw window height no longer decides a group.

## Evidence

A capture that paints a frame also records the presentation facts it resolved.
`resolution` re-runs the shared folds on those recorded inputs, so a PNG alone
is no longer proof the shared measurement contract was painted. The capture
receipt carries a source digest and a resolution digest matched against the
current build; the quick gate runs the guard's self-test and the standard gate
verifies real receipts when `target/capture-receipts` exists.

## Checks

- `cargo nextest run -p taskmanager-ui-contract -j 4` — the in-crate vocabulary,
  threshold, budget and parity folds.
- `cargo nextest run -p taskmanager-gates --test logic -j 4 presentation` —
  the cross-crate theme weld, four-shape resolution, and capture records.
- `scripts/quality/presentation_resolution_guard.py --self-test` — receipt
  shape, digest and four-frontend coverage.
- `scripts/capture_publish.py --self-test` — the capture provenance block.

Shell-facing layout work also follows
[`docs/ELASTIC_LAYOUT_PLAYBOOK.md`](../../../docs/ELASTIC_LAYOUT_PLAYBOOK.md).
