//! Exhaustive one-domain routing and the typed reducer output envelope.

use iced::Task;
use taskmanager_application::PlatformEffect;

use super::super::surface::InteractionSnapshot;
use super::super::{IcedApp, Message};

pub(super) struct UpdateDispatch {
    pub(super) effect: Option<PlatformEffect>,
    pub(super) tasks: Vec<Task<Message>>,
}

impl UpdateDispatch {
    pub(super) fn none() -> Self {
        Self {
            effect: None,
            tasks: Vec::new(),
        }
    }

    pub(super) fn effect(effect: Option<PlatformEffect>) -> Self {
        Self {
            effect,
            tasks: Vec::new(),
        }
    }

    pub(super) fn task(task: Task<Message>) -> Self {
        Self {
            effect: None,
            tasks: vec![task],
        }
    }

    pub(super) fn with_task(mut self, task: Task<Message>) -> Self {
        self.tasks.push(task);
        self
    }
}

enum MessageDomain {
    Input(Message),
    Navigation(Message),
    Service(Message),
    Control(Message),
    Surface(Message),
    Performance(Message),
    FirstRun(Message),
    Transfer(Message),
    Alerts(Message),
    Window(Message),
    Columns(Message),
}

/// One exhaustive ownership table. Adding a top-level message fails to compile
/// until it is assigned to exactly one reducer domain.
fn route(message: Message) -> MessageDomain {
    match message {
        message @ (Message::Tick
        | Message::Key(_)
        | Message::ModifiersChanged(_)
        | Message::PointerPressed
        | Message::TextSelectionClaimed(_)
        | Message::SearchBackspace
        | Message::Focus(_)) => MessageDomain::Input(message),

        message @ (Message::SelectPage(_)
        | Message::SelectPerfDevice(_)
        | Message::SelectProcessStatusFilter(_)
        | Message::ToggleGroupExpansion { .. }
        | Message::ActivateTreeNode { .. }
        | Message::ExpandAllProcessTree
        | Message::CollapseAllProcessTree
        | Message::JumpToProcess { .. }
        | Message::OpenProcessRowMenu { .. }
        | Message::CloseProcessRowMenu
        | Message::ProcessMenuAction(_)
        | Message::OpenProcessColumnsMenu
        | Message::CloseProcessColumnsMenu
        | Message::ToggleProcessColumn(_)
        | Message::ResetProcessColumns
        | Message::EnvironmentFilterChanged(_)
        | Message::SelectRow(_)
        | Message::SortBy(_)
        | Message::SortInfoTable { .. }
        | Message::SearchChanged(_)
        | Message::FocusSearch
        | Message::CloseSearch
        | Message::ServicesSearchChanged(_)
        | Message::SelectDetailsSection(_)) => MessageDomain::Navigation(message),

        // Column-sizing messages are frontend-local process-table state
        // (drag overrides, no shell effect), reduced by `update::columns`.
        message @ (Message::BeginProcessColumnDrag { .. }
        | Message::ProcessColumnDragMoved(_)
        | Message::ProcessColumnDragReleased
        | Message::ResizeProcessColumn { .. }) => MessageDomain::Columns(message),

        message @ (Message::OpenServiceRowMenu { .. }
        | Message::CloseServiceRowMenu
        | Message::OpenServiceLog
        | Message::OpenServiceLogFor { .. }
        | Message::OpenServiceDetailsFor { .. }
        | Message::RefreshServiceDetails
        | Message::ToggleServiceDetailsLogPaused
        | Message::CycleServiceDetailsLogLevel
        | Message::CycleServiceDetailsLogTime
        | Message::CopyServiceDetailsLog
        | Message::RefreshServiceDetailsLogs
        | Message::CloseServiceLog
        | Message::ToggleLogFollow
        | Message::ToggleLogPaused
        | Message::CycleLogLevel
        | Message::CycleLogTime
        | Message::CopyServiceLog
        | Message::ExportServiceLog
        | Message::RequestServiceAction { .. }
        | Message::ConfirmServiceControl) => MessageDomain::Service(message),

        message @ (Message::RefreshSource(_)
        | Message::RequestEndTask
        | Message::RequestProcessBatch(_)
        | Message::ConfirmEndTask
        | Message::ConfirmProcessBatch
        | Message::RequestSessionControl(_)
        | Message::RequestProcessNetworkEscalation
        | Message::OpenUserRowMenu(_)
        | Message::CloseUserRowMenu
        | Message::OpenStartupRowMenu { .. }
        | Message::CloseStartupRowMenu
        | Message::RequestStartupControl(_)
        | Message::RequestStartupControlFor { .. }
        | Message::ConfirmStartupControl
        | Message::RequestSmartSelfTest { .. }
        | Message::ConfirmSmartSelfTest
        | Message::OpenProcessLocation
        | Message::SearchProcessOnline) => MessageDomain::Control(message),

        message @ (Message::DismissOverlay
        | Message::OpenProcessAffinity
        | Message::ToggleProcessAffinityCpu(_)
        | Message::SelectAllProcessAffinity
        | Message::ClearAllProcessAffinity
        | Message::InvertProcessAffinity
        | Message::SelectProcessAffinityPCores
        | Message::SelectProcessAffinityECores
        | Message::ApplyProcessAffinity
        | Message::OpenSettings
        | Message::CloseSettings
        | Message::OpenAbout
        | Message::OpenHealth
        | Message::OpenContainers
        | Message::OpenDiskSmart { .. }
        | Message::OpenAlertCenter
        | Message::CloseAlertCenter
        | Message::OpenRunTask
        | Message::CloseRunTask
        | Message::UpdateRunTaskCommand(_)
        | Message::SubmitRunTask) => MessageDomain::Surface(message),

        message @ (Message::SelectPerformanceGraphPoints(_)
        | Message::SettingsChanged(_)
        // SystemThemeChanged is reduced by `app::appearance` in the run.rs
        // update closure before dispatch; this arm only satisfies exhaustive routing.
        | Message::SystemThemeChanged(_)
        // SystemDashboard(SelectWindow) is frontend-local state reduced in
        // `reduce_performance_message` (no shell effect).
        | Message::SystemDashboard(_)
        | Message::ToggleGpuEngines
        | Message::ToggleGpuEnginesExpanded
        | Message::ToggleDirectoryUsageScan
        | Message::ToggleHistoryReplay
        | Message::SelectHistoryReplayWindow(_)
        | Message::RefreshHistoryReplay) => MessageDomain::Performance(message),

        // The first-run dialog's typed intents are reduced by the dedicated
        // `update::first_run` composition lane (submission, correlation and
        // the surface-slot transitions around `ui::first_run`'s fold).
        message @ Message::FirstRun(_) => MessageDomain::FirstRun(message),

        message @ (Message::CopyTextToClipboard { .. }
        | Message::OpenStartupLocation { .. }
        | Message::CopyAboutDetails
        | Message::ExportSnapshot
        | Message::RequestCurrentWindowCapture
        | Message::ApplySavedView(_)
        | Message::SaveCurrentProcessView
        | Message::ExportSavedViews
        | Message::ImportSavedViews
        | Message::DeleteSavedView(_)
        | Message::CopyProcessTsv
        | Message::CopyProcessJson
        | Message::GenerateDiagnosticsReport) => MessageDomain::Transfer(message),

        message @ (Message::ClearAlertEvents | Message::ExportAlertEvents | Message::Alerts(_)) => {
            MessageDomain::Alerts(message)
        }

        message @ (Message::Frame(_)
        | Message::ApplicationsScrolled(_)
        | Message::AppHistoryScrolled(_)
        | Message::PerformanceRailScrolled(_)
        | Message::ServicesScrolled(_)
        | Message::StartupScrolled(_)
        | Message::UsersScrolled(_)
        | Message::WindowResized(_)
        | Message::WindowCloseRequested) => MessageDomain::Window(message),
    }
}

impl IcedApp {
    pub(super) fn dispatch_message(
        &mut self,
        message: Message,
        interaction_before: InteractionSnapshot,
    ) -> UpdateDispatch {
        match route(message) {
            MessageDomain::Input(message) => self.reduce_input_message(message, interaction_before),
            MessageDomain::Navigation(message) => self.reduce_navigation_message(message),
            MessageDomain::Service(message) => self.reduce_service_message(message),
            MessageDomain::Control(message) => self.reduce_control_message(message),
            MessageDomain::Surface(message) => {
                UpdateDispatch::effect(self.handle_surface_message(message))
            }
            MessageDomain::Performance(message) => self.reduce_performance_message(message),
            MessageDomain::FirstRun(message) => self.reduce_first_run_message(message),
            MessageDomain::Transfer(message) => self.reduce_transfer_message(message),
            MessageDomain::Alerts(message) => self.reduce_alerts_message(message),
            MessageDomain::Window(message) => self.reduce_window_message(message),
            MessageDomain::Columns(message) => self.reduce_process_column_message(message),
        }
    }
}
