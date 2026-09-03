# taskmanager-gpui

## Role

GPUI desktop frontend product (`taskforest-g`, ADR-051). It owns `RootView`,
window-local interaction state, page rendering, focus, overlays, visual
projection and GPUI capture scenes; its `[[bin]]` hands this product's
capabilities (including the Windows `--capture-window` evidence mode) to the
shared CLI harness.

## Boundary

The crate consumes application/shell projections and submits typed intents. It
does not read `/proc`, `/sys`, commands, native handles or choose providers;
slow work stays in runtime lanes.
Local process clocks, Properties timestamps and replay status times use the
app-host-injected local-time observation and shared pure formatters. GPUI uses
no toolkit host-local clock API and discovers no host time-zone state.

The normal desktop host remains the default and keeps the complete application
shell. On Linux/Wayland, setting `TASKFOREST_WINDOW_HOST=layer-shell` explicitly
requests the GPUI layer-shell desktop widget through the neutral
`taskmanager-app-host::WindowPresentation` contract. That opt-in uses a fixed
`520×360` top-right surface and a compact Dashboard projection; it does not
change the standalone RootView or its `xdg_toplevel` geometry. The patched GPUI
backend owns the raw protocol role and configure/ack lifecycle; an unavailable
global or output follows the contract's explicit standalone fallback. Iced and
Bevy remain standalone until their own layer hosts exist.

The optional Linux setup capability is observed quietly during startup. Its
First Run surface is opened only from the explicit Settings entry or after a
user-initiated setup action; capability discovery never becomes a recurring
startup modal.

The product binary also supports `taskforest-g --demo`. Demo startup uses the
shared deterministic shell fixture and an in-memory bounded telemetry history;
it does not create a native platform client, read user configuration, persist
history, spawn a tray, or execute host actions.

## Key modules

- `src/gpui_app/root/projection_materialization.rs` owns the private, revision-keyed GPUI read
  model of the `SystemProjectionStore` privately held by `DirectTrackState`; `RootView` owns no
  naked store. Named platform-batch systems are the materializer's only live writers;
  render and input use immutable accessors. Process/snapshot identities and each inventory's
  rows + source statuses are replaced atomically. Containers, startup evidence, dynamic-device
  values/sources, storage health, directory/NPU results and active alerts cross the same
  boundary; GPUI never re-folds their raw batch events or keeps writable RootView mirrors.
- `src/gpui_app/root/projection_caches.rs` owns every renderer-only process/inventory/history/
  Properties memo. Its `RefCell`s are private implementation details: builders run with no live
  borrow, and render/input callers receive only immutable `Rc` snapshots. `RootView` contains one
  cache component rather than writable memo fields or borrow guards.
- `src/gpui_app/root/window_surface.rs` owns renderer-local surfaces and composes them with
  `DirectTrackState::interaction`, the application-owned authority for Process Properties and
  every dangerous confirmation. Opening either family replaces the other; shared payloads never
  enter `WindowSurface`, stale branch-mismatched dismiss/confirm paths are no-ops, and only a
  matching shared Confirm transition emits `PlatformEffect`.
- Service Details stores no dependency loading/data/failure tuple and no
  separate active-service/log target. It projects the shared dependency
  lifecycle and a request/query-keyed log lifecycle; the injected root tick
  timestamp drives filtering/export. Export completion is published through
  shell typed feedback, not a renderer string. Network escalation is the
  direct track's typed request session; the dialog only renders its immutable state.
- Process Properties insights use one exact-target lifecycle: `Idle | Loading(request) |
  Ready(request, snapshot) | Failed(attempt, error)`. Only a matching application revision and
  frozen provider identity can complete Loading; same-PID reuse, late and duplicate terminals are
  ignored. Process-control completions publish the shell's typed Control notice; renderer-local
  toasts remain only for copy/browser/config presentation and cannot duplicate that outcome.
- `src/gpui_app/root/render/surfaces.rs` exhaustively maps that authority to one active
  renderer; `render/transients.rs` composes pause, feedback, warmup and tooltip lifecycle
  layers after it. `root/responsive.rs` owns the pure `FrameBudget` → `ContentBudget` → page-slot
  projection; the root renderer only supplies shell geometry and mounts the result.
- `src/gpui_app/root/startup/capture_systems.rs` applies capture-only presentation transforms
  after the shared fold through typed named systems; it has no general mutable materialization
  accessor. Startup owns acquisition/configuration and schedules the loop.
- `src/gpui_app/root/presentation_preferences.rs` is the private authority for persisted
  appearance (including language), device visibility, units, graph options, sidebar preferences,
  the window-frame (decoration) policy token and Apps display policy. Settings mutate named axes;
  renderers receive an immutable snapshot,
  and per-axis fingerprints invalidate only the affected projection. Page/focus/sidebar editing,
  drag/scroll state and runtime handles remain separate window-local facts. The process-global
  language catalog is a derived renderer activation, never preference storage.
- `src/gpui_app/root/startup/config_sync.rs` folds immutable config publications and submits
  base-aware drafts without file I/O. Each 200ms owner cycle drains external revisions first,
  then submits a changed presentation fingerprint immediately instead of waiting for the 25-cycle
  fallback fold. Runtime revisions atomically replace persisted fields only; page/focus/alerts/
  history/runtime handles survive. A changed history preference submits only to the bounded
  connector; the UI thread never waits for writer startup or storage.
- `src/gpui_app/root/history_runtime.rs` owns the exhaustive
  `Disabled | Connecting | Unavailable | Active` reader state. Enable submits one correlated
  app-host connector request; disable immediately drops replay and returns Performance to live.
  Only the latest request completion may install a client, so late enable cannot undo disable.
- Snapshot and diagnostic export use app-host-injected named clients. GPUI freezes
  the current projection into an immutable request, then only drains correlated
  completion into shell feedback; System and Service Details own no file writer,
  current-directory lookup or per-window worker.
- Linux GPUI exposes a current-window PNG action in the navigation controls. It
  submits the shared application request and reports the app-host receipt; it does
  not read Wayland state or invoke Spectacle from the renderer. The native adapter
  seam reserves Portal Screenshot and ScreenCast/PipeWire for later evolution.
- GPUI receives the app-host's narrow enable connector plus the paired read-only replay client
  and in-process writer capability. The root tick drains correlated connection/query
  completions; filesystem writes and teardown remain owned by app-host's bounded worker.
- The top History page consumes only the application-owned durable replay projection. It shows
  CPU, memory and process-count peaks plus timestamp-gap-aware trends for 1h/24h/7d; no live
  process list or frontend-local ring can become a second history authority.
- `src/gpui_app/perf_views/history_replay.rs` projects the application replay
  lifecycle and converts each accepted immutable row set into stable GPUI `Rc`
  graph handles once per request. It owns no storage primitive, path or file
  I/O; the root tick drains the app-host client, and stale completions are
  rejected by the shared request reducer.
- `src/gpui_app/root/keyboard.rs` normalizes a key once, routes it through `GpuiSurfaceKind`
  (`Shared | Local`), then delegates confirmation or application action. Surface policy is never
  inferred from combinations of visibility flags.
- The Run dialog's per-window `TextInputState` is the sole command-text authority. Submit reads
  it through a read-only accessor and accepted completion clears the entity; `RootView` keeps no mirrored
  command string. Command launch, resource reveal and URL open all use the direct track's single
  typed shell-action session; GPUI owns no request-id correlation map.
- `src/gpui_app/root/dialog_scroll_state.rs` owns independent per-window handles for long-form
  modal bodies; those dialogs compose through the shared bounded viewport + pinned rail.
- `src/gpui_app/*_view/` owns page-specific visual projection and interaction.
  Service details reads dependency targets through `ServiceDeps`' immutable
  typed projection; inventory rows receive `ServiceItem`'s read-only typed
  relation surface and never cache writable compatibility strings.
- Dashboard alert rows render `DirectTrackState`'s immutable canonical
  `ManagedAlertRule` list. Toggle, add, bounded adjustment, remove and clipboard
  merge submit application semantic edits; `DashboardState` owns no rule or
  enabled-state mirror.
- `tests/common/test_support.rs` provides crate test support; the repository-level GUI behavior
  suite is `../../tests/gui/`.

Root navigation is a bounded layout region: horizontal tabs flex within their strip and scroll when
the locale/page set exceeds the slot; vertical tabs live in a fixed, scrollable rail and the body
receives the remaining width.
The Applications page exposes one category-first hierarchy (Applications / Background /
Uncategorized). The Applications bucket then inserts a PID-less application-root total before
each real parent/child process tree; category/application totals own the sums, while every
process row (including a process with children) keeps its own PID and own sample. Old grouping
tokens are migration inputs, not new UI choices.

Application totals are selectable through `ProcessRowId::Application(root_identity)` without a
representative PID. Keyboard/pointer selection share the rendered semantic row order; batch
verbs freeze the live subtree, while single-process details/affinity stay unavailable. Desktop
UI size is Small/Standard/Large: `RootView` sets the GPUI rem to 14/16/18px so every owned
`FONT_*` token responds across the app, while process-table icon/control metrics consume the
same `UiSize` directly. Row density remains independent.
Every page receives one frame-local `FrameBudget`/`ContentBudget` projection:
shell chrome is deducted once at the root, horizontal `LayoutProfile` and vertical capacity remain
independent, and page modules
derive typed chrome/chart/timeline presentations rather than width/height
booleans. Every Performance device page (CPU/Memory/Disk/Network/GPU/Battery/Fan)
composes through the ONE `perf_views::layout` root (`perf_page`, ADR-039):
pages declare `ChartSpec` charts whose `ChartTier` (headline/secondary/compact)
derives the height floor, first-frame state overlay, hover surface, legend,
aesthetic injection, and summary row in one place; mini density cells render
through the shared `mini_graph_cell`. The main column is one fixed
`overflow_hidden` viewport — never a scrolling body — and the statistics rail
width. Only the left device selector may scroll; the Performance main viewport
and statistics rail are static, and lower content is capped, summarized, or
omitted before it can reach the viewport edge. The CPU page adds its readout
band as the header slot and, when the chart-inventory budget permits, the
per-core matrix below; it owns no metric/detail selector state. The GPU page
likewise owns no metric/detail selector: the large aggregate utilization graph
is always the headline, full-inventory budgets add one fine mini-card per
reported engine, and a compact GPU-memory utilization graph forms the bottom
group. All preserve immutable stats/VRAM facts; a family the platform cannot
measure renders nothing at all — never a fabricated zero and never a selector.
Disk and network main graphs draw two series from the store's split-direction
lanes (read/write, rx/tx; family color and its 0.32 tint) under one shared
peak and one cached static grid — the dynamic scene keys carry an explicit
series slot so the two lanes never share a cache entry, and hover reads each
direction independently without faking a gap lane.
Disk directory-usage renders the canonical
projection and submits only typed start/cancel requests; no renderer-side
snapshot reducer remains.
The privileged GPU engine panel also reads the direct track's request lifecycle and the accepted
shell projection. Its sole window-local state is device binding plus poll cadence; it keeps no
request, loading/error or engine-row mirror. Affinity, process-batch and SMART-self-test request
state is read from the shell direct track's
application-owned typed sessions. GPUI keeps only editor draft/presentation state; it has no
writable request id, loading flag or error mirror. PID reuse and storage-generation changes close
or fail closed, and late/duplicate terminals never create feedback or audit entries.
GPU stats, sidebar captions, capture fixtures and history all use canonical
current scalar/throttle accessors; legacy snapshot keys never become a
GPUI-side data mirror. The GPU headline chart-metric window is no exception:
`render_gpu` reads it through the one shell dispatch
(`gpu_chart_metric_history`) over the direct track's `LiveGraphHistory` view
of its own telemetry store — this frontend keeps no second sampling fold.
System Health borrows the exact generation-bound SMART report directly from the shell projection
for the visible disk. Capture evidence may carry one complete typed observation, but production
owns no fragmented report/device/name option mirror. The affinity CPU set and hover chip form a
dedicated window-local editor component; applying it still submits the shell-owned typed session.

The Apps process table restores a continuous horizontal scrollbar when its enabled column extent
exceeds the actual x viewport (the page width minus the pinned vertical rail). The header and
virtualized body share one outer `ScrollHandle`, so dragging translates one already-laid-out
content surface instead of rebuilding the row projection for each pointer position; wide windows
do not reserve an empty rail. Both that x owner and the vertical `uniform_list` explicitly
restrict wheel input to their own axis. The Name column remains the leading identity column.
Bare Left/Right on the table run the iced-parity tree keyboard matrix: a subtree row
collapses (Left), expands (Right), and a Left on an already-collapsed row climbs the
selection to its nearest visible selectable ancestor, while leaf rows and
Alt/Shift+Left/Right keep the column-cursor/sort navigation path.
Apps chrome consumes one `ProcessChromePresentation` mapped from the frame's typed
`PageLayoutBudget`; it never re-tests viewport pixels or carries compact booleans. Wide surfaces
place title/search in one overview band and primary actions/hierarchy/status filters in one bounded
control band. Standard and compact surfaces use explicit stacked bands; secondary commands always
remain available through the anchored actions menu. Vertical capacity independently reduces
overview detail and inline commands, so the virtualized process table remains the primary height
owner even in a wide, short window.

Startup fixed chrome contains only actions, filters and the bounded evidence summary. The table is
the min-height primary content; the boot timeline is a sibling detail region selected by
`StartupPageBudget` as `Collapsed`, bounded `Expanded`, or bounded `SidePanel`. Source failures use
the same typed notice with a compact presentation under height pressure, so retry/failure truth is
preserved without consuming the table. Constrained height is the only automatic-collapse state;
a narrow but tall window uses a bounded stacked timeline instead of wasting available height.
Timeline overflow is surfaced as an exact omitted count.

The shared scrollbar uses the handle's viewport/max-offset geometry, keyed drag state, capture-phase
pointer handling, hover expansion, and paint-safe coalesced invalidation. The pinned rail's hairline
is inset once; the thumb geometry uses the full rail axis so its paint cap reaches the true end.
The refresh gate is an application-level mitigation: GPUI 0.2.2 still rebuilds a dirty root view
and does not provide retained rows for arbitrary tables. The real event/cost tests live in the
GPUI process scroll behavior module, the process-tree projection tests, the Performance bottom-
offset test, and the UI scrollbar/rail tests; performance, keyboard/pointer, and Niri gates remain
required for changes to this host.

The Performance device sidebar has one persisted width preference, but the frame budget is the
current geometry authority: its effective width is clamped to the available page slots before the
outer slot pins `min/width/max`. Provider-owned device text therefore truncates inside the rail and
can never expand shell chrome through flex min-content sizing. When the sidebar cannot coexist with
the main viewport and statistics rail, the same devices move to the strip. The pinned scrollbar ends
before a dedicated resize gutter, so its wheel-preserving hit layer cannot cover the drag target.
Rows are collected as immutable props, ordered once, and only then rendered with the exact visual
order in an `Rc<[String]>`; drag payloads never recover order through a render-time `RefCell`.

Long-form dialogs use the owned fixed-header + pinned-rail column when they have persistent actions:
actions stay outside the tracked coordinate tree, while section labels and cards belong to the
scrolling body. Split Performance surfaces meet at an explicit themed divider rather than exposing
the window background through a transparent parent gap.

When a native tray is available, closing the main window minimizes it and keeps
the root/ECS/runtime/single-instance guard alive; the tray Quit action is the
process-termination path. A second launch activates the existing window. The
capture-only harness and tray-unavailable fallback may close normally so they
cannot leave an owned process behind.

## Contract and verification

Keep render-entry folds shared by pointer and keyboard paths. Changes require
headless behavior tests and, when visible, current GPUI capture/validator/review
evidence. Component implementation belongs to `taskmanager-ui`.

## Module map

```text
src/assets.rs  src/capture.rs          assets and evidence capture
src/gpui_app/                          RootView composition root
├── chrome.rs  containers_view.rs      window skeleton and containers
├── dashboard/  cpu_view/  graph/      pages and cards (ADR-038/039 budgets)
├── functional.rs                      CORE-04 GPUI surface declarations
├── first_run.rs  about.rs  app_history_view.rs
└── elements.rs  formatting.rs  capabilities.rs
```

Consumes shell projections only; every frame drains a bounded projection cache.
