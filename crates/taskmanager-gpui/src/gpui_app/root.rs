//! Root gpui view: holds live telemetry + process data, renders the Mission Center shell
//! (top tab bar + Devices sidebar + active page), and polls the collector on a timer.

use gpui::{
    AnyWindowHandle, Context, Entity, Pixels, ScrollHandle, Subscription, UniformListScrollHandle,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use taskmanager_ui::data::table::TableState;
use taskmanager_ui::inputs::slider::SliderState;
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::primitives::button::ButtonState;

use crate::core::process::ProcessBatchHistory;
use crate::core::startup::StartupEntryId;

use crate::core::StableDeviceSelection;
use crate::gpui_app::containers_view;
use crate::gpui_app::cpu_view::{self, CpuHistoryCache};
use crate::gpui_app::dashboard::{self, DashboardState};
use crate::gpui_app::elements;
use crate::gpui_app::first_run;
use crate::gpui_app::perf_views::{self, MemoryHistoryCache};
use crate::gpui_app::processes_view;
use crate::gpui_app::services_view;
use crate::gpui_app::settings_view;
use crate::gpui_app::sidebar::{self, SelectedDevice};
use crate::gpui_app::startup_view;
use crate::gpui_app::system_health_view::{self, SystemHealthCallbacks};
use crate::gpui_app::system_view;
use crate::gpui_app::theme::{FontAvailability, Theme, WindowCorner, detect_font_availability};
use crate::gpui_app::users_view;
use crate::i18n;
use taskmanager_application::{
    ConfigClient, DesktopAppearance, OperationFailure, PlatformClient, RefreshRequest, RequestId,
    ServiceId, SetupScriptAction, SourceStatus, TelemetryRefreshPolicy,
};
use taskmanager_shell::{DirectTrackState, TelemetryFrameState};
use taskmanager_telemetry_store::{CorrelatedSystemTelemetryIngestor, TelemetryStore};

/// Concrete accessibility bridge type linked into this frontend. On Linux this
/// is the real `accesskit_unix` adapter; elsewhere it is the contract's honest
/// detached bridge. Both implement `AccessibilityBridge`.
#[cfg(target_os = "linux")]
type AppAccessibilityBridge = taskmanager_accessibility_linux::LinuxAccessKitBridge;
#[cfg(not(target_os = "linux"))]
type AppAccessibilityBridge = taskmanager_ui_contract::DetachedAccessibilityBridge;

mod a11y;
pub mod alert_ui;
mod appearance;
pub mod batch_process;
pub mod capture;
pub mod chrome;
mod clipboard;
mod device_selection;
pub mod diagnostic_bundle;
mod dialog_scroll_state;
mod directory_usage;
pub mod dispatch;
mod gpu_engine_rows;
mod history_runtime;
mod input_modality;
mod interaction_state;
mod keyboard;
mod language;
mod nav;
mod navigation;
mod npu_inventory;
mod page_state;
mod persistence;
mod platform_batch;
pub(crate) mod platform_lists;
mod presentation_preferences;
mod proc_action;
mod process_control;
mod process_details_stats;
mod process_feedback;
mod process_insights_ui;
mod projection_caches;
mod projection_materialization;
mod render;
mod resize;
mod row_memos;
pub(crate) use resize::{
    PROC_COL_MAX_WIDTH, PROC_COL_MIN_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
};
mod graph_options;
pub mod responsive;
mod service_control;
mod services;
mod shared_interaction;
mod shell_state;
mod shell_ui;
mod sidebar_preferences;
mod snapshot_export;
mod startup;
mod telemetry_warmup;

pub(crate) use telemetry_warmup::TelemetryWarmupPhase;
pub mod system_health;
mod system_telemetry;
pub mod termination;
mod tooltip;
mod tray;
mod units;
mod window_surface;
pub use crate::core::alerts::QuietBound;
use capture::{CaptureEvidence, CaptureProcessAction};
pub use chrome::*;
pub use diagnostic_bundle::DiagnosticBundleUiState;
use dialog_scroll_state::DialogScrollState;
pub use dispatch::*;
pub use input_modality::InputModality;
pub(crate) use interaction_state::wire_debounced_search;
pub use interaction_state::{Hover, ProcMenuAction};
use interaction_state::{init_run_entity, init_search_entity};
pub use nav::*;
pub use navigation::{StableDeviceKind, TopPage};
pub use page_state::{
    NavOrientation, ProcessAffinityEditorState, ProcessesState, ServicesState, StartupState,
};
use persistence::{apply_process_config, config_from_view};
pub(crate) use presentation_preferences::{
    AppearancePreferences, DeviceVisibilityPreferences, GraphPreferences, MagnitudeBase,
    QuantityNotation,
};
pub use presentation_preferences::{
    DevicePreference, PresentationFingerprint, PresentationSnapshot, UnitFamily,
};
pub use process_feedback::ProcessControlAction;
pub(crate) use process_feedback::process_control_feedback;
pub use responsive::*;
use startup::page_token;
pub use startup::{StartupEnvironment, StartupRuntime, init};
use system_telemetry::SystemHistoryIngestionDiagnostic;
pub use termination::*;
pub(crate) use tooltip::ProcessHistories;
pub(crate) use tooltip::ProcessTooltipIndex;
pub use window_surface::{
    GpuiInputScope, GpuiSurfaceKind, WindowSurfaceDismissReason, WindowSurfaceKind,
    WindowSurfaceTransition,
};

pub struct RootView {
    pub theme: Theme,
    /// Immutable native local-time rules injected at composition startup.
    /// Projection/render paths never discover host files or environment state.
    pub(crate) local_time_rules: taskmanager_application::LocalTimeRulesObservation,
    /// The selected surface role is frontend composition state, not business
    /// state. Standalone is the default so existing desktop launches retain
    /// the complete application shell; the compact widget branch is only
    /// reachable through the explicit layer-shell host selection.
    pub(crate) surface_role: crate::window_presentation::GpuiSurfaceRole,
    pub nav_orientation: NavOrientation,
    /// Test-only override of the compositor-decoration branch that
    /// `gpui_app::root::render` reads each frame. `None` (the production
    /// value, never set outside tests) reads `window.window_decorations()` live so
    /// the renderer reacts to what the compositor actually granted after the
    /// `Server` request in [`crate::gpui_app::root::startup::init`].
    /// `Some(true)` forces the native-titlebar branch (Server — no app titlebar,
    /// no app-painted corners); `Some(false)` forces the CSD fallback (app
    /// titlebar + window controls + transparent rounded corners). The gpui test
    /// harness's `TestWindow` always reports `Decorations::Server`, so without
    /// this hook the CSD fallback path is unreachable from render tests.
    pub decorations_override: Option<bool>,
    /// Private authority for every persisted presentation axis. Runtime page,
    /// focus, scroll and sidebar interaction lifecycles remain separate.
    presentation: presentation_preferences::PresentationPreferences,
    /// Startup snapshot of which system families exist on this host. Detected
    /// once (font registration happens just before in `gpui_app::init`); every
    /// later skin/font change re-resolves against this same snapshot.
    pub font_availability: FontAvailability,
    /// Per-window focus origin. Root capture listeners update this before child
    /// controls handle the same input; render snapshots use it for focus-visible.
    pub input_modality: InputModality,
    /// Window-filtered pre-action keyboard observer. GPUI can resolve Tab into an
    /// action before element key listeners run, so root capture alone is not
    /// sufficient; the RootView owns this subscription for its full lifetime.
    pub(crate) input_modality_key_subscription: Option<Subscription>,
    /// Per-window switch states for the Settings dialog toggles (hc +
    /// device visibility), created lazily on first Settings render. The
    /// `Entity<SwitchState>` owns the `on` flag and focus handle, so two
    /// windows' toggles never share state (same ownership rule as
    /// `settings_slider`).
    pub settings_switches:
        HashMap<&'static str, Entity<taskmanager_ui::inputs::switch::SwitchState>>,
    /// Stable per-window handles for every long-form modal body. The dialogs
    /// share one bounded viewport + pinned-rail component but never share
    /// scroll position with each other or with another window.
    pub(crate) dialog_scroll: DialogScrollState,
    /// Per-window System-page scroll state (sectioned hardware cards).
    pub system_scroll: ScrollHandle,
    /// Per-window Performance statistics-rail scroll state. Every device page
    /// composes through the one fixed-viewport page root, so the rail is the
    /// only scrolling surface on the page.
    pub performance_stats_scroll: ScrollHandle,
    /// Per-window App-history list scroll state. The history page can contain
    /// more application groups than fit below its fixed title/status chrome;
    /// keeping this handle on the RootView avoids a process-global scroll
    /// position when more than one window is open.
    pub app_history_scroll: UniformListScrollHandle,
    /// Per-window sidebar device-list scroll state. The list can contain many
    /// disks, NICs, GPUs, batteries, and fans; its rail must remain paired
    /// with this handle across telemetry-driven renders.
    pub sidebar_scroll: ScrollHandle,
    /// Per-window Apps process-list and column scroll state.
    pub processes_scroll: processes_view::ProcessesScrollState,
    /// Per-window System dashboard scroll state.
    pub dashboard_scroll: ScrollHandle,
    /// Per-window System health scroll state.
    pub system_health_scroll: ScrollHandle,
    /// The app-level overlay host replacing the gc `Root` wrapper (P4): every
    /// modal/popup layer renders through a `taskmanager_ui` LayerStack.
    ///
    /// The stateless `elements::dialog_overlay` helpers host per-dialog stacks
    /// (keyed by call site, since they receive no RootView handle); this is the
    /// designated single-stack mount point for window-wide layers (toasts /
    /// popups / future P5 dialogs). Its render mount point lives at the end of
    /// Platform-neutral histories populated only from application-correlated
    /// system outcomes.
    pub telemetry: Arc<TelemetryStore>,
    /// The direct track's read view over `telemetry` — the same
    /// renderer-neutral live-graph authority the composed track's Iced/TUI
    /// shells hold. Capacity is the physical ring bound (`MAX_HISTORY_CAPACITY`);
    /// this frontend's `graph_data_points` preference keeps narrowing the
    /// visible tail at the graph element, so sliding and the y-ceiling keep
    /// reading the full retained window exactly as before the single-track
    /// convergence (ADR-034 GPU chart-metric sampling).
    pub(in crate::gpui_app) live_graph_history: taskmanager_shell::history::LiveGraphHistory,
    /// Frontend-retained write capability. Native providers never receive it.
    pub(crate) telemetry_ingestor: CorrelatedSystemTelemetryIngestor,
    /// Next-start preference plus this process's typed, boot-fixed history
    /// capabilities, replay lifecycle and boot baseline projection.
    pub(in crate::gpui_app) history_runtime: history_runtime::HistoryRuntimeState,
    /// The motion-preference token loaded this run. GPUI does not consume the
    /// preference (no switch is fabricated); the value is retained only so
    /// `config_from_view` echoes it instead of clobbering a recorded choice.
    pub(in crate::gpui_app) motion_token: String,
    /// Bounded read-model rejection diagnostics. These are not provider
    /// failures and never enter the platform failure stream.
    system_history_ingestion_diagnostics: Vec<SystemHistoryIngestionDiagnostic>,
    /// Local validated scheduler state; no operating-system round trip.
    pub telemetry_refresh_policy: TelemetryRefreshPolicy,
    /// The spawned system tray (ADR-032); `None` when the platform cannot
    /// host one (typed failure at spawn) or the tray was not started.
    pub(crate) tray_controller: Option<Box<dyn taskmanager_app_host::TrayController>>,
    /// The primary single-instance guard (ADR-032 follow-up); `Some` only
    /// when this process owns the instance. Held for the process lifetime.
    pub(crate) instance_guard: Option<Box<dyn taskmanager_app_host::InstanceGuard>>,
    pub(crate) instance_rx: Option<std::sync::mpsc::Receiver<taskmanager_app_host::InstanceEvent>>,
    pub(crate) tray_events_rx: Option<std::sync::mpsc::Receiver<crate::core::tray::TrayEvent>>,
    materialized: projection_materialization::ProjectionMaterialization,
    pub selected: SelectedDevice,
    /// Stable hardware identity behind an index-based view selection. The ID is
    /// retained while absent and re-resolved when the same device returns.
    pub stable_device_selection: StableDeviceSelection,
    pub stable_device_kind: Option<StableDeviceKind>,
    pub selected_device_missing: bool,
    pub page: TopPage,
    /// Production-only native platform ports. The Root view owns only the
    /// platform-neutral application contract; the OS-specific adapter is
    /// selected and boxed at the executable composition edge.
    pub(crate) platform: Option<PlatformClient>,
    /// Bounded diagnostic tail of completed v2 platform failures.
    pub(crate) platform_failures: Vec<OperationFailure>,
    /// Lazy PID lookup used only by the cursor-tooltip projection. Keeping it
    /// separate from the row projection avoids copying command lines into every
    /// visible-row model while removing an O(process-count) scan per row hover.
    pub(crate) process_tooltip_index: ProcessTooltipIndex,
    /// Narrow renderer-neutral evidence window for alert and SMART suggestions.
    /// Live graph facts are read from the correlated `TelemetryStore`; this
    /// window must not become a second chart-history authority (ADR-027).
    pub smart_history: taskmanager_application::AlertSuggestionWindow,
    /// Private owner of every revision/fingerprint-keyed renderer projection.
    /// Callers receive immutable `Rc` snapshots; no `RefCell` guard escapes.
    projection_caches: projection_caches::GpuiProjectionCaches,
    /// Cached per-core CPU utilization projection for the CPU page, invalidated
    /// on every accepted CPU-domain telemetry outcome (see `CpuHistoryCache`).
    pub(crate) cpu_core_history: CpuHistoryCache,
    /// Cached memory + swap sample projections for the Memory page header
    /// charts, invalidated on every accepted Memory-domain telemetry outcome
    /// (see `MemoryHistoryCache`).
    pub(crate) memory_history: MemoryHistoryCache,
    /// Shell-owned direct-track interactive state (ADR-027): the process
    /// selection (pid set + anchor), the Services/Startup/Users inventory
    /// sorts, and the typed action-feedback slots all live in
    /// `taskmanager-shell` reducers — this window only holds the instance and
    /// folds typed values into render state (see `root/shell_state.rs`). No
    /// authoritative selection/sort/feedback field may live beside it.
    pub shell: DirectTrackState,
    /// Provider-issued authority target of the selected service. Display names
    /// are never reconstructed into native control or observation targets.
    pub selected_service: Option<ServiceId>,
    /// Provider-issued identity of the currently-selected startup entry.
    pub selected_startup: Option<StartupEntryId>,
    /// Id of the currently-selected session row on the Users page, or `None`.
    pub selected_session: Option<String>,
    pub desktop_appearance: DesktopAppearance,
    /// Native-adapter source truth used to choose the initial visual skin.
    pub desktop_appearance_sources: Vec<SourceStatus>,
    /// Renderer-local interactive surfaces for this window. Process
    /// Properties and dangerous confirmations are excluded: their sole
    /// authority is `shell.interaction` in the application-owned direct track.
    window_surface: window_surface::WindowSurfaceState,
    /// First-run workflow data. Visibility is owned by `window_surface`.
    pub first_run: first_run::FirstRunUiState,
    pub(crate) first_run_requests: HashMap<RequestId, SetupScriptAction>,
    /// Per-window persistent refresh-interval slider entity for the Settings
    /// dialog (owns the thumb position, drag state, and current value).
    /// Created lazily on the first Settings render (see
    /// `settings_view::init_slider_entity`); never shared between windows —
    /// the old shared `thread_local SLIDER` leaked drag/thumb/value state
    /// across windows.
    pub settings_slider: Option<Entity<SliderState>>,
    /// Per-window persistent Data Points slider state for Performance
    /// Settings. The range is 10..=600 and never crosses window boundaries.
    pub graph_points_slider: Option<Entity<SliderState>>,
    /// One-line error feedback from the last command-launch outcome (None when the
    /// dialog has no error to show). Cleared on successful launch / dialog close.
    pub run_error: Option<String>,
    /// Active section in the process Properties dialog. RootView owns this UI
    /// state; `root::chrome` renders stateless Overview/Performance/Command views.
    pub details_section: ProcessDetailsSection,
    /// Exact target/request-correlated lifecycle for independently scheduled
    /// process-insight facets. Application owns facet correlation; this
    /// component accepts only the matching shared projection and cannot keep
    /// request and terminal state in separate optional slots.
    process_insights: process_insights_ui::ProcessInsightsLifecycle,
    /// Bounded audit trail for completed multi-process actions. It records the
    /// frozen identities and each per-target outcome, never live process data.
    pub process_batch_history: ProcessBatchHistory,
    /// Transient one-line feedback from the last non-signal process context-menu
    /// action (e.g. `"Location unavailable"` from "Open file location" when the exe
    /// path can't be resolved). Rendered as a deferred top-center toast (mirrors the
    /// Paused badge); cleared at the start of the next `apply_proc_action`.
    pub local_feedback_toast: Option<Entity<taskmanager_ui::overlays::toast::ToastState>>,
    /// Dismiss subscription for the current feedback toast (cleared when it
    /// auto-dismisses or is replaced).
    pub local_feedback_subscription: Option<Subscription>,
    /// Monotonic toast id source.
    pub local_feedback_seq: u64,
    /// Application-correlated export lifecycle plus the app-host client. No
    /// renderer path owns serialization or filesystem publication.
    snapshot_export: snapshot_export::SnapshotExportRuntime,
    pub(crate) diagnostic_bundle_runtime: diagnostic_bundle::DiagnosticBundleRuntime,
    /// Per-window background dependency/log/export state behind the service
    /// details dialog. Owned here (never a shared `thread_local`, which crossed
    /// window boundaries): each window's dialog shows only its lifecycle target,
    /// log feed and orthogonal pause/level/time filters. Export outcomes enter
    /// the shell's typed feedback authority.
    pub service_details: services_view::ServiceDetailsState,
    /// Latest injected application-tick wall-clock sample. Service-log
    /// filtering and export consume this cache; render never reads the host
    /// clock.
    pub(crate) service_log_now_ms: u64,
    /// Renderer-local cadence and device binding for the on-demand GPU engine
    /// refresh loop. Request/terminal authority and accepted payload live in
    /// `shell`; this state can only schedule or stop future submissions.
    gpu_engine_pacing: gpu_engine_rows::GpuEnginePacingState,
    /// The currently-hovered interactive element (see [`Hover`]); drives hover overlays.
    pub hovered: Option<Hover>,
    /// Live graph-hover slot for this window's Performance graphs: the
    /// window-space cursor position + formatted value under the pointer.
    /// Written by `graph::graph_element_hover`'s mouse-move/leave listeners,
    /// read by the page renderers (`perf_views` / `cpu_view`) to place the
    /// cursor-following tooltip. Per-window — each RootView owns its own slot,
    /// so hovering a graph in one window never paints a tooltip in another.
    pub graph_hover: Rc<RefCell<Option<crate::gpui_app::graph::GraphHover>>>,
    /// Typed lifecycle of the visible telemetry frame. Partial domain
    /// arrivals stay in the shared pending projection and do not clear the
    /// committed-frame gate.
    pub telemetry_frame_state: TelemetryFrameState,
    /// Monotonic UI watchdog for the first complete telemetry frame. This is
    /// deliberately separate from the renderer-neutral lifecycle: timeout is
    /// a presentation affordance, not a fabricated provider failure.
    pub(crate) telemetry_warmup_started_at: Instant,
    /// Lazily-owned retry control for the startup mask. It lives on RootView
    /// so pointer and keyboard activation share one per-window state entity.
    /// `None` also means that the next render must create and focus it, so one
    /// authority owns both mount and initial-focus lifecycle.
    pub(crate) telemetry_warmup_retry_button: Option<Entity<ButtonState>>,
    /// One-shot request consumed after the destination page has rendered its
    /// search entity. The destination is captured with the request so an
    /// intervening page change cannot redirect focus to the wrong input.
    pub(crate) pending_search_focus: Option<TopPage>,
    /// Per-window own-text-input state for the Apps-page search box. Created
    /// lazily on the first Apps render (see [`init_search_entity`]); drives
    /// the shell-owned process query (`shell.processes`, via
    /// [`RootView::set_process_query`]) through its Change subscription.
    /// Never shared between windows.
    pub(crate) search_input: Option<Entity<TextInputState>>,
    /// Per-window own-text-input state for the Run dialog's command field.
    /// Created lazily on the first Run-dialog render (see [`init_run_entity`]);
    /// is the sole command-text authority read by the typed submit path. Never
    /// shared between windows; no parallel RootView string mirrors it.
    pub(crate) run_input: Option<Entity<TextInputState>>,
    /// Per-page UI state (services / startup / processes), each in its own sub‑struct.
    pub services_state: ServicesState,
    pub services_search: Option<Entity<TextInputState>>,
    /// Shared per-window source-recovery button entity. The page-specific
    /// render projection supplies the independent `RefreshRequest`; the
    /// focus state itself is presentation-only and can be reused as the user
    /// moves between Services, Startup, and Users.
    pub source_retry_button: Option<Entity<ButtonState>>,
    /// Per-window persistent Table entity for the Services page (owns the scroll
    /// position, sort, selection, and focus of the services table). Created lazily on
    /// the first Services render (see `services_view::init_table_entity`); never shared
    /// between windows — a shared `thread_local` used to leak scroll/sort/selection
    /// across windows.
    pub services_table: Option<Entity<TableState<services_view::ServicesDelegate>>>,
    pub startup_state: StartupState,
    pub startup_search: Option<Entity<TextInputState>>,
    /// Per-window persistent Table entity for the Startup page; same ownership rule as
    /// `services_table`.
    pub startup_table: Option<Entity<TableState<startup_view::StartupDelegate>>>,
    pub users_table: Option<Entity<TableState<users_view::UsersDelegate>>>,
    pub processes_state: ProcessesState,
    /// Per-window visibility of the Performance device navigator. This is
    /// transient presentation state: F9 changes the layout only and never
    /// touches telemetry, provider, or persisted device preferences.
    pub sidebar_visible: bool,
    /// Transient edit mode for the Performance device sidebar. Reordering and
    /// concrete show/hide actions persist through the typed sidebar config;
    /// the edit affordance itself is intentionally per-window and transient.
    pub sidebar_edit_mode: bool,
    /// Cursor-x captured at the start of a sidebar drag so every `on_drag_move`
    /// is a stable delta from the drag origin (mirrors
    /// `ProcessesState::resize_anchor_x` for the column drag).
    pub(crate) sidebar_resize_anchor_x: Option<Pixels>,
    /// Coalesces cursor-following tooltip invalidations to one RootView update
    /// per animation frame. Pointer devices can emit hundreds of MouseMove
    /// events per frame; the table hover surface itself only changes when the
    /// pointer crosses a row, so rebuilding the whole root for every event is
    /// unnecessary work.
    cursor_refresh_state: render::CursorRefreshState,
    /// Root-owned dashboard history, rule manager, event center, and saved Apps
    /// view presets. Views consume this typed state without collection or I/O.
    pub dashboard: DashboardState,
    /// Capture-only readiness markers and deterministic visual scenarios. This
    /// remains disabled unless `TM_CAPTURE_EVIDENCE`/`TM_CAPTURE_SCENARIO` is set.
    capture_evidence: CaptureEvidence,
    /// Linked native accessibility bridge. On Linux this owns a real
    /// `accesskit_unix::Adapter` that publishes the semantic tree to AT-SPI;
    /// on other targets it is the contract's detached (no-op) bridge.
    pub(crate) a11y_bridge: AppAccessibilityBridge,
    /// Monotonic revision stamped onto each published accessibility snapshot.
    /// Adapters reject stale inbound actions against this revision.
    pub(crate) a11y_revision: u64,
}

impl RootView {
    /// Immutable canonical facts owned by the shell direct-track component.
    #[must_use]
    pub(crate) const fn projection(&self) -> &taskmanager_shell::SystemProjectionStore {
        self.shell.projection()
    }

    /// Visible-row projection shared by the render path and keyboard paging.
    ///
    /// The row model (ordered `VisibleRow`s plus their pid order) is a pure
    /// function of `procs` + the shell-owned process viewing state (query +
    /// status filter + sort, read from `self.shell.processes`) + the
    /// processes state; it is cached so hover-driven re-renders and
    /// PageUp/PageDown presses reuse the previous build instead of
    /// re-filtering, re-sorting, and re-cloning 10k rows per event. The
    /// cache is invalidated by the tick (a new `processes_generation`) or by
    /// any state change the key compares.
    pub fn processes_projection(
        &mut self,
    ) -> (
        std::rc::Rc<Vec<processes_view::rows::VisibleRow>>,
        std::rc::Rc<Vec<u32>>,
        String,
    ) {
        let (sort_col, sort_dir) = self.shell.processes.sort();
        let sort_asc = matches!(sort_dir, taskmanager_shell::SortDir::Asc);
        let filter = self.shell.processes.status_filter();
        let query = self.shell.processes.query().trim().to_owned();
        if let Some(cache) = self.projection_caches.processes()
            && cache.processes_generation == self.processes_generation()
            && cache.query == query
            && cache.sort_col == sort_col
            && cache.sort_asc == sort_asc
            && cache.filter == filter
            && cache.collapsed == self.processes_state.collapsed
            && cache.expanded_apps == self.processes_state.expanded_apps
            && cache.local_time_rules == self.local_time_rules.cache_key()
        {
            return (cache.rows.clone(), cache.pids.clone(), cache.query.clone());
        }
        let procs_refs: Vec<&crate::core::process::ProcessItem> = self.processes().iter().collect();
        let application_count = processes_view::rows::application_root_count(&procs_refs);
        let rows = processes_view::rows::visible_rows_with_local_time(
            processes_view::rows::VisibleRowsProps {
                processes: &procs_refs,
                query: &query,
                sort_col,
                sort_asc,
                filter,
                collapsed: &self.processes_state.collapsed,
                expanded_apps: &self.processes_state.expanded_apps,
            },
            &self.local_time_rules,
        );
        let rows = std::rc::Rc::new(rows);
        let pids = std::rc::Rc::new(
            rows.iter()
                .filter_map(|row| row.process_pid)
                .collect::<Vec<_>>(),
        );
        self.projection_caches
            .replace_processes(processes_view::rows::ProjectionCache {
                processes_generation: self.processes_generation(),
                query: query.clone(),
                sort_col,
                sort_asc,
                filter,
                collapsed: self.processes_state.collapsed.clone(),
                expanded_apps: self.processes_state.expanded_apps.clone(),
                local_time_rules: self.local_time_rules.cache_key(),
                rows: rows.clone(),
                pids: pids.clone(),
                application_count,
            });
        (rows, pids, query)
    }

    /// Cached application-root count for the current process generation.
    #[must_use]
    pub fn process_application_count(&self) -> usize {
        self.projection_caches.application_count()
    }
    pub fn new(theme: Theme, cx: &mut Context<Self>) -> Self {
        let (telemetry, telemetry_ingestor) = TelemetryStore::shared_with_correlated_ingestion(
            taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY,
        );
        Self::new_inner(
            theme,
            telemetry,
            telemetry_ingestor,
            TelemetryRefreshPolicy::default(),
            None,
            crate::window_presentation::GpuiSurfaceRole::Standalone,
            cx,
        )
    }

    /// Construct the interactive shell with an application-owned platform
    /// client. The telemetry store remains a read model; keyboard and settings
    /// controls submit typed requests through `platform`.
    pub fn new_with_platform(
        theme: Theme,
        telemetry: Arc<TelemetryStore>,
        telemetry_ingestor: CorrelatedSystemTelemetryIngestor,
        telemetry_refresh_policy: TelemetryRefreshPolicy,
        platform: PlatformClient,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_platform_and_surface_role(
            theme,
            telemetry,
            telemetry_ingestor,
            telemetry_refresh_policy,
            platform,
            crate::window_presentation::GpuiSurfaceRole::Standalone,
            cx,
        )
    }

    /// Construct the interactive shell with an explicit frontend surface
    /// role. Startup uses this only for the opt-in layer-shell widget; the
    /// existing [`Self::new_with_platform`] API remains standalone.
    pub(crate) fn new_with_platform_and_surface_role(
        theme: Theme,
        telemetry: Arc<TelemetryStore>,
        telemetry_ingestor: CorrelatedSystemTelemetryIngestor,
        telemetry_refresh_policy: TelemetryRefreshPolicy,
        platform: PlatformClient,
        surface_role: crate::window_presentation::GpuiSurfaceRole,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(
            theme,
            telemetry,
            telemetry_ingestor,
            telemetry_refresh_policy,
            Some(platform),
            surface_role,
            cx,
        )
    }

    fn new_inner(
        theme: Theme,
        telemetry: Arc<TelemetryStore>,
        telemetry_ingestor: CorrelatedSystemTelemetryIngestor,
        telemetry_refresh_policy: TelemetryRefreshPolicy,
        platform: Option<PlatformClient>,
        surface_role: crate::window_presentation::GpuiSurfaceRole,
        cx: &mut Context<Self>,
    ) -> Self {
        let live_graph_history = taskmanager_shell::history::LiveGraphHistory::from_store(
            telemetry.clone(),
            taskmanager_shell::history::MAX_HISTORY_CAPACITY,
        );
        // Own UI layer bootstrap: focus registry + input/dialog/popup/table/tree
        // keymaps. The startup path already ran it; tests constructing RootView
        // directly get it here. Idempotent (P6: replaces the old
        // `gpui_component::init`).
        taskmanager_ui::init(cx);
        let capture_evidence = CaptureEvidence::from_environment();
        capture_evidence.mark_theme(&theme);
        // Linux/Wayland: the window surface is transparent and the chrome
        // paints the per-skin corner radius itself (startup already set this on
        // the initial theme; direct construction in tests gets it here too, so
        // the rounded render path is exercised on the CI Linux runners).
        let mut theme = theme;
        theme.window_transparent = cfg!(target_os = "linux");
        let mut shell = DirectTrackState::default();
        if let Some(platform) = platform.as_ref() {
            shell.apply_capability_snapshot(platform.capabilities().snapshot());
        }
        Self {
            theme,
            local_time_rules: taskmanager_application::LocalTimeRulesObservation::unsupported(0),
            surface_role,
            presentation: presentation_preferences::PresentationPreferences::default(),
            nav_orientation: NavOrientation::Horizontal,
            decorations_override: None,
            font_availability: detect_font_availability(cx),
            input_modality: InputModality::default(),
            input_modality_key_subscription: None,
            settings_switches: HashMap::new(),
            dialog_scroll: DialogScrollState::default(),
            system_scroll: ScrollHandle::new(),
            performance_stats_scroll: ScrollHandle::new(),
            app_history_scroll: UniformListScrollHandle::new(),
            sidebar_scroll: ScrollHandle::new(),
            processes_scroll: processes_view::ProcessesScrollState::default(),
            dashboard_scroll: ScrollHandle::new(),
            system_health_scroll: ScrollHandle::new(),
            telemetry,
            live_graph_history,
            telemetry_ingestor,
            history_runtime: history_runtime::HistoryRuntimeState::default(),
            motion_token: crate::core::config::MOTION_NORMAL.to_string(),
            projection_caches: projection_caches::GpuiProjectionCaches::default(),
            system_history_ingestion_diagnostics: Vec::new(),
            telemetry_refresh_policy,
            tray_controller: None,
            instance_guard: None,
            instance_rx: None,
            tray_events_rx: None,
            materialized: projection_materialization::ProjectionMaterialization::default(),
            selected: initial_selected(),
            stable_device_selection: StableDeviceSelection::default(),
            stable_device_kind: None,
            selected_device_missing: false,
            page: initial_page(),
            platform,
            platform_failures: Vec::new(),
            process_tooltip_index: ProcessTooltipIndex::default(),
            smart_history: taskmanager_application::AlertSuggestionWindow::new(),
            cpu_core_history: CpuHistoryCache::new(),
            memory_history: MemoryHistoryCache::new(),
            shell,
            selected_service: None,
            selected_startup: None,
            selected_session: None,
            desktop_appearance: DesktopAppearance::default(),
            desktop_appearance_sources: Vec::new(),
            window_surface: window_surface::WindowSurfaceState::initial(
                std::env::var("TM_SETTINGS")
                    .map(|s| !s.is_empty() && s != "0")
                    .unwrap_or(false)
                    .then_some(window_surface::WindowSurface::Settings),
            ),
            first_run: first_run::FirstRunUiState::default(),
            first_run_requests: HashMap::new(),
            settings_slider: None,
            graph_points_slider: None,
            run_error: None,
            details_section: ProcessDetailsSection::default(),
            process_insights: process_insights_ui::ProcessInsightsLifecycle::default(),
            process_batch_history: ProcessBatchHistory::default(),
            local_feedback_toast: None,
            local_feedback_subscription: None,
            local_feedback_seq: 0,
            snapshot_export: snapshot_export::SnapshotExportRuntime::default(),
            diagnostic_bundle_runtime: diagnostic_bundle::DiagnosticBundleRuntime::default(),
            service_details: services_view::ServiceDetailsState::new(),
            service_log_now_ms: 0,
            gpu_engine_pacing: gpu_engine_rows::GpuEnginePacingState::default(),
            hovered: None,
            graph_hover: Rc::new(RefCell::new(None)),
            telemetry_frame_state: TelemetryFrameState::Collecting,
            telemetry_warmup_started_at: Instant::now(),
            telemetry_warmup_retry_button: None,
            pending_search_focus: None,
            search_input: None,
            run_input: None,
            services_state: ServicesState::default(),
            services_search: None,
            source_retry_button: None,
            services_table: None,
            startup_state: StartupState::default(),
            startup_search: None,
            startup_table: None,
            users_table: None,
            processes_state: ProcessesState::default(),
            sidebar_visible: true,
            sidebar_edit_mode: false,
            sidebar_resize_anchor_x: None,
            cursor_refresh_state: render::CursorRefreshState::Idle,
            dashboard: DashboardState::new(),
            capture_evidence,
            a11y_bridge: AppAccessibilityBridge::default(),
            a11y_revision: 0,
        }
    }

    /// Queue list collection on the background worker. This method performs no
    /// filesystem access and is safe to call from any GPUI event closure.
    pub(crate) fn request_refresh(&mut self, request: RefreshRequest) {
        if let Some(platform) = &mut self.platform {
            let _ = platform.request_refresh(request, platform_submission_time_ms());
        }
    }

    /// Advance the runtime-owned ECS scheduler. Periodic refreshes must enter
    /// the same application request path as manual commands; the frontend only
    /// supplies a clock tick and pause state.
    pub(crate) fn run_scheduled_refresh(&mut self, now_ms: u64) {
        self.service_log_now_ms = now_ms;
        if self.telemetry_refresh_policy.is_paused() {
            return;
        }
        if let Some(platform) = &mut self.platform {
            platform.set_telemetry_interval(self.telemetry_refresh_policy.interval());
            let _ = platform.run_scheduled_refresh(now_ms);
        }
    }

    fn record_platform_failures(&mut self, failures: Vec<OperationFailure>) {
        const FAILURE_HISTORY_CAPACITY: usize = 32;
        self.platform_failures.extend(failures);
        if self.platform_failures.len() > FAILURE_HISTORY_CAPACITY {
            let remove = self.platform_failures.len() - FAILURE_HISTORY_CAPACITY;
            self.platform_failures.drain(..remove);
        }
    }

    pub(crate) fn drain_instance_events(
        &mut self,
        cx: &mut Context<Self>,
        window_handle: AnyWindowHandle,
    ) {
        let mut activate = false;
        if let Some(rx) = &self.instance_rx {
            activate = rx.try_recv().is_ok();
            while rx.try_recv().is_ok() {
                activate = true;
            }
        }
        if activate {
            cx.activate(true);
            let _ = window_handle.update(cx, |_, window, _| window.activate_window());
        }
    }
}

pub(crate) fn platform_submission_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}
