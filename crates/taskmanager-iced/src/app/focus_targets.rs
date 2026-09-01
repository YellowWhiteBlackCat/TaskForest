//! Renderer-local focus targets for the Iced adapter, extracted from
//! [`super`] so the state module stays under the repository's source-size
//! budget. The stable operation IDs (`iced-…`) are the single identity each
//! focusable widget registers with Iced's focus traversal.

use taskmanager_application::{AppPage, RefreshRequest};
use taskmanager_core::core::services::ServiceAction;

use taskmanager_shell::SortCol;

use super::DetailsSection;
use super::selectors::PerfDevice;
use taskmanager_shell::ProcessStatusFilter;

/// Focus targets that remain local to the Iced adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// The current modal close action.
    ModalClose,
    /// A row in one of the renderer-local typed tables.
    TableRow {
        page: AppPage,
        index: usize,
    },
    /// One of the seven shared page tabs.
    PageTab(AppPage),
    /// The Applications search trigger.
    SearchTrigger,
    /// The active Applications search close action.
    SearchClose,
    /// Open the Applications column-visibility menu.
    ProcessColumnsTrigger,
    /// One Applications column-visibility menu item.
    ProcessColumnToggle(SortCol),
    /// One column-menu width stepper (narrow) — the keyboard-accessible
    /// sizing path alongside the header-drag edge.
    ProcessColumnNarrow(SortCol),
    /// One column-menu width stepper (widen) — the keyboard-accessible
    /// sizing path alongside the header-drag edge.
    ProcessColumnWiden(SortCol),
    /// Dismiss the Applications column-visibility menu.
    ProcessColumnsClose,
    /// The Services-page filter input.
    ServicesSearch,
    /// A page-scoped retry action for a degraded inventory source.
    SourceRetry(RefreshRequest),
    /// The Applications end-task trigger.
    EndTask,
    /// The Applications open-file-location action (routed via platform port).
    OpenProcessLocation,
    /// The Applications search-online action (routed via platform port).
    SearchProcessOnline,
    /// Open the selected process's CPU-affinity editor.
    ProcessAffinityOpen,
    /// Select all logical CPUs in the affinity editor.
    ProcessAffinitySelectAll,
    /// Clear all logical CPUs in the affinity editor.
    ProcessAffinityClearAll,
    /// Invert CPU selection in the affinity editor.
    ProcessAffinityInvert,
    /// Select Performance cores in the affinity editor.
    ProcessAffinityPCores,
    /// Select Efficient cores in the affinity editor.
    ProcessAffinityECores,
    /// One logical CPU toggle in the affinity editor.
    ProcessAffinityCpu(u32),
    /// Apply the affinity editor's frozen-target mask.
    ProcessAffinityApply,
    /// Expand all subtree nodes in Process Tree view.
    ProcessTreeExpandAll,
    /// Collapse all subtree nodes in Process Tree view.
    ProcessTreeCollapseAll,
    /// Jump to process in service details modal.
    ServiceDetailsJumpToProcess,
    /// Open the startup entry executable or desktop file in file manager.
    StartupOpenLocation,
    /// Switch performance graph history resolution data points.
    PerformanceGraphPoints(u32),
    /// The Insights network-escalation acceptance pill.
    ProcessNetworkEscalation,
    /// One process-details modal section tab.
    DetailsTab(DetailsSection),
    /// The Applications Suspend-process action (shell batch path).
    SuspendProcess,
    /// The Applications Resume-process action (shell batch path).
    ResumeProcess,
    /// The Applications Kill-process action (destructive; opens the batch
    /// confirmation bar via the shell batch path).
    KillProcess,
    /// Process-row context-menu actions (GPUI vocabulary, Iced focus stops).
    ProcessMenuEndTask,
    ProcessMenuEndTree,
    ProcessMenuKill,
    ProcessMenuSuspend,
    ProcessMenuResume,
    ProcessMenuSignalHangup,
    ProcessMenuSignalInterrupt,
    ProcessMenuSignalUser1,
    ProcessMenuSignalUser2,
    ProcessMenuOpenLocation,
    ProcessMenuSearchOnline,
    ProcessMenuProperties,
    ProcessMenuCopyName,
    ProcessMenuCopyPid,
    ProcessMenuCopyCommandLine,
    ProcessMenuClose,
    /// The pending end-task confirmation action.
    ConfirmEndTask,
    /// The pending destructive batch (Kill) confirmation action.
    ConfirmProcessBatch,
    /// The pending end-task cancellation action.
    CancelEndTask,
    /// The Users session disconnect action.
    SessionDisconnect,
    /// The Users session lock action.
    SessionLock,
    /// The Users row-context-menu disconnect entry.
    UserRowMenuDisconnect,
    /// The Users row-context-menu lock entry.
    UserRowMenuLock,
    /// The Users row-context-menu close entry (dismisses the menu).
    UserRowMenuClose,
    /// The toolbar settings trigger.
    SettingsTrigger,
    /// The toolbar containers trigger.
    ContainersTrigger,
    /// The toolbar health trigger.
    HealthTrigger,
    /// The toolbar about trigger.
    AboutTrigger,
    /// The toolbar export trigger.
    Export,
    /// One service row lifecycle action (Start/Stop/Restart).
    ServiceAction {
        index: usize,
        action: ServiceAction,
    },
    /// One Services-row context-menu action.
    ServiceMenuAction {
        index: usize,
        action: ServiceAction,
    },
    /// Dismiss the Services-row context menu.
    ServiceMenuClose,
    /// Open the selected service's details/log modal.
    ServiceLogOpen {
        index: usize,
    },
    /// Open the selected service's dependency/lifecycle details modal.
    ServiceDetailsOpen {
        index: usize,
    },
    /// The pending service-control confirmation action.
    ConfirmServiceControl,
    /// The pending service-control cancellation action.
    CancelServiceControl,
    /// The startup-entry enable/disable toggle on the Startup page.
    StartupControl,
    /// One Startup-row context-menu enable/disable action.
    StartupMenuAction {
        index: usize,
        enabled: bool,
    },
    /// Dismiss the Startup-row context menu.
    StartupMenuClose,
    /// The confirm button on the gated startup-control confirmation bar.
    ConfirmStartupControl,
    /// One settings chooser pill; `section` is a stable row name and `index`
    /// the choice position inside it.
    SettingsChoice {
        section: &'static str,
        index: u8,
    },
    /// One Performance-page resource tab (the select-a-device selector).
    PerfDeviceTab(PerfDevice),
    /// One Applications process-state filter pill.
    ProcessStatusFilterTab(ProcessStatusFilter),
    /// The directory-usage scan trigger on the Disk device panel (G-13).
    DirectoryUsageScan,
    /// Open the selected disk's SMART detail dialog.
    DiskSmartOpen {
        index: usize,
    },
    /// The conditional cancel action for a running directory-usage scan
    /// (rendered only while the selected disk's scan is Scanning).
    DirectoryUsageCancel,
    /// The About modal's copy-details clipboard action (G-16).
    AboutCopyDetails,
    /// The per-engine GPU utilization session toggle on the GPU device panel
    /// (the typed `telemetry.gpu.engines` lane).
    GpuEngineRowsToggle,
    /// Service-log modal controls.
    ServiceLogFollow,
    ServiceLogPause,
    ServiceLogLevel,
    ServiceLogTime,
    ServiceLogCopy,
    ServiceLogExport,
    /// Retry the dependency query in the service-details modal.
    ServiceDetailsRetry,
    /// Service-details modal merged log-panel controls.
    ServiceDetailsLogPause,
    ServiceDetailsLogLevel,
    ServiceDetailsLogTime,
    ServiceDetailsLogCopy,
    ServiceDetailsLogRefresh,
    /// Saved views preset buttons and actions.
    SavedViewPreset(u64),
    SavedViewSaveCurrent,
    SavedViewExport,
    SavedViewImport,
    /// History replay controls.
    HistoryReplayToggle,
    HistoryReplayWindow(taskmanager_core::core::history::HistoryWindow),
    HistoryReplayRefresh,
    /// Alert center modal controls.
    AlertCenterClear,
    AlertCenterExport,
    /// Additional context menu actions.
    ProcessMenuCopyTsv,
    ProcessMenuCopyJson,
    /// Run new task modal controls.
    RunTaskOpen,
    RunTaskCommandInput,
    RunTaskSubmit,
    RunTaskCancel,
    /// The frontend-local Alerts page tab (the eighth nav-strip pill; the
    /// route lives outside the shared `AppPage` set).
    AlertsPageTab,
    /// One managed alert-rule row toggle on the Alerts page (render-order
    /// focus position only; edits carry the canonical stable rule id).
    AlertsRuleToggle(usize),
    /// One first-run dialog descriptor copy stop (location / run command /
    /// revert command rows, indices 0..=2).
    FirstRunCopy(u8),
    /// One first-run dialog action pill (wiki / view / run / revert /
    /// restart / retry, indices 0..=5).
    FirstRunAction(u8),
}

impl FocusTarget {
    /// Every focus target that can be registered by the Iced adapter.
    pub const ALL: [Self; 142] = [
        Self::ModalClose,
        Self::PageTab(AppPage::Performance),
        Self::PageTab(AppPage::Applications),
        Self::PageTab(AppPage::Services),
        Self::PageTab(AppPage::System),
        Self::PageTab(AppPage::Startup),
        Self::PageTab(AppPage::Users),
        Self::SearchTrigger,
        Self::SearchClose,
        Self::ProcessColumnsTrigger,
        Self::ProcessColumnToggle(SortCol::Pid),
        Self::ProcessColumnNarrow(SortCol::Pid),
        Self::ProcessColumnWiden(SortCol::Pid),
        Self::ProcessColumnsClose,
        Self::ServicesSearch,
        Self::SourceRetry(RefreshRequest::Services),
        Self::SourceRetry(RefreshRequest::Startup),
        Self::SourceRetry(RefreshRequest::Sessions),
        Self::EndTask,
        Self::OpenProcessLocation,
        Self::SearchProcessOnline,
        Self::ProcessAffinityOpen,
        Self::ProcessAffinityCpu(0),
        Self::ProcessAffinityApply,
        Self::ProcessNetworkEscalation,
        Self::DetailsTab(DetailsSection::Overview),
        Self::DetailsTab(DetailsSection::Performance),
        Self::DetailsTab(DetailsSection::Command),
        Self::DetailsTab(DetailsSection::Insights),
        Self::SuspendProcess,
        Self::ResumeProcess,
        Self::KillProcess,
        Self::ProcessMenuEndTask,
        Self::ProcessMenuEndTree,
        Self::ProcessMenuKill,
        Self::ProcessMenuSuspend,
        Self::ProcessMenuResume,
        Self::ProcessMenuSignalHangup,
        Self::ProcessMenuSignalInterrupt,
        Self::ProcessMenuSignalUser1,
        Self::ProcessMenuSignalUser2,
        Self::ProcessMenuOpenLocation,
        Self::ProcessMenuSearchOnline,
        Self::ProcessMenuProperties,
        Self::ProcessMenuCopyName,
        Self::ProcessMenuCopyPid,
        Self::ProcessMenuCopyCommandLine,
        Self::ProcessMenuClose,
        Self::ConfirmEndTask,
        Self::ConfirmProcessBatch,
        Self::CancelEndTask,
        Self::SessionDisconnect,
        Self::SessionLock,
        Self::UserRowMenuDisconnect,
        Self::UserRowMenuLock,
        Self::UserRowMenuClose,
        Self::SettingsTrigger,
        Self::ContainersTrigger,
        Self::HealthTrigger,
        Self::AboutTrigger,
        Self::Export,
        Self::ServiceAction {
            index: 0,
            action: ServiceAction::Start,
        },
        Self::ServiceMenuAction {
            index: 0,
            action: ServiceAction::Start,
        },
        Self::ServiceMenuClose,
        Self::ServiceLogOpen { index: 0 },
        Self::ServiceDetailsOpen { index: 0 },
        Self::ConfirmServiceControl,
        Self::CancelServiceControl,
        Self::StartupControl,
        Self::StartupMenuAction {
            index: 0,
            enabled: true,
        },
        Self::StartupMenuAction {
            index: 0,
            enabled: false,
        },
        Self::StartupMenuClose,
        Self::ConfirmStartupControl,
        Self::SettingsChoice {
            section: "skin",
            index: 0,
        },
        Self::PerfDeviceTab(PerfDevice::Cpu),
        Self::PerfDeviceTab(PerfDevice::Memory),
        Self::PerfDeviceTab(PerfDevice::Disk(0)),
        Self::PerfDeviceTab(PerfDevice::Network(0)),
        Self::PerfDeviceTab(PerfDevice::Gpu(0)),
        Self::PerfDeviceTab(PerfDevice::Battery(0)),
        Self::PerfDeviceTab(PerfDevice::Fan(0)),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::All),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::Running),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::Sleeping),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::Stopped),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::Zombie),
        Self::ProcessStatusFilterTab(ProcessStatusFilter::Other),
        Self::DirectoryUsageScan,
        Self::DiskSmartOpen { index: 0 },
        Self::DirectoryUsageCancel,
        Self::AboutCopyDetails,
        Self::GpuEngineRowsToggle,
        Self::ServiceLogFollow,
        Self::ServiceLogPause,
        Self::ServiceLogLevel,
        Self::ServiceLogTime,
        Self::ServiceLogCopy,
        Self::ServiceLogExport,
        Self::ServiceDetailsRetry,
        Self::ServiceDetailsLogPause,
        Self::ServiceDetailsLogLevel,
        Self::ServiceDetailsLogTime,
        Self::ServiceDetailsLogCopy,
        Self::ServiceDetailsLogRefresh,
        Self::SavedViewPreset(1),
        Self::SavedViewSaveCurrent,
        Self::SavedViewExport,
        Self::SavedViewImport,
        Self::HistoryReplayToggle,
        Self::HistoryReplayWindow(taskmanager_core::core::history::HistoryWindow::OneHour),
        Self::HistoryReplayRefresh,
        Self::AlertCenterClear,
        Self::AlertCenterExport,
        Self::ProcessMenuCopyTsv,
        Self::ProcessMenuCopyJson,
        Self::RunTaskOpen,
        Self::RunTaskCommandInput,
        Self::RunTaskSubmit,
        Self::RunTaskCancel,
        Self::AlertsPageTab,
        Self::AlertsRuleToggle(0),
        Self::FirstRunCopy(0),
        Self::FirstRunCopy(1),
        Self::FirstRunCopy(2),
        Self::FirstRunAction(0),
        Self::FirstRunAction(1),
        Self::FirstRunAction(2),
        Self::FirstRunAction(3),
        Self::FirstRunAction(4),
        Self::FirstRunAction(5),
        Self::ProcessAffinitySelectAll,
        Self::ProcessAffinityClearAll,
        Self::ProcessAffinityInvert,
        Self::ProcessAffinityPCores,
        Self::ProcessAffinityECores,
        Self::ProcessTreeExpandAll,
        Self::ProcessTreeCollapseAll,
        Self::ServiceDetailsJumpToProcess,
        Self::StartupOpenLocation,
        Self::PerformanceGraphPoints(60),
        Self::PerformanceGraphPoints(120),
        Self::PerformanceGraphPoints(300),
    ];
}
