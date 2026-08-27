# ADR-028: Iced frontend — the third peer frontend

Status: accepted — architecture/first-slice scope

## Context

ADR-027 landed the shared renderer-independent shell state
(`taskmanager-shell`) and reduced the TUI to rendering. This ADR establishes
the third frontend's architecture slice: `taskmanager-iced` becomes a peer of
the GPUI desktop shell and the ratatui TUI, running the same shell state
machine, the same
neutral design tokens (ADR-026) and the same application platform ports.

During integration an upstream blocker surfaced: iced 0.14 pulls
wgpu 27 / naga 27.0.3, and naga 27.0.3 has a regression — its error
formatting falls back to a `String` writer when its `termcolor` feature is
off, which fails to compile once `codespan-reporting`'s `termcolor` feature
is unified on by gpui's naga 25 (both chains share the codespan-reporting
crates in one workspace graph; the probe crate compiled only because that
feature was never actually exercised without the sccache cache). The fix is
a direct `naga` feature pin in the iced frontend's manifest.

## Decision

### New crate: `taskmanager-iced` (third frontend, peer of the others)

- iced 0.14 (wgpu + tiny-skia renderers; taken with `default-features = false`
  and an explicit feature set that drops the default `x11` backend — strict
  Wayland, mirroring the GPUI wayland-only edge; the umbrella crate's only
  0.14.x release is 0.14.0, with sub-crate patches such as `iced_widget`
  0.14.2 resolved by the lock);
- `theme.rs` — `iced_theme(&Theme) -> iced::Theme` (`Theme::custom` with the
  skin palette) plus `color()` and panel/row/button style helpers, all from
  the neutral tokens (ADR-026) — the theme dependency is taken with default
  features, so **the iced frontend links zero gpui**;
- `keys.rs` — iced keyboard events normalized into `ShellKeyEvent`;
- `app.rs` — `IcedApp` (shell state + `PlatformClient` behind an
  `Rc<RefCell<Option<...>>>` so the iced boot closure, which must be `Fn`, can
  hand the client over exactly once) and the `Message` loop
  (tick/subscription, keyboard, page tabs, search, end-task confirmation);
- `ui.rs` — the iced view: page tabs, CPU/memory/swap gauges, the shared
  filtered/sorted process table, a typed Services inventory table with
  loading/empty/ready states, a typed System facts/telemetry summary with
  independent loading states, a read-only typed Startup inventory table, a
  read-only typed Users session table, shared row selection, the Disconnect/Lock action bar, the
  end-task confirmation bar, and `ui/overlays.rs` for shell-owned keyboard-help and threshold-suggestion
  modal projections;
  `a11y.rs` — a bounded detached `SemanticSnapshot` projection shared with
  GPUI's accessibility contract; unavailable process scalars remain explicit
  and active modals expose a typed Dialog/Dismiss node;
  `focus.rs` — an Iced `advanced`-feature adapter with stable operation IDs and
  custom `operation::Focusable` wrappers for page tabs, search, end-task,
  confirmation, Users session actions, and the four typed table row views;
  the Users row menu, complete control feedback, overlay focus/keyboard coverage, Iced pixel evidence,
  native accessibility bridge, and real permission receipt remain explicitly scoped next interaction slices;

The iced 0.14 focus audit remains an explicit upstream boundary (audited
against the 0.14 line: umbrella `iced` 0.14.0 with the widget-layer patch
`iced_widget` 0.14.2 — no umbrella "0.14.2" release exists). Its stock
`button`, `container`, and `scrollable` do not implement
`operation::Focusable`, so `focus.rs` enables Iced's `advanced` feature and
provides a renderer-local custom widget/registry for actionable controls and
table rows. The shell-owned `ShellApp::table_row_count` is the only row-count
authority; after shared ArrowUp/ArrowDown and Applications PageUp/PageDown
selection handling, the adapter projects the selected index to the current
renderer-local row focus, with search/empty/modal guards. The modal view
returns only the overlay tree, while the pending end-task confirmation focuses
its real Confirm control; the adapter routes initial focus, Tab/Shift+Tab
containment, Enter/Space activation, pointer focus, row selection, and exact
trigger restoration through Iced operations/messages; shell state remains
toolkit-neutral. Table-row keyboard semantics, the full interaction/semantic
matrix, and the native accessibility bridge remain open. The independent
`scripts/capture-iced.sh` receipt
`target/iced-evidence/20260808T144448Z_803912a9521e_dirty_3663850/` is
validator-PASS at 1048×763 and was reviewed, but its default Performance frame
proves current window/binary binding only, not row-focus pixels. A detached
semantic snapshot or this single frame cannot substitute for those full
matrices or native receipts.
- `taskmanager-app-host` — the shared composition edge: it selects the native
  adapter, creates the `PlatformClient`, and supplies the native config/history
  paths; `main.rs` and the three UI launchers consume that seam. `--demo` stays
  fixture-only with no host I/O (mirrors the TUI runtime and `src/main.rs`).

### naga 27.0.3 pin (upstream bug workaround)

`taskmanager-iced` declares
`naga = { version = "=27.0.3", default-features = false, features = ["termcolor"] }`
so naga 27 compiles through its correct `NoColor` diagnostic path whenever
the workspace unifies `codespan-reporting`'s `termcolor` feature (which
gpui's naga 25 chain enables). Without the pin, the workspace build fails in
naga's `error.rs`/`span.rs` (`String: WriteColor`). This is a dependency
feature pin, not a fork.

### Windows renderer dependency family

Iced 0.14's Windows renderer resolves through wgpu 27.0.1 and
`wgpu-hal` 27.0.4. That `wgpu-hal` release uses `windows` 0.58 for DX12,
while `gpu-allocator` 0.27 accepts a range that also permits 0.57. The
workspace contains an unrelated optional GPUI screen-capture edge that keeps
0.57 resolvable; allowing Cargo to reuse it makes the allocator and wgpu-hal
exchange incompatible `windows-core` interface types.

The Iced manifest therefore declares the renderer's target-only `windows`
0.58 family, and the checked-in lockfile pins the `gpu-allocator` dependency
edge to `windows` 0.58. The older 0.57 package may remain for an unrelated
optional workspace edge, but it must not appear below Iced's allocator. This
is dependency-resolution hygiene, not a new Windows API surface: Iced still
uses wgpu/winit safe public APIs and does not call the Windows crate directly.

### Rules (with ADR-027)

1. **Every frontend state transition lives in `taskmanager-shell`** (ADR-027). A
   frontend's `update`/input loop only maps toolkit events onto shell
   operations and renders shell state; it never re-implements page routing,
   filtering, sorting or platform-batch application.
2. **Platform access stays at the composition edge.** Only each frontend's
   binary names `taskmanager-platform-native`; the shell reaches the
   platform exclusively through `taskmanager-application` ports.
3. **No feature may live in only one frontend.** New behavior is added to
   `taskmanager-core`/`taskmanager-application`/`taskmanager-shell` first;
   frontends only render it (the firewall test
   `frontends_execute_no_native_commands_and_read_no_native_paths` guards
   the I/O side).
4. **The theme crate stays toolkit-neutral** (ADR-026); each frontend maps
   it at its own edge.

## Consequences

- The TUI loses ~1600 lines of duplicated state-machine surface and gains
  the shared `queue_effect`/local-binding logic; its 39 headless tests move
  with the state machine to `taskmanager-shell` (19 tests) plus the
  rendering tests that stay TUI-specific.
- The iced frontend is an architectural peer from day one: same shell state,
  same demo fixtures, same theme tokens, same platform port. The current
  implementation remains a first slice; Services and System now have real
  typed read-model projections and unknown performance metrics stay unavailable
  instead of becoming zero; Startup controls are intentionally not exposed
  until the application authority path is connected, while complete views, failure states, permissions,
  interaction and pixel evidence remain open.
- The dependency firewall now tracks `taskmanager-shell` and
  `taskmanager-iced` edges; the theme-neutrality gate checks the iced
  frontend consumes the theme without the gpui feature.
- Workspace builds pull wgpu/naga (iced's renderer chain); the naga feature
  pin documents why it is required. The release-bloat trend gate must
  compare like-for-like (the iced frontend is a separate binary, not part of
  the GPUI binary).
- Current references: `docs/ARCH.md` defines the three-frontend boundary and
  `crates/taskmanager-iced/README.md` owns Iced implementation details.

This ADR defines the Iced architecture and its current first slice. The frontend
must not claim a capability until the corresponding view, failure/permission
behavior, interaction regression, current-build evidence and target-environment
validation are present.
