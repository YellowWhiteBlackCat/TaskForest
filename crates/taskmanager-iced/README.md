# taskmanager-iced

## Role

Iced desktop frontend with responsive geometry, Canvas charts, semantic focus,
keyboard routing and renderer-local widgets.

## Boundary

It consumes `taskmanager-shell` and toolkit-neutral contracts. It does not
access OS sources, choose providers, import GPUI entities or share large-table
widget state with another frontend.
Process rows and Properties consume the app-host-injected local-time rules.
Iced owns no `TZ`, zoneinfo filesystem or implicit UTC fallback path.

## Key modules

- `src/app/surface.rs` owns the single Iced-local primary surface, the single
  context-menu slot, branch-matched close transitions and the derived
  `InputScope`. Shared confirmations and Process Properties remain in the
  application `InteractionState`; payload models such as Run Task and Alert
  Center do not own parallel visibility booleans.
- Focus and motion consume `PresenceTransition::{StableClosed, Opened,
  StableOpen, Closed}` derived from before/after scopes, never a pair of modal
  visibility booleans.
- `src/app/` owns navigation, focus, settings and projection state;
  `src/app/update/` has one exhaustive message→domain authority. The main
  update entry only captures its prelude, dispatches once, then runs the common
  finish systems; input, navigation, service, control, surface, performance,
  transfer, Alerts and window reducers return one typed effect/task envelope
  and cannot bypass lifecycle convergence.
- The frontend-only Alerts page is `FrontendRoute::AlertsPage`, mutually
  exclusive with `SharedPage`; it is not an `open: bool` beside the shell page.
  Selecting a shared page or explicit close returns to `SharedPage`, bare Esc
  closes Alerts only when no higher-priority surface owns input, and a modal
  closes first while preserving the route underneath it.
- The Alerts page reads the shell's immutable canonical `ManagedAlertRule`
  list and submits the shared toggle edit. `AlertsPageState` contains only its
  typed route; disabled rules are neither copied nor removed from the list.
- `src/app/config_sync.rs` is the only configuration bridge: production receives
  an app-host `ConfigClient`, ticks drain immutable revision publications, and
  settings use non-blocking base-aware submissions. Queue rejection/save failure
  rolls renderer preferences back to the canonical snapshot without changing
  page, focus, scroll or runtime state; demo/default instances remain local-only.
- `src/app/configuration_state.rs` privately owns the coordinator cursor, applied revision,
  canonical draft, immutable presentation preferences, language and resolved Theme. One snapshot
  application replaces all five together; there are no independent draft/preference/theme writers.
- The named input, process-presentation, performance, capture and window-time components under
  `src/app/` split renderer-only ownership out of `IcedApp`. Window time is tick-injected and per-window;
  these components contain no shared request payload or provider fact.
- `src/app/viewport_state.rs` owns the window size and the six independent Applications,
  App-history, Services, Startup, Users and Performance-rail scroll lifetimes. A validated resize
  invalidates every observed viewport atomically; views receive only offsets, bounds and widget ids.
- `src/app/projection_caches.rs` is the private owner of the eight renderer-only
  memos. Process performance keys by pid/ring revision; graph series by history
  revision/capacity/device identity; durable app-history rows by accepted replay request;
  Services, Startup and Users by independent
  inventory revisions; the Performance rail by system/history revisions,
  visible window, device selector order and unit choices. Views receive immutable
  `Rc` snapshots rather than `RefCell` guards. Scroll and viewport state remain
  outside because their interaction lifetime is independent.
- Canonical system facts live only in `ShellApp`'s private `SystemProjectionStore`; Iced reads
  `projection()` and submits batches or named semantic reducers. Demo/capture/tests use the
  shell's typed fixture facts instead of assigning projection fields.
- `src/app/history_replay.rs` adapts the application request lifecycle and the exhaustive
  `Disabled | Connecting | Unavailable | Active` reader state. Runtime enable is a non-blocking,
  latest-request-wins connector transition; disable immediately closes replay. The top History
  page consumes the shared durable 1h/24h/7d application projection, while Performance filters
  application series from its device replay. The tick drains clients and never reads files.
- `src/tray.rs` owns the Iced-local tray spec/action mapping; `src/app/runtime.rs`
  owns the non-blocking single-instance activation pump. Production uses the
  `TaskForestI` identity, keeps one primary window/process, and minimizes to the
  tray when the native tray is available; typed tray failure degrades to a
  window-only process. Demo/capture bypasses native lifetime resources.
- Keyboard, native-window and tray exits submit distinct shell `QuitReason`
  values; Iced only reads `should_quit()`. Footer activity, settings failures,
  export and clipboard results all use the shell's typed feedback authority;
  Iced keeps no parallel footer feedback strings.
- Service Details projects the same application dependency/log lifecycles as
  GPUI. Queue admission begins before runtime submission, filter/cursor changes
  create a new query generation, and only its returned request can fold. Log
  timestamps come from the tick/event cache; export status uses shell feedback.
- Snapshot and service-log export use app-host-injected named clients. Message
  handlers only submit immutable typed requests; the tick drains correlated
  completions into shell feedback, with no renderer filesystem/thread owner.
- `src/ui.rs` owns root dispatch/chrome; `src/ui/performance.rs` owns the Performance
  selector/detail composition; `src/ui/perf_rail/captions.rs` owns pure rail text projection;
  the remaining `src/ui/` modules own page projections and charts.
- Service-details dependency chips iterate the immutable typed `ServiceDeps`
  projection, while inventory rows keep `ServiceItem` relations read-only;
  legacy JSON strings never enter Iced state as a parallel authority.
- GPU rail/detail projections fix aggregate utilization as the primary history and keep canonical
  scalar/VRAM facts simultaneous. Standard geometry adds every available engine graph; compact
  geometry is an elastic, non-scrolling aggregate surface. Engine collection remains a typed
  capability action rather than display state. System projects each successful NPU inventory as
  identity, driver, aggregate/engine utilization and dedicated/shared memory cards; unavailable
  observations remain explicit and no trend is invented without history authority.
- `src/ui/components.rs` owns shared Iced surfaces, the root page scaffold, key/value
  rows and the state-panel grammar; `src/ui/components/primitives.rs` adds the token-styled
  badge/divider/progress/tooltip primitives (colors only from the palette snapshot,
  unavailable progress never renders as a measured zero); `src/ui/components/inputs.rs`
  adds switch/slider/select/search/segmented controls that route keyboard access through
  the focus shell; `src/focus.rs` owns keyboard-reachable activation shells and draws the
  shared `palette().ring` focus ring gated by the renderer-local input modality
  (`src/input_modality.rs`: only keyboard focus paints the ring, the same strict policy
  the GPUI root tracker applies); `src/ui/virtual_list.rs` owns bounded table-window
  geometry, the sticky-header composition shell and the typed column vocabulary — the
  Applications table derives widths/alignment/hideability from the shared
  `PROCESS_COLUMNS` contract, and Services/Users/Startup keep local typed specs.
- `src/ui/responsive.rs` is the frame-local layout budget chain (`LayoutProfile` ×
  `VerticalSpace` → `PageLayoutBudget` → page presentation enums, GPUI semantics as pure
  data). Chrome and toolbar breakpoints are typed facts on it, not scattered literals.
- `src/app/appearance.rs` resolves the System color mode from the OS
  (`iced::system::theme()` boot query + `theme_changes()` subscription); an explicit
  Light/Dark/EyeForest choice is never overridden. The motion preference is a
  persisted config token (`normal`/`reduced`/`none`) that seeds the process
  motion policy: reduced clamps transitions to 80ms, none drops modals straight
  to their final state without the frame pump.
- Type scale is single-track: `tokens::FONT_*` px constants on the Small baseline,
  scaled by the application `scale_factor` (`renderer_scale`). Role methods that bake
  the UiSize into the value double-scale and are not used here.
- The Settings surface is grouped (General/Appearance/Fonts/System/Notifications/Units)
  and renders real controls — segmented, slider, select, switch — over the same
  `SettingsChange` channel and persisted tokens; the shortcut legend derives from the
  Iced binding declaration. Text rendering shows an honest unavailable state (no dead
  selector).
- Iced-local surfaces: the first-run dialog (`ui/first_run.rs`, platform setup-script
  observe/action lane, dismiss is side-effect free), the Containers full page
  (`ui/containers.rs`, six honest branch states via `page_branch`), and the System-page
  dashboard segment (`ui/system_dashboard.rs`, summary card + `HistoryWindow` pills +
  alert mirror — no history is invented without a shell projection).
- Device charts share one window→geometry mapping (`WindowSlots`, right-anchored slots;
  hover crosshairs snap through the same mapping) with DATA/OVERLAY dual caches; the
  dual-series device chart (`ui/device_chart/multi.rs`) separates read/write and rx/tx
  with same-hue tinted variants, fed by the store's split live rings (session-local
  chart height until the multi factory grows a Fill variant).
- Applications column widths are keyboard-reachable, persisted overrides over the
  contract defaults: a 6px header drag hotspot or the column menu's per-column
  stepper publishes `ResizeProcessColumn` (fill/identity columns are never
  resizable), widths clamp to 40..=600px, and the header, body and scroll extent
  resolve from the same override-or-default seam so sticky alignment never drifts.
  Widths persist through the shared `process_col_widths` config tokens (the same
  vocabulary GPUI writes); startup rehydrates through the same clamp domain and
  drops unknown tokens.
- `../../scripts/capture-iced.sh` and its validator own pixel evidence, not product state.

The Applications projection presents the same category-first tree contract as GPUI and TUI:
Applications, Background and Uncategorized are first-level buckets. Applications inserts a
selectable PID-less application aggregate before each recursive process tree; the other buckets
expand directly to real process rows. Legacy grouping tokens are normalized when configuration
loads. Small/Standard/Large UI size uses the application scale hook on top of compositor DPI;
density remains a separate table-whitespace preference.

## Capability parity and page-level divergence

Component/surface capability parity is MACHINE-CHECKED (CORE-08):
`src/capabilities.rs` declares every ui-contract registry capability with an
explicit decision — `Ported`, `Native`, or a reasoned `Divergent`/
`Unsupported`. The one registered divergence: Toast (transient feedback
renders through the shared footer activity line). TextSelection left the
divergence list when `src/ui/components/selectable_text.rs` ported the
reference semantics — drag/word/all selection, primary-clipboard on drag
finish, Ctrl/Cmd-C copy — and the network page's per-field copy buttons were
retired with it. A silent omission or unexplained difference fails the
crate gate.

Below is the complementary PAGE-LEVEL registry: Performance-page skin
divergences hanging on a shared semantic with a real Iced-vs-GPUI
architecture driver (ARCH.md §8.2). They are presentation executions of
shared semantics, not component-capability differences, so they stay prose
here; anything duplicating GPUI semantics without such a driver was removed
or aligned instead.

- CPU-page trend strip: the device rail's sparklines are unreachable in Strip frames
  (narrow width or F9); the strip keeps the five shared family trends visible.
- Inline 60/120/300s graph-points pills: the same shared persisted preference GPUI's
  Settings page owns; Iced's full-screen settings modal makes a local access surface the
  density-appropriate execution.
- Gauges row (CPU/Memory): a skin/density execution of the same used-percentage
  observations; the Memory page has no GPUI readout-band equivalent to carry them.
- Wi-Fi status card (network): skin-level presentation (status dot + signal-quality bar)
  of the same wireless facts the stats rail carries.
- Earlier/now chart-axis labels: a skin-level annotation of the shared chronological
  contract (oldest-left, newest-right).
- CPU+Memory dual-series chart: density execution over the same shared history store;
  the Memory page still carries its own swap headline chart so the shared swap-over-time
  semantic stays covered.
- VRAM meters in the left column and the engine-escalation toggle below the card:
  placement differences for facts GPUI pins in its stats-rail footer; the facts, gating,
  and honesty rules are identical.
- The GPU metric selector stays unaligned by design (ICED-014, 2026-08-29): every
  measured family renders simultaneously, so the selection surface and its message were
  deleted outright; the shared shell selection state machine remains reachable through
  behavior tests.

## Contract and verification

Cross-crate owners and transition matrices are authoritative in
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); Iced only implements
the renderer rows and returns owned cache snapshots rather than borrow guards.

Preserve compact/wide geometry, current/partial/loading/error projection and
Iced-native focus/scroll semantics. Visible changes require the current Iced
matrix and validator; headless changes require behavior/geometry tests.
