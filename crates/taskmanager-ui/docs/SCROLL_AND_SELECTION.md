# Scroll, scaffold, and selection contracts

## Scroll ownership

A tracked vertical viewport is the sole owner of its `ScrollHandle` offset.
Switches attached to a scroll viewport measure their real bounds and reveal
keyboard focus without activating the control. Settings reuses this component
behavior; capture accepts focus only after the complete switch is visible.
Pinned scrollbar rails are sibling chrome in a fixed relative frame, never
children of the tracked node; real-wheel tests must prove that content moves
while the viewport and rail keep identical window-space bounds across redraws.

Long-form modal bodies use `bounded_scroll_region_with_rail` with a stable
per-window handle; its optional exact width prevents intrinsic text from
resizing dialog geometry. A modal with fixed actions or filters composes them
through `bounded_scroll_column_with_fixed_header`, which keeps that chrome
outside the tracked coordinate tree and gives the whole column one width
authority. Unrailed bounded regions are reserved for embedded sub-lists whose
parent owns the discoverable scroll affordance.
`bounded_scroll_column_with_fixed_footer` budgets the entire review column,
reserving complete action groups and a gap below its shrinkable body viewport.

## Page scaffold

`PageScaffold` is the data-page family's ONE outer shell (ADR-042): every
non-chart top-level page in `taskmanager-gpui` composes through this
viewport/frame/footer column, and no page may grow its own outer wrapper.
The render-path guard (`page_family_contract_tests`, exhaustive over
`TopPage::ALL`) proves every page paints its family root; selector
identities are shared constants (`layout::selectors`) between this crate
and the guard, so they cannot drift apart. The telemetry-readiness marker
lives on the `page_viewport` wrapper, never stamped onto the page body, so
the body keeps its family selector.

## Text selection and table resize

`SelectableText` is the read-only selection authority: stable per-element
state, UTF-8-safe pointer ranges, palette selection ink, Ctrl/Cmd+A and
Ctrl/Cmd+C, plus Linux primary-selection sync on release. A window-level
coordinator makes the active selection exclusive, so starting elsewhere clears
the previous highlight. `KeyValueRow` opts values into it with an explicit
semantic ID; dense interactive tables remain excluded until their
row-selection and text-selection arbitration is defined.

`data/table/resize.rs` is the separate `ColumnDragResize` reference surface:
it owns the typed GPUI drag payload, stable first-motion anchoring, width
transition, and outside-release wiring; page adapters retain column identity
and persistence.
