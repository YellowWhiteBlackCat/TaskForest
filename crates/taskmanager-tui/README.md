# taskmanager-tui

## Role

This crate is the terminal frontend for TaskForest's shared application layer. It uses
Ratatui 0.30.2 with Crossterm 0.29.0, connects live Linux work through
`taskmanager-platform-native`, and owns no `/proc`, `/sys`, or command execution.

```bash
cargo run --locked -p taskmanager-tui                 # live Linux telemetry
cargo run --locked -p taskmanager-tui -- --demo       # deterministic, no host actions
cargo run --locked -p taskmanager-tui -- --snapshot 120 36
cargo nextest run --locked -p taskmanager-tui -j 4
bash scripts/capture-tui.sh                            # real Niri/Alacritty evidence
```

## Boundary

The TUI consumes `taskmanager-shell` projections and owns terminal geometry,
events, menus and TestBackend behavior. It never reads platform sources.
`ShellApp` privately owns the canonical `SystemProjectionStore`; TUI rendering and input receive
only `projection()`. Demo/capture/tests inject typed shell fixture facts and cannot borrow or
assign the store directly.
Process-table and details timestamps consume the same app-host-injected local-
time observation as the desktop frontends; demo frames inject fixed UTC as an
explicit fixture rule rather than reading the terminal host.

`src/surface.rs` is the single authority for TUI-local surfaces and derives one
`TuiInputScope`; configuration I/O is owned by
the app-host background coordinator; the event loop only drains immutable
publications and submits bounded patches.

## Contract and verification

Keep real terminal evidence separate from deterministic frames; shared
boundaries are defined in `../../docs/ARCH.md` and
`../../docs/screenshots/README.md`.

## Module map

```text
src/main.rs → surface.rs  terminal.rs        terminal host and surface authority
src/demo/capture.rs                           scene readiness and normal settings input
src/runtime/                                  event loop runtime
│   └── keys.rs  modals.rs  navigation.rs  semantic.rs  seam.rs
src/ui.rs                                     page rendering root
│   ├── pages/                                page dispatch
│   ├── perf_overview.rs  perf_data.rs        performance overview
│   ├── perf_core_grid.rs  perf_gpu.rs  perf_memory.rs  perf_disks.rs
│   ├── perf_networks.rs  perf_battery.rs  perf_fan.rs  perf_npu.rs
│   ├── perf_selector_instances.rs  perf_overview_data.rs  performance.rs  sidebar_editor.rs
│   ├── process_table.rs  process_data.rs  process_details/  process_menu.rs
│   ├── process_properties.rs
│   ├── health.rs  health_data.rs  alerts.rs
│   ├── app_history.rs  boot_timeline.rs
│   ├── about.rs  settings.rs  help.rs  containers.rs
│   ├── service_dependencies_modal.rs  service_menu.rs  session_menu.rs
│   ├── startup_menu.rs  batch_menu.rs  column_menu.rs  affinity_modal.rs
│   ├── confirmations.rs  header.rs  footer.rs  frame_plan.rs
│   ├── sparkline.rs  chart_cursor.rs  highlight.rs  table_hit.rs  text.rs  units.rs
src/command_palette/  (+ surface_protocol.rs) command palette
src/bindings.rs  capabilities.rs  functional.rs  feature_coverage.rs  keys, capabilities, CORE-04
src/clipboard.rs  column_prefs.rs  preferences.rs  selection.rs  selectors.rs  sidebar.rs
src/alert_editor.rs  first_run.rs  health_review.rs  information.rs  smart_self_test.rs
src/saved_views.rs  window_capture.rs  snapshot_export.rs  diagnostic_bundle.rs
src/demo.rs  history_runtime.rs  menus.rs  process_view.rs
src/service_log.rs  startup_control.rs  theme.rs
```

## Docs

- [`docs/PAGES_AND_KEYS.md`](docs/PAGES_AND_KEYS.md) — page hierarchy, navigation keys and
  per-page performance, health, dashboard and review behavior.
- [`docs/SURFACES_AND_STATE.md`](docs/SURFACES_AND_STATE.md) — surface authority, properties
  and insights, health editor keys, settings lifecycle, exports and capture.
- [`docs/UI_GUIDE_TUI.md`](docs/UI_GUIDE_TUI.md) — TUI rendering guide.
- [`ARCH`](../../docs/ARCH.md) and [ADR-029](../../adr/029-one-binary-three-ui-shapes.md) —
  shared boundaries and the one-binary frontend products.
