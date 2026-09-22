# taskmanager-ui

## Role

TaskForest's own GPUI component layer (ADR-017 Phase 3/4): primitives,
inputs, and overlays built directly on gpui, taskmanager-theme (`Palette`),
taskmanager-icons, and taskmanager-ui-contract. No `gpui_component`.

This crate is the SEMANTIC REFERENCE SOURCE for the parallel component
layers (Iced/TUI/Bevy, GPUI-05): every capability in the ui-contract registry
names a component here through `ComponentCapability::reference_path`.
Parallel frontends port these semantics; a gap or bug found while porting
is written back to this crate (and the contract, if the vocabulary is
wrong) first — never forked silently. The GPUI capability gate test fails
if a registry entry points at a component that does not exist.

- `focus` — modal focus trap / restore (absorbed from `crates/taskmanager-gpui/src/gpui_app/modal_focus.rs`)
- `primitives/` — button, icon_button, label, selectable_text, badge, divider, spinner,
  progress, tooltip, scrollbar, pill, toolbar, state_panel, card_surface
- `layout.rs` — bounded page viewport/frame/scaffold and scroll-region contracts
- `data/` — table, column resize, virtual list, tree, highlighter, data row, key/value row
- `inputs/` — switch, slider, checkbox, text_input, search_input
- `overlays/` — layer_stack, dialog, popup, context_menu, dropdown_menu, toast
- `styled.rs` — palette-driven style helpers (no hand-written colors)

## Boundary

This crate owns reusable GPUI presentation only; page-specific state remains in
`taskmanager-gpui`, while semantics remain in `taskmanager-ui-contract`.

## Contract and verification

Keep focus, disabled, loading, failure and token behavior covered by component
tests and the current visual evidence route in
`../../docs/screenshots/README.md`.

A tracked vertical viewport is the sole owner of its `ScrollHandle` offset.
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

`PageScaffold` is the data-page family's ONE outer shell (ADR-042): every
non-chart top-level page in `taskmanager-gpui` composes through this
viewport/frame/footer column, and no page may grow its own outer wrapper.
The render-path guard (`page_family_contract_tests`, exhaustive over
`TopPage::ALL`) proves every page paints its family root; selector
identities are shared constants (`layout::selectors`) between this crate
and the guard, so they cannot drift apart. The telemetry-readiness marker
lives on the `page_viewport` wrapper, never stamped onto the page body, so
the body keeps its family selector.

`SelectableText` is the read-only selection authority: stable per-element state,
UTF-8-safe pointer ranges, palette selection ink, Ctrl/Cmd+A and Ctrl/Cmd+C,
plus Linux primary-selection sync on release. A window-level coordinator makes
the active selection exclusive, so starting elsewhere clears the previous
highlight. `KeyValueRow` opts values into it with an explicit semantic ID;
dense interactive tables remain excluded until their row-selection and
text-selection arbitration is defined. `data/table/resize.rs` is the separate
`ColumnDragResize` reference surface: it owns the typed GPUI drag payload,
stable first-motion anchoring, width transition, and outside-release wiring;
page adapters retain column identity and persistence.

## Module map

```text
src/primitives.rs                 foundational UI primitives
│   ├── button.rs  icon_button.rs  label.rs  selectable_text.rs
│   ├── badge.rs  pill.rs  divider.rs  spinner.rs  progress.rs
│   ├── toolbar.rs  section_header.rs  state_panel.rs  card_surface.rs
│   ├── tooltip.rs  scrollbar/  segmented.rs  motion.rs
src/data.rs                       data display components
│   ├── table/                    columns, interaction, model, render, resize
│   ├── tree.rs  virtual_list/    hierarchical tree and virtualized list
│   ├── highlighter.rs  row.rs  key_value_row.rs
src/inputs.rs                     input components
│   ├── checkbox.rs  select.rs  slider.rs  switch.rs
│   ├── text_input/  search_input.rs
src/overlays.rs                   overlay components
│   ├── context_menu.rs  dialog.rs  dropdown_menu.rs
│   ├── popup.rs  layer_stack.rs  toast.rs
src/layout.rs  layout/adaptive_grid.rs    page viewport/scaffold and grid
src/focus.rs                      modal focus trap/restore
src/styled.rs                     palette-driven style helpers
src/icons_binding.rs  theme_binding.rs    icon and theme integration
```

Toolkit-neutral component contracts; each frontend materializes them (ADR-017/020/026).
