# Surfaces and state

Confirmations and process-properties visibility remain owned by the shared
application `InteractionState`; the TUI's optional process-properties view
model is a render cache only and cannot make that surface visible.

Properties freeze identity and resolve current canonical facts by live key.
Their Performance tab shows current values, peaks and four bounded history
trends through the terminal component's Unicode or ASCII repertoire; missing
history remains collecting and recording gaps remain visible. Their Insights
tab uses 1–7 to select complete facets and `r` to refresh that same target;
denied or partial network traffic exposes typed authorization. Only the
bounded body scrolls, and PID reuse never redirects an open review. Opening a
new surface replaces the prior owner, and stale typed dismiss events are
no-ops.

Search, Help and Suggestions are branches of the shared shell's one
`ShellInputMode`, so the terminal cannot carry contradictory keyboard owners.
Bare-key and command-palette exits submit distinct typed quit reasons. The
footer lives in `src/ui/footer.rs` and reads the shell's single feedback
projection; TUI settings, clipboard, persistence and control paths publish
typed notices instead of mutating a shared status string.

## Health surface

The Health surface reads the shell's canonical managed alert rules directly;
disabled rules stay listed and labelled. TUI supports selection and toggle
only; rule authoring and import/export are unsupported. Threshold suggestions
remain a separate read-only evidence projection and cannot become rule
authority. Enter/Space toggles the selected rule. `p` opens active alerts and
`e` opens complete event transitions; event review keeps F2 filters, F5 export,
and F8 clear. Health `w` selects filesystem and SMART reports; `s` selects
sensors and `q` returns to managed rules. Arrows visit complete groups,
including denied readings and the last observed report. SMART self-test is
available only from Performance·Disk with `t` and its command-palette twin;
Health review has no SMART test shortcut.

## Configuration and settings

Configuration I/O is owned by the app-host background coordinator. The event
loop only drains immutable publications and submits bounded patches. The
uncomposed `TuiApp::new`/`from_shell`/demo constructors perform no host
discovery; only `runtime::run_live` creates `NativeAppHost` and injects its
`ConfigClient` and local-time observation. The frontend stores neither a
configuration path nor a fallback coordinator.

The Settings form has a typed Clean/Dirty/Conflict lifecycle: an external
revision updates runtime preferences but never discards a dirty form or
permits its stale unedited fields to overwrite the new canonical snapshot;
Cancel reloads the latest snapshot before editing may resume.

Settings F3 opens concrete device editing. Arrows select and reorder stable
keys; Space toggles visibility through the shared core rule and coordinator.
Enter opens that identity; removal displays disconnection rather than another
device. Title and actions stay fixed around whole rows, and the same choices
order/filter the selector and detail projection. Its continuous-history
control uses the same canonical config field as both desktop frontends.

Enable enters a non-blocking `Connecting` reader state; disable immediately
drops replay, while the frontend-owned writer follows the preference during
the TUI lifetime. The History page reads only durable application
CPU/memory/process-count series for 1h/24h/7d and preserves recording downtime
as visible trend gaps. Disk and network device blocks render labeled
read/write and rx/tx sparkline rows from the store's split-direction lanes
under one shared normalization (the one deliberate exception to per-row
scaling), with per-direction gap glyphs and independent warm-up gating; the
summed throughput summary below them stays on the summed lane.

## Exports and capture

Diagnostic export first opens a sanitized review. Enter confirms the displayed
plan, Esc invalidates its session, and late completions cannot reopen it. Only
the body scrolls; wrapped action hints and a bottom safety row remain reserved
at compact sizes. Snapshot export follows the same ownership rule: the key
path submits one typed request to the app-host worker, and the event loop
drains its correlated completion into shell feedback. The terminal thread
never serializes or writes the three artifacts.

Service inventory consumes `ServiceItem` descriptors without materializing
legacy relation strings; the canonical typed relation graph remains read-only
at the terminal boundary. GPU captions, details and graph samples likewise
consume only current typed scalar/throttle accessors, so unavailable provider
facts remain gaps.

Real terminal evidence has two entry points:

- `bash scripts/capture-tui-matrix.sh` runs the committed
  `scripts/capture_tui_scenarios.tsv` matrix (one supervised receipt per row;
  the product binary is built once). Use
  `--scenarios process-selection,apps-identity-matrix` for a bounded subset and
  `--list` / `--dry-run` to inspect the resolved page/device/scene/skin without
  capturing.
- `TM_TUI_CAPTURE_DEVICE=gpu bash scripts/capture-tui.sh` and
  `TM_TUI_CAPTURE_SCENE=system-npu bash scripts/capture-tui.sh` remain the
  single-frame entry for a targeted manual run.

The matrix row name is a receipt label; the scene token is derived from the
`CAPTURE_SCENE` case in `scripts/capture-tui.sh`. A scene the UI parity audit
declares `Unsupported` (for example `sidebar-hidden`) has no matrix row.
