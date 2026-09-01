# taskmanager-bevy-ui

## Role

Fourth product frontend: renders the shared neutral shell projections with
Bevy 0.19's official two-piece UI base — `bevy_ui` + `bevy_ui_widgets` —
with a hard 100% `bsn!` (Bevy Scene Notation) authoring contract: every
production UI tree, including dynamic children and state surfaces, is composed
as a `Scene` and mounted with `spawn_scene`; observers and required components
bind state to that tree but never create a second imperative UI hierarchy. Peer
surface to GPUI, Iced and the TUI. The public architecture contract lives in
[`docs/ARCH.md`](../../docs/ARCH.md), the frontend charter in
[`docs/BEVY_UI_FRONTEND.md`](../../docs/BEVY_UI_FRONTEND.md), and the frontend
identity is registered in
[`docs/PRODUCT_IDENTITY.md`](../../docs/PRODUCT_IDENTITY.md).

All nine routed pages are mounted on the widget substrate (the shared page
set is complete, System included), and the structural seams are live: real
input (keyboard through the shell's own routers, pointer picking and wheel
scroll through typed page seams), destructive verbs (EndTask/ProcessBatch/
ServiceControl through the shared gate — a typed action menu per inventory,
one confirmation modal, typed effects), the polyline chart family (hero
curves, sidebar mini-graphs, and the memory composition bar through one
gap-aware adapter), and the semantic accessibility channel (ui-contract
`SemanticSnapshot` plus `bevy_a11y` nodes published through the
`accesskit_unix` bridge on Linux). Remaining open surface is declared, never
hidden: the GPU-engine detail cards, service log streaming, startup/session
control verbs, settings persistence across sessions, the tray seam, and
multi-window composition. Feathers (the official skin system) is not
adopted — theme tokens are the only skin authority.

## Module map

- `src/app.rs` — the frontend-owned route model (eight pages), the nav rail,
  page mounting, and `ShellTrack`: the SystemParam every page reads the
  folded projection through (the page-agent data entry).
- `src/input.rs` — the real-input seam (W4): Bevy keyboard events forwarded
  through the shell's own routers (`handle_local_key`/`handle_local_char`),
  the frontend-owned route chords, the Dialog-scope Enter mapping, the
  `PendingEffects` effect bridge to the drain, and the one-shot quit forward.
- `src/confirmation.rs` — the shell's armed destructive-action gate rendered
  as one modal under the app shell root, with typed confirm/dismiss paths
  and republished gate transitions.
- `src/semantic.rs` — the accessibility seam: the ui-contract
  `SemanticSnapshot` (revision-keyed) plus `bevy_a11y` row nodes.
- `src/window.rs` — the bsn! app shell (route-aware shell + nav rail + content slot),
  including the shared MiSans VF UI and Roboto Mono telemetry font roles.
  and its observers; the bsn! idiom reference for this crate.
- `src/drain.rs` — the per-frame `PreUpdate` event-port drain; folds platform
  batches into the shell and triggers `ShellProjectionFolded` (the pages'
  data-refresh event).
- `src/palette.rs` — the theme-token → bevy adapter; the only place tokens
  become bevy colors/type metrics.
- `src/pages.rs` + `src/pages/` — eight mounted page modules, one file each
  (`content(&PageContext) -> impl Scene`), plus the mounted `process_tree.rs`
  projection and `history.rs` connector adapter: the M1
  process table, performance summary/curves/device blocks, the three
  read-only inventory tables (services/startup/sessions), settings, and the
  alert center.
- `src/widgets.rs` + `src/widgets/` — the owned component layer: table and
  sparkline pure cores + bsn! render adapters, the M1.5 bounded `chart.rs`
  projection and `control_contract.rs`, plus menu/dialog skeletons (W4).
- `src/input_contract.rs` — M1.5 Bevy-key normalization through the shared
  command router, stable semantic addresses and explicit IME ownership.
- `src/runtime.rs` — the process-wide platform client via the app-host
  `OnceLock` cache pattern.

## Boundary

Dependency whitelist is charter law: `taskmanager-application`,
`taskmanager-app-host`, `taskmanager-core`, `taskmanager-platform-contract`,
`taskmanager-shell`, `taskmanager-theme`, `taskmanager-ui-contract`,
`taskmanager-assets` and exactly-locked `bevy =0.19.1` (features `bevy_ui`,
`bevy_ui_widgets`, `bevy_scene` — the bsn! macro — plus the
render/asset/window closure; Linux adds `wayland` only) — never
`platform-runtime` or a platform crate. Bevy types never cross this crate's
public API. The two Worlds never merge: the platform client is
acquired once per process through the app-host `OnceLock` cache pattern
(`src/runtime.rs`) and every frame's `PreUpdate` drain (`src/drain.rs`)
talks to it through non-blocking `try_recv` batches with the TUI-seam-shaped
bound — no blocking collection on the UI thread, no provider inside a
frontend system. Linux windowing is Wayland-only: the bevy `x11` feature
stays off; X11 sessions are carried by the existing frontends.

The 100% `bsn!` law is a production authoring constraint, not a style
preference. `content`, page subtrees, rows/cells, widgets, overlays and all
loading/empty/error variants must be `*_scene` adapters returning `Scene` (or
composed scene values) and must enter the World through `spawn_scene`. Direct
UI `Node`/`Children` construction, `with_children`, or imperative child
spawning is forbidden in production code. ECS systems may update typed
components on scene-owned entities, wire events/focus, or despawn and replace
a bounded subtree with another `bsn!` scene. Headless fixtures and Bevy's own
plugin internals are not production UI authoring routes.
For scene polymorphism, follow Bevy 0.19's boundary: fixed composition uses
`impl Scene`, homogeneous lists use `Vec<S>`, and `Vec<Box<dyn Scene>>` is
reserved for runtime-filtered or heterogeneous children. Every boxed value
still comes from a `bsn!` Scene adapter; boxing is a type-erasure tool, not a
second UI construction route. Stable telemetry scenes update marked
components in place instead of allocating and rebuilding on every frame.

## Contract and verification

Data direction is the global one-way flow (frontend → application →
core/shell → runtime → OS); event draining, capability snapshots and refresh
scheduling reuse the existing shell/application seams, and the shared
`queue_effect` path is the only effect submission. Keyboard routing
normalizes bevy input into the shared `ShellKeyEvent` and routes through the
shared command table — same chords, same pages as the TUI; the Settings
surface keeps the frontend-local unmodified-`P` binding (TUI `p` parity).
`taskforest-b` (`src/main.rs`) is a thin composition-free entry registered
in the frontend binaries build script,
[build-frontend-binaries.sh](../../scripts/build-frontend-binaries.sh).
The Performance scene module (`src/pages/performance/scene.rs`) is the default
composition reference for new Bevy UI work.

Verify with `cargo check --locked -p taskmanager-bevy-ui --tests`,
`bash scripts/accept-bevy-interactions.sh`, and
`cargo clippy --locked -p taskmanager-bevy-ui --tests`. The 100% Scene law is
also enforced by `python3 scripts/quality/bevy_bsn_guard.py --mode enforce`;
real pixels require
`bash scripts/capture-bevy.sh` in a live Wayland compositor; its validator is
fail-closed on app_id, PID/window identity, PNG, markers, source provenance
and current worktree.
Seam and projection tests are headless in `tests/headless/` (drain bound and
idle behavior against scripted event ports, runtime-cache singleton
semantics, palette token mapping, routing/keyboard semantics and nav
highlight, virtual-table-window and sparkline math, page/widget scene
assembly on `MinimalPlugins`, plus the drain→summary-line wiring without a
compositor); real-window pixel evidence belongs to the capture flow defined
in [`docs/QUALITY_GATES.md`](../../docs/QUALITY_GATES.md). Check the shared
workspace resolution keeps `bevy_app`/`bevy_ecs` at one 0.19 version across
this crate and `taskmanager-platform-runtime`
(`cargo tree -p taskmanager-bevy-ui -d`).
