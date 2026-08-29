//! The iced application state: platform data flow and the `Message` loop.
//!
//! All domain state lives in the shared [`ShellApp`] (ADR-027); this module
//! only owns the iced-specific glue: the optional
//! [`taskmanager_application::PlatformClient`], the
//! runtime scheduler, the mapping of iced events (tick, keyboard) onto
//! shell operations, and the frontend-local modal surfaces (settings, about,
//! health, containers, process details). The view reads the same shell state
//! through [`crate::ui`].
//!
//! The heavier method groups live in child modules and are re-exported here so
//! the [`IcedApp`] surface stays unchanged: `constructors` (new/demo),
//! `settings` (settings application + theme rebuild), `refresh` (the tick
//! loop), `focus_state` (the focus command policy).

use std::time::{Duration, Instant};

use taskmanager_application::{
    AppAction, AppPage, ConfigClient, PlatformEffect, RefreshRequest, TelemetryInterval,
};
use taskmanager_core::core::config::Config;
use taskmanager_core::core::process::ProcessBatchAction;
use taskmanager_core::core::services::{ServiceAction, ServiceItem};
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::setup::SetupScriptAction;

use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, InfoSortCol, InfoTable,
    ProcessStatusFilter, ShellApp, SortCol,
};
use taskmanager_theme::{FontChoice, HighContrast, LightDark, Skin, Theme};

mod accessors;
mod affinity;
pub(crate) mod alerts;
pub(crate) mod appearance;
mod capture_state;
mod column_menu;
mod config_sync;
mod configuration_state;
mod constructors;
mod focus_state;
mod focus_targets;
pub(crate) mod history_replay;
mod history_series;
mod input_state;
mod menus;
mod motion;
mod navigation;
mod performance_state;
mod pointer_capture;
mod preferences;
mod prefs_accessors;
mod process_menu;
mod process_presentation_state;
mod projection;
mod projection_caches;
mod refresh;
mod runtime;
mod scroll;
mod selectors;
mod service_details;
mod service_log;
mod service_menu;
mod settings;
mod settings_types;
mod snapshot_export;
mod startup_menu;
mod subscription;
mod surface;
mod update;
mod viewport_state;
mod window_time;

// Frontend-local view selectors live in the [`selectors`] module and are
// re-exported below so the historical
// `crate::app::<Type>` path (used by the view + tests) stays unchanged.
pub(crate) use projection::AppHistoryRowModel;
use projection_caches::IcedProjectionCaches;
pub(crate) use scroll::VirtualScrollState;
pub use selectors::PerfDevice;
// The settings/dialog vocabulary and the focus-target registry moved to their
// own modules to stay under the source-size budget; the historical
// `crate::app::<Type>` paths stay valid through these re-exports.
pub use alerts::AlertsMessage;
pub use focus_targets::FocusTarget;
// The first-run dialog and System dashboard segment vocabularies (the same
// re-export shape `AlertsMessage` uses so the `Message` payload types stay
// nameable at the crate boundary).
pub use crate::ui::first_run::FirstRunMessage;
pub use crate::ui::system_dashboard::SystemDashboardMessage;
pub use motion::{ModalAppear, PageNav, WarmupSpin};
pub use preferences::PresentationPreferences;
pub use process_menu::ProcessMenuAction;
pub use service_details::ServiceDetailsSnapshot;
pub use settings_types::{DetailsSection, DeviceKind, ModeChoice, SettingsChange};
// Session-local process-table column sizing (drag overrides + the open drag
// session), reduced by `update::columns`.
pub(crate) use surface::{
    ContextMenu, ContextMenuKind, ContextMenuState, InputScope, LocalSurface, LocalSurfaceKind,
    LocalSurfaceState, PresenceTransition,
};
pub(crate) use update::columns::keyboard_resize_width;
pub(crate) use update::columns::{ColumnWidthOverrides, ProcessColumnSizing};

use crate::i18n::Language;
use crate::keys::IcedKey;

const EVENT_POLL: Duration = Duration::from_millis(100);
/// The iced frontend's message loop.
#[derive(Debug, Clone)]
pub enum Message {
    /// Platform poll + refresh scheduling tick.
    Tick,
    /// A keyboard event normalized by [`crate::keys`].
    Key(IcedKey),
    /// Live modifier state changed (Shift / Ctrl / Super). Tracked so a row
    /// click can branch on the modifiers held at click time without the view
    /// needing to inspect the pointer event.
    ModifiersChanged(iced::keyboard::Modifiers),
    /// A pointer button was pressed anywhere over the root surface. This
    /// feeds only the renderer-local input-modality tracker (see
    /// [`crate::input_modality`]) — the iced counterpart of the GPUI root's
    /// capture-phase mouse-down listener; it never carries surface semantics.
    PointerPressed,
    /// One selectable value began a selection and claims the window's single
    /// active selection (the reference selection-registry rule, routed
    /// through the shell-free input state).
    TextSelectionClaimed(iced::advanced::widget::Id),
    /// Backspace pressed while the search field is active.
    SearchBackspace,
    /// A page tab was clicked.
    SelectPage(AppPage),
    /// A Performance resource tab was clicked (select-a-device model). Sets the
    /// frontend-local selector; the shell never sees this, so it cannot affect
    /// the shared router or other frontends.
    SelectPerfDevice(PerfDevice),
    /// An Applications process-state filter pill was clicked. The shell owns
    /// the filtered row projection so selection, keyboard navigation, and
    /// actions consume the same six-state list the renderer shows.
    SelectProcessStatusFilter(ProcessStatusFilter),
    /// A hierarchy aggregate was activated — insert/remove its stable key in
    /// the frontend-local expanded set so the next render shows/hides its
    /// member rows, AND select the group's main process (the header is a
    /// selectable row like gpui's aggregate rows). The `flat_index` is the
    /// main pid's position in the shared process list, so the shared cursor
    /// stays in bounds of the visible rows.
    ToggleGroupExpansion {
        name: String,
        main_pid: u32,
        flat_index: usize,
        row_key: Option<taskmanager_shell::ProcessRowId>,
    },
    /// A recursive process parent was activated — toggle its subtree AND select the
    /// row in one click. The toggle flips membership in the frontend-local
    /// collapsed-pid set; the selection follows the shared cursor so the
    /// properties/insights overlays attach to the parent. `flat_index` is the
    /// pid's position in the shared process list (stable across collapse —
    /// only descendants appear/disappear — so both can happen on one
    /// activation).
    ActivateTreeNode { pid: u32, flat_index: usize },
    /// A process row was right-clicked. The row index selects the shared
    /// process identity; the pid is retained locally so a later refresh cannot
    /// make the menu appear attached to a different row.
    OpenProcessRowMenu { flat_index: usize, pid: u32 },
    /// Close the Applications-row context menu.
    CloseProcessRowMenu,
    /// Apply one action from the Applications-row context menu.
    ProcessMenuAction(ProcessMenuAction),
    /// Open the Applications column-visibility menu.
    OpenProcessColumnsMenu,
    /// Close the Applications column-visibility menu.
    CloseProcessColumnsMenu,
    /// Toggle one hideable Applications column.
    ToggleProcessColumn(SortCol),
    /// Reset Applications table columns to the default layout: every hideable
    /// column visible again and every width override (and its persisted
    /// token) cleared.
    ResetProcessColumns,
    /// A resizable process-table header edge was pressed: open the
    /// frontend-local drag session for that column at its current rendered
    /// width. The edge's `mouse_area` captures the press, so the sort click
    /// underneath never fires.
    BeginProcessColumnDrag { column: SortCol, start_width: f32 },
    /// The pointer moved while a process-column drag session is open. Fed by
    /// the raw pointer subscription mounted only while a session exists; the
    /// reducer derives the live width from the session's anchor.
    ProcessColumnDragMoved(iced::Point),
    /// The left button was released, closing the drag session. The stored
    /// width override stays and is committed to the persisted configuration
    /// token at this point (the single persistence point of a drag).
    ProcessColumnDragReleased,
    /// Set one process-table column's width override directly. The drag path
    /// and the keyboard/menu stepper path (the column menu's per-column
    /// widen/narrow controls) share this transition; each stepper step also
    /// commits the override set to the persisted configuration token. Purely
    /// frontend-local: no shell effect, clamped to the sizing domain on store.
    ResizeProcessColumn { column: SortCol, width: f32 },
    /// A Services row was right-clicked. `visual_index` keeps the shared
    /// selection highlight aligned with the rendered table; `source_index`
    /// freezes the provider-order identity used by the action path.
    OpenServiceRowMenu {
        visual_index: usize,
        source_index: usize,
    },
    /// Close the Services-row context menu.
    CloseServiceRowMenu,
    /// Open the log stream for the selected service (submits the initial
    /// follow request through the shell's shared state machine).
    OpenServiceLog,
    /// Select a service row and open its log stream. The row index is resolved
    /// against provider order by the Services page before this message is
    /// published.
    OpenServiceLogFor { index: usize },
    /// Select a service row and open its dependency/lifecycle details modal.
    /// The dependency query is submitted through the shared typed effect lane.
    OpenServiceDetailsFor { index: usize },
    /// Retry the dependency query for the open service-details modal.
    RefreshServiceDetails,
    /// Toggle paused/running for the details modal's merged log panel.
    ToggleServiceDetailsLogPaused,
    /// Cycle the details modal's merged log level filter.
    CycleServiceDetailsLogLevel,
    /// Cycle the details modal's merged log time filter.
    CycleServiceDetailsLogTime,
    /// Copy the details modal's merged log lines to the clipboard.
    CopyServiceDetailsLog,
    /// Immediately re-request the details modal's merged log stream.
    RefreshServiceDetailsLogs,
    /// Close the open service-log stream.
    CloseServiceLog,
    /// Toggle follow-on/off for the open service-log stream.
    ToggleLogFollow,
    /// Toggle paused/running for the open service-log stream.
    ToggleLogPaused,
    /// Cycle the open stream's level filter.
    CycleLogLevel,
    /// Cycle the open stream's time window.
    CycleLogTime,
    /// Copy the currently visible filtered service-log entries.
    CopyServiceLog,
    /// Export the currently visible filtered service-log entries through the
    /// bounded diagnostic worker.
    ExportServiceLog,
    /// A visible table row was clicked.
    SelectRow(usize),
    /// A process-table column header was clicked. Sort by that column; when it
    /// is already the active column this mirrors the `S` chord and flips
    /// direction. Resolves to the same shell sort state the `s`/`S` bindings
    /// mutate — never a parallel sort path.
    SortBy(SortCol),
    /// An inventory-table header (Services / Startup / Users) was clicked.
    /// Routes through the shell's single per-table sort slot
    /// ([`ShellApp::set_info_sort`]) so selection indexes always map to the
    /// same visible row order across frontends.
    SortInfoTable {
        table: InfoTable,
        column: InfoSortCol,
    },
    /// The search field text changed.
    SearchChanged(String),
    /// The search field was focused.
    FocusSearch,
    /// The search field was closed.
    CloseSearch,
    /// The Services-page filter input changed (a frontend-local query that
    /// never touches the shared process query).
    ServicesSearchChanged(String),
    /// Refresh the inventory page whose source reported a retryable failure.
    RefreshSource(RefreshRequest),
    /// End-task was requested (shows the confirmation bar).
    RequestEndTask,
    /// Request a batch process-control action (Suspend / Resume / Kill /
    /// SetPriority) on the selected process through the shell's shared batch
    /// path. Non-destructive actions submit directly; a destructive one is
    /// gated behind a confirmation (mirrors the End-task flow).
    RequestProcessBatch(ProcessBatchAction),
    /// The pending end-task confirmation was confirmed.
    ConfirmEndTask,
    /// The pending destructive batch (Kill) confirmation was confirmed.
    ConfirmProcessBatch,
    /// The pending end-task confirmation was dismissed.
    DismissOverlay,
    /// Request a direct action for the currently selected login session.
    RequestSessionControl(SessionControlAction),
    /// A Users row was right-clicked: select it and open its row context
    /// menu (Disconnect / Lock, GPUI parity).
    OpenUserRowMenu(usize),
    /// Close the open Users row context menu (Escape or a menu action).
    CloseUserRowMenu,
    /// A Startup row was right-clicked. The visual index is resolved to the
    /// provider-issued entry identity before any menu action is submitted.
    OpenStartupRowMenu { visual_index: usize },
    /// Close the Startup-row context menu.
    CloseStartupRowMenu,
    /// Request enable (true) / disable (false) of the currently selected
    /// startup entry. Submits through the shell's shared startup-control
    /// request (latest-wins), mirroring [`Message::RequestSessionControl`].
    RequestStartupControl(bool),
    /// Request Startup enable/disable for an exact provider-order entry from
    /// the row context menu. This keeps sorting from retargeting the action.
    RequestStartupControlFor { index: usize, enabled: bool },
    /// Confirm a gated startup Enable/Disable (mirrors GPUI's confirm dialog).
    /// The shell's `request_startup_control` only sets the pending slot; this
    /// message emits the actual StartupControl effect.
    ConfirmStartupControl,
    /// A toolkit focusable adapter reported its focus target.
    Focus(FocusTarget),
    /// A real Iced frame was requested by the evidence runner.
    Frame(Instant),
    /// The native window changed size; this drives the frontend-local
    /// responsive layout breakpoint and never crosses into the shell.
    WindowResized(iced::Size),
    /// The native titlebar requested a close. With a live tray this is a
    /// minimize-to-tray action; without one it closes the only window.
    WindowCloseRequested,
    /// The Applications table viewport moved. The view uses the absolute
    /// vertical offset to materialize only its visible row window.
    ApplicationsScrolled(iced::widget::scrollable::Viewport),
    /// The App-history table viewport moved. Kept separate because the two
    /// tables have different row-height contracts and scroll independently.
    AppHistoryScrolled(iced::widget::scrollable::Viewport),
    /// The Performance device rail reports its absolute scroll viewport.
    PerformanceRailScrolled(iced::widget::scrollable::Viewport),
    /// The Services inventory table viewport moved.
    ServicesScrolled(iced::widget::scrollable::Viewport),
    /// The Startup inventory table viewport moved.
    StartupScrolled(iced::widget::scrollable::Viewport),
    /// The Users/session inventory table viewport moved.
    UsersScrolled(iced::widget::scrollable::Viewport),
    /// A service row action was requested; opens the shared confirmation bar.
    RequestServiceAction { index: usize, action: ServiceAction },
    /// The pending service-control confirmation was confirmed.
    ConfirmServiceControl,
    /// Reveal the selected process's executable in the platform file manager.
    OpenProcessLocation,
    /// Open a web search for the selected process's name.
    SearchProcessOnline,
    /// Open the Iced-native CPU-affinity editor for the selected process.
    OpenProcessAffinity,
    /// Toggle one logical CPU in the local affinity editor.
    ToggleProcessAffinityCpu(u32),
    /// Select all logical CPUs in the local affinity editor.
    SelectAllProcessAffinity,
    /// Clear all logical CPUs in the local affinity editor.
    ClearAllProcessAffinity,
    /// Invert logical CPU selection in the local affinity editor.
    InvertProcessAffinity,
    /// Select Performance cores in the local affinity editor.
    SelectProcessAffinityPCores,
    /// Select Efficient cores in the local affinity editor.
    SelectProcessAffinityECores,
    /// Apply the affinity mask to the frozen process identity.
    ApplyProcessAffinity,
    /// Expand all subtree nodes in Process Tree view.
    ExpandAllProcessTree,
    /// Collapse all subtree nodes in Process Tree view.
    CollapseAllProcessTree,
    /// Jump to the process in Applications view and highlight it.
    JumpToProcess { pid: u32 },
    /// Copy text to system clipboard with a status label.
    CopyTextToClipboard { label: String, text: String },
    /// The process-properties environment table's key filter changed.
    EnvironmentFilterChanged(String),
    /// Open the startup entry executable or desktop file in file manager.
    OpenStartupLocation { index: usize },
    /// Switch performance graph history resolution data points.
    SelectPerformanceGraphPoints(u32),
    /// Accept the Insights network escalation pill: request the system-wide
    /// per-process network capture escalation through the shared effect.
    RequestProcessNetworkEscalation,
    /// Switch the process-details modal's section tab (Overview /
    /// Performance / Command / Insights).
    SelectDetailsSection(DetailsSection),
    /// Open the frontend-local settings modal.
    OpenSettings,
    /// Close the frontend-local settings modal.
    CloseSettings,
    /// One settings control changed (persisted + applied to the theme).
    SettingsChanged(SettingsChange),
    /// One observed OS color-scheme change (iced `system::theme_changes`
    /// subscription or the boot `system::theme` query). Reduced by
    /// `app::appearance` ahead of the domain router: only a `System`
    /// color-mode preference follows it — an explicit user choice is never
    /// overridden.
    SystemThemeChanged(iced::theme::Mode),
    /// Open the frontend-local about/system-information modal.
    OpenAbout,
    /// Open the frontend-local system-health modal.
    OpenHealth,
    /// Open the frontend-local containers modal.
    OpenContainers,
    /// Toggle the directory-usage scan lifecycle for the selected Disk
    /// device (G-13): an idle/terminal slot starts a bounded scan of the
    /// disk's first mounted partition (or `/`); an active scan of that disk
    /// cancels. Queued through the shell's typed
    /// [`ShellApp::request_directory_usage`] effect lane.
    ToggleDirectoryUsageScan,
    /// Open the SMART detail dialog for one observed disk.
    OpenDiskSmart { index: usize },
    /// Toggle the per-engine GPU utilization session on the GPU device panel
    /// (the typed `telemetry.gpu.engines` lane): enable submits ONE bounded
    /// request for the first GPU (the user-initiated escalation entry), the
    /// tick re-requests on a bounded cadence while the GPU device is visible.
    ToggleGpuEngines,
    /// Copy the About modal's system-information lines to the clipboard
    /// (G-16, GPUI about-parity). Served by the iced clipboard task; the
    /// footer feedback mirrors the export line's lifecycle.
    CopyAboutDetails,
    /// Export the current snapshot into the working directory.
    ExportSnapshot,
    /// Apply a saved process view preset.
    ApplySavedView(u64),
    /// Save current Applications view configuration as a custom preset.
    SaveCurrentProcessView,
    /// Export user saved views to JSON on clipboard.
    ExportSavedViews,
    /// Import user saved views from JSON on clipboard.
    ImportSavedViews,
    /// Delete a user-saved view preset.
    DeleteSavedView(u64),
    /// Toggle performance history replay panel.
    ToggleHistoryReplay,
    /// Select performance history replay window.
    SelectHistoryReplayWindow(taskmanager_core::core::history::HistoryWindow),
    /// Refresh history replay query.
    RefreshHistoryReplay,
    /// Open the Alert Center modal.
    OpenAlertCenter,
    /// Close the Alert Center modal.
    CloseAlertCenter,
    /// Clear alert event history.
    ClearAlertEvents,
    /// Export alert events.
    ExportAlertEvents,
    /// Frontend-local Alerts page message (route open/close + rule toggle).
    Alerts(AlertsMessage),
    /// Frontend-local first-run dialog intents (GPUI first-run parity). The
    /// dialog's state machine and renderer live in `ui::first_run`; the
    /// typed intents feed the surface wiring that owns the observation and
    /// setup-script submission lane.
    FirstRun(FirstRunMessage),
    /// Frontend-local System-page dashboard segment message (history-window
    /// selection; the segment renderer lives in `ui::system_dashboard`).
    SystemDashboard(SystemDashboardMessage),
    /// Copy selected process as TSV row.
    CopyProcessTsv,
    /// Copy selected process as JSON object.
    CopyProcessJson,
    /// Generate and copy redacted system diagnostics report.
    GenerateDiagnosticsReport,
    /// Open the Run New Task modal.
    OpenRunTask,
    /// Close the Run New Task modal.
    CloseRunTask,
    /// Update the Run New Task command string.
    UpdateRunTaskCommand(String),
    /// Toggle the administrative privileges checkbox for Run New Task.
    ToggleRunTaskAdmin,
    /// Submit the Run New Task action.
    SubmitRunTask,
}

pub struct IcedApp {
    pub shell: ShellApp,
    /// Immutable native local-time rules injected by the composition root.
    /// Renderer code never discovers host files or environment state.
    pub(crate) local_time_rules: taskmanager_core::core::time::LocalTimeRulesObservation,
    /// Sole owner of the platform client, singleton/tray handles and runtime
    /// cadence. View state cannot dynamically borrow or clone these resources.
    pub(crate) runtime: runtime::IcedRuntime,
    /// Capture-only marker state; absent for ordinary production sessions.
    capture: capture_state::CaptureState,
    /// Focus, modifiers, context menu and short-lived renderer motion.
    pub(crate) input: input_state::InputState,
    /// Coordinator cursor, canonical draft, applied revision, language,
    /// presentation projection and resolved theme, updated through one bridge.
    configuration: configuration_state::IcedConfiguration,
    /// Run New Task state.
    pub(crate) run_task: crate::ui::overlays::run_task::RunTaskState,
    /// User process view presets.
    pub(crate) saved_views: Vec<crate::saved_views::SavedViewPreset>,
    pub(crate) next_saved_view_id: u64,
    pub(crate) saved_view_feedback: Option<crate::saved_views::SavedViewTransferFeedback>,
    /// Frontend-local Alerts-page route (an Iced-local
    /// route outside the shared `AppPage` set, GPUI Containers-page style).
    pub(crate) alerts_page: alerts::AlertsPageState,
    /// Frontend-local first-run dialog state (GPUI first-run parity). The
    /// state machine and renderer live in `ui::first_run`; the composition
    /// lane in `app::update::first_run` folds correlated platform answers
    /// into it and drives the `LocalSurface::FirstRun` slot.
    pub(crate) first_run: crate::ui::first_run::FirstRunUiState,
    /// Pending first-run setup-script submissions, correlated by request id
    /// (the drained batch's answers and typed failures consume from here).
    pub(crate) first_run_requests:
        std::collections::HashMap<taskmanager_platform_contract::RequestId, SetupScriptAction>,
    /// Frontend-local System-page dashboard window selection. The dashboard
    /// segment renderer lives in `ui::system_dashboard`; the pills publish
    /// `Message::SystemDashboard(SelectWindow)` which stores here.
    pub(crate) system_dashboard_window: taskmanager_core::core::history::HistoryWindow,
    /// Boot-resolved replay capability plus its application-correlated panel
    /// lifecycle. Runtime config publications cannot change the capability.
    history_runtime: history_replay::IcedHistoryRuntime,
    /// Application-correlated export lifecycle with the app-host's named
    /// worker client; unavailable demo/test instances perform no file I/O.
    snapshot_export: snapshot_export::IcedSnapshotExportRuntime,
    /// The sole Iced-owned primary surface. Shared confirmations and Process
    /// Properties remain in `application.interaction`; input ownership is
    /// derived across both machines by `InputScope`.
    local_surface: LocalSurfaceState,
    /// Applications and process-details presentation state. Canonical process
    /// facts and request lifecycles remain in the shell/application track.
    pub(crate) process_presentation: process_presentation_state::ProcessPresentationState,
    /// Session-local Applications column sizing: header-drag width overrides
    /// plus the transient drag session while an edge is held. Width semantics
    /// stay contract truth (`ui::applications`); this is override storage only.
    pub(crate) process_column_sizing: update::columns::ProcessColumnSizing,
    /// Named client of the app-host's process-wide diagnostic writer.
    service_log_export: service_log::IcedServiceLogExportRuntime,
    /// Renderer-local service-details data. Its open target is carried by the
    /// `LocalSurface::ServiceDetails` payload.
    pub(crate) service_details: service_details::ServiceDetailsState,
    /// Iced-only Performance selection, visibility and bounded process chart
    /// state. Shared device facts and history remain shell-owned.
    pub(crate) performance: performance_state::PerformanceState,
    /// Per-window event-time cache. Only the tick boundary may update it;
    /// renderers consume an immutable timestamp.
    window_time: window_time::WindowTimeCache,
    /// Window size and six independent virtual-scroll identities/offsets.
    viewport: viewport_state::IcedViewportState,
    /// Renderer-only projection caches. Each cache owns a narrow typed
    /// fingerprint boundary; viewport/scroll state remains outside because it
    /// has an independent interaction lifetime.
    projection_caches: IcedProjectionCaches,
}

impl Default for IcedApp {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
#[path = "../tests/gui/app/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/gui/app/tests_parity.rs"]
mod tests_parity;

#[cfg(test)]
#[path = "../tests/gui/app/tests/inventory_menus.rs"]
mod inventory_menus;

#[cfg(test)]
#[path = "../tests/gui/app/quit_tests.rs"]
mod quit_tests;
