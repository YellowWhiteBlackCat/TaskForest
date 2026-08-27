# ADR-017: Own the UI component layer and remove gpui-component

Status: accepted

## Context

The GUI is built on `gpui` 0.2.2 plus `gpui-component` 0.5.1. Three facts make
`gpui-component` a liability rather than an asset for this project:

1. **It is a third-party library, not the official component stack.** Zed's
   official `ui` crate lives inside the Zed workspace, is GPL-3.0-or-later,
   and is coupled to Zed-internal crates (`theme`, `menu`, `component`,
   `ui_macros`, …). It is not a standalone, stable, third-party-facing release.
   `gpui-component` belongs to Longbridge and sits on top of the pre-1.0 GPUI,
   where breaking changes are frequent.

2. **We already carry a vendored patch.** Upstream uses the single
   `ThemeColor::background` token for both the window backdrop and Dialog/panel
   surfaces. Linux CSD windows need a transparent surface so we paint our own
   rounded corners, so `patches/gpui-component/` changes `Root::render` to a
   transparent backdrop. The patch is one intentional line, but every upgrade
   and every new gc dependency is now a maintenance cost we pay for code we did
   not choose.

3. **The application's real need is small.** The app imports only eight gc
   modules (`table`, `input`, `slider`, `menu`, `dialog`, `button`, `switch`,
   plus `Icon`/`IconName`/`Root`/`Theme` globals). The other 60+ components
   (dock, editor, markdown, charts, inspector, …) provide no value. The
   project's real assets are its business widgets — `ProcessTable`,
   `ProcessTree`, `PerformanceChart`, `ProcessDetails`, `ResourceSummary` —
   which a generic component library will never provide.

Meanwhile the project already owns the foundations this replacement needs:
`crates/taskmanager-gpui/src/gpui_app/theme.rs` (1473 lines) + `theme/`, `tokens.rs`, `elements.rs`
(1191 lines), `icons.rs`, `modal_focus.rs`, and the toolkit-neutral contracts
in `taskmanager-ui-contract` (accessibility, focus, commands, `IconId`).

`gpui-component` is Apache-2.0 and its 0.5.1 source is already vendored in
`patches/gpui-component/`, so its algorithms can be read and absorbed offline.
Zed's code is GPL and is a read-only reference for architecture only.

## Decision

The final state is **zero `gpui-component`**: no dependency, no
`[patch.crates-io]` entry, no `patches/gpui-component/` directory. The GUI
component layer is owned by this repository, structured like Zed's
`crates/theme` + `crates/icons` + `crates/ui` split:

- `crates/taskmanager-theme` — theme tokens, registry, skin/mode/high-contrast
  axes; absorbs the gc theme token structure.
- `crates/taskmanager-icons` — GPUI-side rendering adapter for the semantic
  `IconId` contract (which stays in `taskmanager-ui-contract` for the TUI).
- `crates/taskmanager-ui` — the component library:
  `primitives/`, `inputs/`, `overlays/`, `data/`.
- `crates/taskmanager-gpui/src/gpui_app` — business views and window assembly only.

Dispatch policy, fixed for every component in scope:

| Class | Policy |
|---|---|
| Complex interaction logic | Absorb gc source (Apache-2.0, vendored offline): table virtualization, popup positioning/dismissal, modal stack, text-input state machine, theme token schema. Reimplement as ours, never re-export gc. |
| Simple controls and business widgets | Build from scratch on gpui; gc is not consulted. |
| Cross-cutting patterns | Owned centrally: theme tokens, State/Element separation, overlay layer stack, focus management, accessibility bridge, keyboard conventions, icon assets. |

Zed's `ui`/`theme` crates are study material (architecture and interaction
conventions) only; no file-by-file copying under GPL.

The six-phase migration is complete. Theme, icons, primitives, inputs,
overlays, and data widgets are owned by TaskForest; the old dependency,
vendored patch, theme bridge, compatibility wrappers, and migration inventory
were removed together. `docs/UI_COMPONENT_ARCHITECTURE.md` defines the current
component boundary.

`tests/logic/ui_component_boundary.rs` enforces the lasting dependency rule:
repository code has zero `gpui_component` references. It has no migration
allowlist and is not evidence that partial migration is acceptable.

## Consequences

- **Owned behavior**: table, menu, dialog, input, focus, overlay, and theme
  state machines live in the TaskForest component crates and are reviewed as
  product code.
- **Dependency independence**: no vendored gc patch, theme-global bridge, root
  wrapper, import budget, or compatibility path remains; upgrades are no
  longer dictated by a third-party pre-1.0 component API.
- **Test surface**: headless tests exercise the owned overlay host and component
  behavior directly. Visual changes retain the full evidence chain: behavior,
  failure semantics, headless geometry, capture markers, and real screenshots.
- **Cross-frontend boundary**: `taskmanager-ui-contract` remains
  toolkit-neutral; GPUI-specific implementation stays in the owned component
  and frontend layers.
- The upstream token-collision lesson is preserved in the architecture doc so
  the same mistake cannot be re-introduced in `taskmanager-theme`.
