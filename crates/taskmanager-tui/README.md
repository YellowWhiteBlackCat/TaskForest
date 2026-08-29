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
The System page is an ordered, scrollable section projection. NPU devices stay in the fixed
Graphics & accelerators section with identity, driver, aggregate utilization, every reported
engine utilization and dedicated/shared memory; unavailable observations remain dashes and no
NPU history is invented.

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
Its continuous-history control uses the same canonical config field as both desktop frontends.
Enable enters a non-blocking `Connecting` reader state; disable immediately drops replay, while
the frontend-owned writer follows the preference during the TUI lifetime. The History page reads
only durable application CPU/memory/process-count series for 1h/24h/7d and preserves recording
downtime as visible trend gaps. Disk and network device blocks render
labeled read/write and rx/tx sparkline rows from the store's split-direction
lanes under one shared normalization (the one deliberate exception to per-row
scaling), with per-direction gap glyphs and independent warm-up gating; the
summed throughput summary below them stays on the summed lane.
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
