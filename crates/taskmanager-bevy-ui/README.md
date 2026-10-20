# taskmanager-bevy-ui

## Role

Fourth product frontend: renders the shared neutral shell projections with
Bevy 0.20's official two-piece UI base — `bevy_ui` + `bevy_ui_widgets` —
with a hard 100% `bsn!` (Bevy Scene Notation) authoring contract: every
production UI tree, including dynamic children and state surfaces, is composed
as a `Scene` and mounted with `spawn_scene`; observers and required components
bind state to that tree but never create a second imperative UI hierarchy. Peer
surface to GPUI, Iced and the TUI. The public architecture contract lives in
[`docs/ARCH.md`](../../docs/ARCH.md), the frontend charter in
[`docs/BEVY_UI_FRONTEND.md`](docs/BEVY_UI_FRONTEND.md), and the frontend
identity is registered in
[`docs/PRODUCT_IDENTITY.md`](../../docs/PRODUCT_IDENTITY.md).

All nine routed pages are mounted on the widget substrate, and the structural
seams (real input, destructive verbs, the polyline chart family and the
semantic accessibility channel) are live; remaining open surface is declared
in [`docs/PAGE_SEAMS.md`](docs/PAGE_SEAMS.md), never hidden. Feathers (the
official skin system) is not adopted — theme tokens are the only skin
authority.

## Module map

- `src/app.rs` — frontend route model (nine pages), nav rail, page mounting, `ShellTrack`.
- `src/input.rs` — real-input seam: shell routers, route chords, action menus, effect bridge.
- `src/window_surface.rs`, `first_run_modal.rs`, `about_modal.rs`,
  `system_information_modal.rs` — the single modal surface family.
- `src/text_selection.rs`, `confirmation.rs`, `focus_visible.rs`, `semantic.rs` — clipboard,
  destructive gate, focus ring and accessibility seams.
- `src/window.rs` — the bsn! app shell and font roles; the bsn! idiom reference.
- `src/drain.rs`, `palette.rs`, `input_contract.rs`, `runtime.rs` — event drain, theme
  adapter, key normalization and the process-wide platform client.
- `src/pages.rs` + `src/pages/` — nine mounted pages plus `process_tree.rs` and
  `history.rs`; `src/widgets.rs` + `src/widgets/` — the owned component layer.
- Per-module detail: [`docs/MODULE_NOTES.md`](docs/MODULE_NOTES.md).

## Boundary

Systems and observers declare specific resources and component queries. The
[frontend charter](docs/BEVY_UI_FRONTEND.md) forbids unrestricted world
access; composition registers the access sets and coalesced paint systems.

Properties reserve title, identity, wrapping selectors and complete footer
actions. The remaining body owns scrolling and an eight-pixel final inset;
labels and values occupy separate bounded rows. Charts retain their readable
floor; all facts remain reachable at short and narrow sizes. Refresh binds the
frozen live key, coalesces pending requests and publishes typed independent
availability. System Dashboard selects the shared time windows over long host
rings; component budgets admit whole metric groups, and insufficient space has
an explicit resize state. System actions stay fixed above one native scroll
area; a successful SMBIOS or NPU lane remains visible while static hardware is
unavailable.

The 100% `bsn!` authoring law lives in
[`docs/BSN_AUTHORING.md`](docs/BSN_AUTHORING.md); the rem sizing axis (the
type ladder rides rem, never px), the dependency whitelist,
locked Bevy version and two-Worlds rule are charter law in
[`docs/BEVY_UI_FRONTEND.md`](docs/BEVY_UI_FRONTEND.md). Bevy types never cross
this crate's public API; Linux windowing is Wayland-only.

## Contract and verification

Data direction is the repository's global one-way flow
([AGENTS.md](../../AGENTS.md)); event draining, capability snapshots and
refresh scheduling reuse the existing shell/application seams, and the shared
`queue_effect` path is the only effect submission. Keyboard routing
normalizes bevy input into the shared `ShellKeyEvent` and routes through the
shared command table — same chords, same pages as the TUI; the Settings
surface keeps the frontend-local unmodified-`P` binding (TUI `p` parity).
`taskforest-b` (`src/main.rs`) is a thin composition-free entry registered in
[build-frontend-binaries.sh](../../scripts/build-frontend-binaries.sh). The
Performance scene module (`src/pages/performance/scene.rs`) is the default
composition reference for new Bevy UI work.

Verify with `cargo check --locked -p taskmanager-bevy-ui --tests`,
`bash scripts/accept-bevy-interactions.sh`,
`cargo clippy --locked -p taskmanager-bevy-ui --tests`,
`python3 scripts/quality/bevy_bsn_guard.py --mode enforce` (100% `bsn!` law)
and `python3 scripts/quality/bevy_rem_guard.py --mode enforce` (rem sizing
axis); guard details,
headless suite and capture flow live in
[`docs/VERIFICATION.md`](docs/VERIFICATION.md) and
[`docs/QUALITY_GATES.md`](../../docs/QUALITY_GATES.md).

## Docs

- [`docs/BEVY_UI_FRONTEND.md`](docs/BEVY_UI_FRONTEND.md) — frontend charter (定位、边界铁律、
  发布地位), [`docs/UI_GUIDE_BEVY_UI.md`](docs/UI_GUIDE_BEVY_UI.md) — widget/rendering guide.
- [`docs/PAGE_SEAMS.md`](docs/PAGE_SEAMS.md) — page modules, per-page seams and declared open surface.
- [`docs/MODULE_NOTES.md`](docs/MODULE_NOTES.md) — per-module detail behind the module map.
- [`docs/BSN_AUTHORING.md`](docs/BSN_AUTHORING.md) — the 100% `bsn!` law, scene polymorphism
  and the dependency whitelist.
- [`docs/VERIFICATION.md`](docs/VERIFICATION.md) — commands, guards, headless suite and capture.
- [`UI_COMPONENT_ARCHITECTURE`](../../docs/UI_COMPONENT_ARCHITECTURE.md) and
  [ADR-046](../../adr/046-bevy-fourth-shared-contract-shape.md) — shared contract shape.

共享组件扩展与依赖升级遵守 [跨产品共享包契约](../../docs/SHARED_UI_PACKAGES.md)。
