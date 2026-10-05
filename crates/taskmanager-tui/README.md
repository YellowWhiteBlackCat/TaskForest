# TaskForest TUI

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

Use `Alt+1`…`Alt+6` to change pages, arrows and page keys to navigate, `Ctrl+F` to
search, `F5` to refresh, and `Ctrl+Space` to pause. `Delete` opens an identity-frozen
confirmation; only `y` after that confirmation can submit an end-task effect.

The Applications page uses one hierarchy: Applications, Background and Uncategorized category
headers; Applications then exposes PID-less selectable application roots before their recursive
process trees, while the other categories expand directly to process rows. Legacy saved grouping
values are normalized at import and are not exposed as alternate modes.
Terminal font size remains the terminal emulator's responsibility; TUI config writes preserve the
desktop `ui_size` token.
The CPU Performance surface has no metric selector: utilization, temperature, frequency and
power facts render together, one dominant utilization history owns the main graph, and the
per-core viewport remains reachable with arrows/page keys when the terminal has room. Compact
terminals keep the facts and main graph and omit the optional core grid.
The GPU Performance surface follows the same fixed-fact rule: aggregate utilization owns its
only device-wide history, while temperature, frequency, power, idle residency and memory facts
remain simultaneous rows. Standard terminals add live-engine and opt-in PMU detail; compact
terminals keep a dense primary-fact strip plus the largest possible utilization chart and omit
the secondary engine region. Only the standard engine region scrolls; the main chart never does.
Disk source failures retain SMART status and recovery guidance even with older readings.
Battery review shows charge/power history, voltage and typed fan readings. Startup evidence
puts failed unit identities before optional chain rows so the compact view keeps the failure group.
Health exposes `p` for active warning facts and `e` for complete event transitions.
Up/Down selects whole observed groups; severity, thresholds and identity remain visible
in compact terminals, and empty reviews keep their close and navigation controls.
System `w` opens the shared dashboard; `1`–`4` select its real time window,
arrows page complete metric groups, and `b` returns to hardware facts.
The System viewport counts physical wrapped rows, so the final continuation remains
reachable in short windows. Its ordered section projection keeps NPU devices in the fixed
Graphics & accelerators section with identity, driver, aggregate utilization, every reported
engine utilization and dedicated/shared memory; unavailable observations remain dashes and no
NPU history is invented.

Persistent review uses the application controller and app-host query worker.
Performance exposes `r` to enter/leave review, `1`/`2`/`3` to select windows and
`f` to refresh. Application History shares that reader and refresh command;
compact terminals admit complete metric cards. `ui/pinned_actions.rs` reserves
wrapped controls and a bottom safety row. `runtime/keys/direct.rs` executes the
local registry and its reader-availability guards. History preference changes reconnect
through the native connector; renderers never seed replay rows.

Optional native setup is observed quietly at live startup. When a descriptor is available,
Settings exposes F2 to review it. View/Run/Revert/Restart use the shared application controller;
only the descriptor body scrolls, and wrapped action hints keep Close reachable in compact terminals.

About exposes build version, license, repository opening and a distinct System
Information review. Both use shared shell projections; `information.rs` owns
frozen system rows and clipboard payloads, while the modal protocol registry
owns action keys and fixed footer hints. Only the information body scrolls. Native
boot submits desktop appearance through the shared effect seam and caches its response.

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
`TuiInputScope` for keyboard and pointer routing. Confirmations and process-properties
visibility remain owned by the shared application `InteractionState`; the TUI's optional
process-properties view model is a render cache only and cannot make that surface visible.
Properties freeze identity and resolve current canonical facts by live key.
Their Performance tab shows current values, peaks and four bounded history
trends through the terminal component's Unicode or ASCII repertoire; missing
history remains collecting and recording gaps remain visible.
Their Insights tab uses 1–7 to select complete facets and `r` to refresh that
same target; denied or partial network traffic exposes typed authorization.
Only the bounded body scrolls, and PID reuse never redirects an open review.
Opening a new surface replaces the prior owner, and stale typed dismiss events are no-ops.
Search, Help and Suggestions are branches of the shared shell's one
`ShellInputMode`, so the terminal cannot carry contradictory keyboard owners.
Bare-key and command-palette exits submit distinct typed quit reasons. The
footer lives in `src/ui/footer.rs` and reads the shell's single feedback
projection; TUI settings, clipboard, persistence and control paths publish
typed notices instead of mutating a shared status string.
The Health surface reads the shell's canonical managed alert rules directly;
disabled rules stay listed and labelled. Threshold suggestions remain a
separate read-only evidence projection and cannot become rule authority.
Health exposes `n` add, `d` remove, `m` metric, `u/o` threshold, `f/b` duration,
`g/l` hysteresis, `v` severity and `t` disk target. `y` exports through the terminal
clipboard; `a` arms merge and `r` arms replacement for the next JSON paste.
Compact terminals retain the selected rule, four editor rows and fixed action hints;
the optional device summary and event group are admitted only with their full budget.
Health `w` selects filesystem and SMART reports; `s` selects sensors and `q`
returns to rules. Arrows visit complete groups, including denied readings and
the last observed report. `z`/`x` arm short/extended tests through confirmation.

Configuration I/O is owned by the app-host background coordinator. The event
loop only drains immutable publications and submits bounded patches. The
uncomposed `TuiApp::new`/`from_shell`/demo constructors perform no host
discovery; only `runtime::run_live` creates `NativeAppHost` and injects its
`ConfigClient` and local-time observation. The frontend stores neither a
configuration path nor a fallback coordinator.
The Settings form has a typed Clean/Dirty/Conflict lifecycle: an external revision
updates runtime preferences but never discards a dirty form or permits its stale
unedited fields to overwrite the new canonical snapshot; Cancel reloads the
latest snapshot before editing may resume.
Settings F3 opens concrete device editing. Arrows select and reorder stable keys; Space
toggles visibility through the shared core rule and coordinator. Enter opens that identity;
removal displays disconnection rather than another device. Title and actions stay fixed around
whole rows, and the same choices order/filter the selector and detail projection.
Its continuous-history control uses the same canonical config field as both desktop frontends.
Enable enters a non-blocking `Connecting` reader state; disable immediately drops replay, while
the frontend-owned writer follows the preference during the TUI lifetime. The History page reads
only durable application CPU/memory/process-count series for 1h/24h/7d and preserves recording
downtime as visible trend gaps. Disk and network device blocks render
labeled read/write and rx/tx sparkline rows from the store's split-direction
lanes under one shared normalization (the one deliberate exception to per-row
scaling), with per-direction gap glyphs and independent warm-up gating; the
summed throughput summary below them stays on the summed lane.
Diagnostic export first opens a sanitized review. Enter confirms the displayed plan, Esc
invalidates its session, and late completions cannot reopen it. Only the body scrolls;
wrapped action hints and a bottom safety row remain reserved at compact sizes.
Snapshot export follows the same ownership rule: the key path submits one
typed request to the app-host worker, and the event loop drains its correlated
completion into shell feedback. The terminal thread never serializes or writes
the three artifacts.
Service inventory consumes `ServiceItem` descriptors without materializing
legacy relation strings; the canonical typed relation graph remains read-only
at the terminal boundary.
GPU captions, details and graph samples likewise consume only current typed
scalar/throttle accessors, so unavailable provider facts remain gaps.
Real terminal evidence can target these non-default surfaces with
`TM_TUI_CAPTURE_DEVICE=gpu bash scripts/capture-tui.sh` and
`TM_TUI_CAPTURE_SCENE=system-npu bash scripts/capture-tui.sh`.

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
src/bindings.rs  capabilities.rs  functional.rs  keys, capabilities, CORE-04
src/clipboard.rs  column_prefs.rs  preferences.rs  selection.rs  selectors.rs  sidebar.rs
src/demo.rs  diagnostic_bundle.rs  snapshot_export.rs
src/history_runtime.rs  menus.rs  process_view.rs
src/service_log.rs  startup_control.rs  theme.rs
```
