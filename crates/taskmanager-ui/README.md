# taskmanager-ui

## Role

TaskForest's own GPUI component layer (ADR-017): primitives,
inputs, and overlays built on GPUI Kit's GPUI re-export, taskmanager-theme (`Palette`),
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
- `data/` — table, column resize, virtual list, tree, data row, key/value row
- `inputs/` — switch, slider, checkbox, text_input, search_input
- `overlays/` — layer_stack, dialog, popup, context_menu, dropdown_menu, toast
- `styled.rs` — palette-driven style helpers (no hand-written colors)

## Boundary

This crate owns reusable GPUI presentation only; page-specific state remains in
`taskmanager-gpui`, while semantics remain in `taskmanager-ui-contract`.

## Contract and verification

Keep focus, disabled, loading, failure and token behavior covered by component
tests and the current visual evidence route in
`../../docs/screenshots/README.md`. Scroll ownership, the page-scaffold guard
and the selection/resize reference surfaces are specified in
[`docs/SCROLL_AND_SELECTION.md`](docs/SCROLL_AND_SELECTION.md).

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
│   ├── row.rs  key_value_row.rs
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

## Docs

- [`docs/SCROLL_AND_SELECTION.md`](docs/SCROLL_AND_SELECTION.md) — scroll ownership,
  `PageScaffold` render-path guard, `SelectableText` and `ColumnDragResize` references.
- [`UI_COMPONENT_ARCHITECTURE`](../../docs/UI_COMPONENT_ARCHITECTURE.md) and
  [ADR-017](../../adr/017-own-ui-component-layer.md) — cross-frontend component law.
