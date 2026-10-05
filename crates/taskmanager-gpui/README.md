# taskmanager-gpui

## Role

GPUI desktop frontend product (`taskforest-g`, ADR-051). It owns `RootView`,
window-local presentation, page rendering, focus, overlays and GPUI capture scenes.
Its binary hands product capabilities, including Windows `--capture-window`, to
`taskmanager-cli`.

## Boundary

The frontend consumes immutable application/shell projections and submits typed
intents. Providers and native I/O remain in platform runtime and app-host; no
renderer or keyboard handler reads process files, native handles or host clocks.
Local dates use the app-host time-zone observation and shared pure formatters.

The desktop host preserves the complete standalone shell. On Linux/Wayland,
`TASKFOREST_WINDOW_HOST=layer-shell` requests the app-host presentation contract's
520×360 Dashboard widget. Patched GPUI owns the protocol role and configure/ack
lifecycle; unavailable protocols follow the explicit standalone fallback. Iced
and Bevy retain their independent host adapters.

`taskforest-g --demo` uses shared deterministic fixtures and bounded in-memory
history. It does not load user configuration, persist history, spawn a tray or
execute host actions. First Run opens through Settings or a user setup action;
capability discovery alone never creates a recurring modal.

## State and ownership

- `root/projection_materialization.rs` owns the revision-keyed read model of the
  shell's privately held system store. Named platform-batch systems replace each
  inventory's rows and source status together; renderers never fold raw events.
- `root/projection_caches.rs` owns immutable process, inventory, history and
  Properties memos. Builders run outside `RefCell` borrows, and consumers receive
  owned `Rc` snapshots instead of mutable guards.
- `root/window_surface.rs` composes local surfaces with application-owned
  `DirectTrackState::interaction`. Properties and dangerous confirmations replace
  each other; only a matching frozen Confirm transition emits an effect.
- `root/process_insights_ui.rs` correlates one frozen target and application
  revision. The accepted `ProjectedProcessInsights` retains seven independent
  Pending/Current/Unavailable facets. Partial replies, late enrichment and missing
  reasons stay distinct. Refresh preserves the original frozen target and refuses
  duplicate work while any facet collects. PID reuse cannot publish into it.
- `process_insights/view/facets.rs` renders the selected shared facet. Long lists
  keep complete data and true totals; normal page controls reach entries beyond
  the bounded row materialization window. No whole-snapshot error masks another
  facet's available data or distinct missing reason.
- `root/render/surfaces.rs` maps the application surface authority to one active
  renderer. `render/transients.rs` then composes feedback, pause and tooltips.
- `root/responsive.rs` derives one frame/content budget. Pages receive explicit
  slots after shell chrome is deducted; width and vertical capacity stay separate.
- `root/presentation_preferences.rs` owns persisted appearance, units, device and
  sidebar policy as one immutable snapshot. Per-axis fingerprints invalidate only
  affected projections; page, focus and runtime handles remain local state.
- `root/startup/config_sync.rs` drains external configuration before submitting
  base-aware drafts without file I/O. Revisions replace persisted fields together
  and never reset navigation, runtime handles or active history requests.
- `root/history_runtime.rs` owns Disabled/Connecting/Unavailable/Active reader
  phases. Enable submits a correlated app-host request; disable releases replay.
  Stale completions cannot undo disable or replace a newer connection.
- `perf_views/history_replay.rs` adapts the application replay lifecycle and each
  accepted row allocation into stable GPUI graph handles. Filesystem work and
  writer teardown belong to app-host's bounded worker.
- `root/keyboard.rs` normalizes each key once and routes through the shared/local
  surface policy. Pointer and keyboard paths consume the same cached projection.
- `root/dialog_scroll_state.rs` owns separate long-form body handles. Persistent
  actions stay outside the scrolling coordinate tree.

Service Details reads shared dependency and query-keyed log lifecycles. Export
completion enters shell feedback; it owns no worker, file writer or mirrored
service target. Process control, affinity, resource limits, SMART jobs and GPU
engine requests likewise use application-owned sessions with frozen identity and
storage generation checks. Late and duplicate completions cannot create feedback.

The Run dialog's `TextInputState` is the command-text authority. Launch, reveal and
URL open use the direct track's typed shell-action session. Snapshot, diagnostic
and PNG requests use named app-host clients. The PNG host tries Blade readback
before its selected native adapter; unsupported backends remain typed failures.

## Presentation contracts

Navigation uses bounded horizontal tabs or a fixed scrollable vertical rail.
Apps exposes category/application totals and real parent/child rows. Totals have
no representative PID: batch verbs freeze their live subtree; single-process
Properties and affinity remain unavailable on aggregate rows. Selection follows
the rendered semantic order for both pointer and keyboard.

Small/Standard/Large sets GPUI rem to 14/16/18px and shared control metrics; row
density remains independent. The process header and virtualized body share one
horizontal scroll owner. Name stays the leading identity column. Tree Left/Right
collapses, expands or selects an ancestor; leaf/modifier navigation retains column
cursor and sort actions. Typed chrome budgets keep the table's minimum height,
with secondary commands reachable through the actions menu.

Every live Performance device page composes through `perf_views::layout`.
`ChartSpec` tiers own chart floors, state overlays, hover, legends and summary
rows. Live charts and statistics stay in a fixed main viewport; the device rail
alone scrolls at page level. CPU has a bounded nested details viewport. GPU keeps
its headline, optional engine cards and memory group without inventing unsupported
families. Disk/network directions share a peak and static grid but have separate
scene keys and independent hover facts.

Device-sidebar width is clamped to current page slots before pinning its edges.
If the rail cannot coexist with the main viewport, the same devices use the strip.
The pinned scrollbar leaves a dedicated resize gutter. Drag order comes from the
same immutable row props as paint, with no render-time writable order mirror.

Dashboard windows and readouts use application-correlated system history. Controls
and paging stay fixed, all four complete metric cards remain reachable, and paint
and hover use the selected timeline's actual count. Durable History shows the
shared 1h/24h/7d replay projections; live process rings never become its authority.
Recorded curves keep gaps and signed ranges with separate per-window scrolling.

Startup fixed chrome keeps actions and bounded source notices outside its primary
table. `StartupPageBudget` admits the timeline as collapsed, bounded stacked or a
side panel. Height pressure preserves failures, retry actions and exact omitted
counts instead of hiding the table or silently dropping timeline facts.

Properties preserves frozen name/PID while looking up live values by exact key.
Overview exposes available PSS/USS/Swap; Performance carries current and peak
readouts with honest recent samples. Long values and all insight rows remain
reachable in one bounded body. Closing or losing the frozen process cannot
retarget the window through the table selection.

SMBIOS uses shared inventory rows, configured units and wrapped selectable values.
Permission entry stays in Settings. The shared scrollbar uses viewport/max-offset
geometry, keyed drag state and paint-safe invalidation with one edge inset.

When a tray exists, window close minimizes while root/runtime/singleton remain
alive; tray Quit terminates. Secondary launch activates the existing window.
Capture-only and tray-unavailable sessions may close without leaving a process.

## Contract and verification

Keep render-entry folds shared by keyboard and pointer. Verify behavior and side
effects through nextest; visible changes additionally require measured headless
bounds with product fonts and current native capture/validator/manual review.
Process insight captures use shared positive facts and require a rendered-frame
acknowledgement; opening a surface or having a file does not certify its contents.
Component implementations belong to `taskmanager-ui`.

## Module map

```text
src/assets.rs  capture.rs  run.rs      assets, evidence capture, binary entry
src/window_presentation.rs             layer-shell / standalone host adapter
src/gpui_app.rs                        RootView composition root
├── root/                              root view layout and lifecycle
│   ├── render.rs  render/             page rendering and shell composition
│   └── render/pages/                  page bodies (inventory, performance, apps, system)
├── chrome.rs  containers_view.rs      window skeleton and containers
├── sidebar/                           navigation sidebar
├── dashboard/                         dashboard page and cards
├── perf_views/                        performance page composition (ADR-039)
├── cpu_view/                          per-core CPU detail views
├── processes_view/                    process table page
├── process_insights/                  per-process detail overlay
├── services_view/                     services page
├── startup_view/                      startup items page
├── users_view/                        user sessions page
├── system_view/                       system information page
├── system_health_view/                system health page
├── settings_view/                     settings page
├── graph/                             shared graph/chart components
├── elements/                          shared UI elements
├── list_view.rs                       virtual list component
├── history_samples.rs  timeline.rs    history reads and application-curve allocation adapter
├── help_overlay.rs                    keyboard help overlay
├── system_about.rs  about.rs          system/app about pages
├── app_history_view.rs  first_run.rs  application history and first-run
├── functional.rs  capabilities.rs     CORE-04 declarations and capabilities
├── icons.rs  theme.rs  formatting.rs  icon bindings, theme, formatting
```

Consumes shell projections only; every frame drains a bounded projection cache.
