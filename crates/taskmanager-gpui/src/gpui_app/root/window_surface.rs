//! Single-owner window-surface state for the GPUI frontend.
//!
//! Widget entities, scroll handles and in-flight workflow state remain on
//! [`RootView`](super::RootView). Visibility and the payload that identifies an
//! interactive modal live only here, so rendering, keyboard routing, focus and
//! accessibility cannot infer different active dialogs from parallel booleans.

use crate::gpui_app::dashboard::DashboardPanel;
use crate::gpui_app::root::diagnostic_bundle::DiagnosticBundleUiState;
use taskmanager_application::{
    ConfirmationKind, InteractionEvent, InteractionReduction, PendingConfirmation, PlatformEffect,
    ProcessTerminationConfirmation, ServiceControlTarget, SurfaceDismissReason, SurfaceKind,
    SurfaceTransition,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessBatchIntent};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::target::ServiceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowSurfaceKind {
    Settings,
    Help,
    SystemAbout,
    About,
    FirstRun,
    RunTask,
    DiagnosticBundle,
    ServiceDetails,
    DiskSmart,
    DashboardPanel,
    ProcessAffinity,
}

impl WindowSurfaceKind {
    pub const ALL: [Self; 11] = [
        Self::Settings,
        Self::Help,
        Self::SystemAbout,
        Self::About,
        Self::FirstRun,
        Self::RunTask,
        Self::DiagnosticBundle,
        Self::ServiceDetails,
        Self::DiskSmart,
        Self::DashboardPanel,
        Self::ProcessAffinity,
    ];
}

#[derive(Clone, Debug)]
pub(crate) enum WindowSurface {
    Settings,
    Help,
    SystemAbout,
    About,
    FirstRun,
    RunTask,
    DiagnosticBundle(DiagnosticBundleUiState),
    ServiceDetails(ServiceId),
    DiskSmart(usize),
    DashboardPanel(DashboardPanel),
    ProcessAffinity(u32),
}

impl WindowSurface {
    #[must_use]
    pub const fn kind(&self) -> WindowSurfaceKind {
        match self {
            Self::Settings => WindowSurfaceKind::Settings,
            Self::Help => WindowSurfaceKind::Help,
            Self::SystemAbout => WindowSurfaceKind::SystemAbout,
            Self::About => WindowSurfaceKind::About,
            Self::FirstRun => WindowSurfaceKind::FirstRun,
            Self::RunTask => WindowSurfaceKind::RunTask,
            Self::DiagnosticBundle(_) => WindowSurfaceKind::DiagnosticBundle,
            Self::ServiceDetails(_) => WindowSurfaceKind::ServiceDetails,
            Self::DiskSmart(_) => WindowSurfaceKind::DiskSmart,
            Self::DashboardPanel(_) => WindowSurfaceKind::DashboardPanel,
            Self::ProcessAffinity(_) => WindowSurfaceKind::ProcessAffinity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowSurfaceDismissReason {
    Cancel,
    Escape,
    CloseButton,
    Scrim,
    PageChanged,
    TargetUnavailable,
    Completed,
}

#[derive(Clone, Debug)]
pub(crate) enum WindowSurfaceEvent {
    Open(WindowSurface),
    DismissCurrent(WindowSurfaceDismissReason),
    Dismiss {
        expected: WindowSurfaceKind,
        reason: WindowSurfaceDismissReason,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowSurfaceTransition {
    #[default]
    Unchanged,
    Opened(WindowSurfaceKind),
    Replaced {
        previous: WindowSurfaceKind,
        current: WindowSurfaceKind,
    },
    Dismissed {
        surface: WindowSurfaceKind,
        reason: WindowSurfaceDismissReason,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WindowSurfaceState {
    active: Option<WindowSurface>,
}

impl WindowSurfaceState {
    #[must_use]
    pub const fn initial(active: Option<WindowSurface>) -> Self {
        Self { active }
    }

    #[must_use]
    pub const fn active(&self) -> Option<&WindowSurface> {
        self.active.as_ref()
    }

    #[must_use]
    pub fn active_mut(&mut self) -> Option<&mut WindowSurface> {
        self.active.as_mut()
    }

    #[must_use]
    pub const fn kind(&self) -> Option<WindowSurfaceKind> {
        match self.active.as_ref() {
            Some(surface) => Some(surface.kind()),
            None => None,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.active.is_some()
    }

    pub fn reduce(&mut self, event: WindowSurfaceEvent) -> WindowSurfaceTransition {
        match event {
            WindowSurfaceEvent::Open(surface) => {
                let current = surface.kind();
                let previous = self.active.replace(surface).map(|value| value.kind());
                previous.map_or(WindowSurfaceTransition::Opened(current), |previous| {
                    WindowSurfaceTransition::Replaced { previous, current }
                })
            }
            WindowSurfaceEvent::DismissCurrent(reason) => {
                let Some(surface) = self.active.take() else {
                    return WindowSurfaceTransition::Unchanged;
                };
                WindowSurfaceTransition::Dismissed {
                    surface: surface.kind(),
                    reason,
                }
            }
            WindowSurfaceEvent::Dismiss { expected, reason } => {
                if self.kind() != Some(expected) {
                    return WindowSurfaceTransition::Unchanged;
                }
                let Some(surface) = self.active.take() else {
                    return WindowSurfaceTransition::Unchanged;
                };
                WindowSurfaceTransition::Dismissed {
                    surface: surface.kind(),
                    reason,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuiInputScope {
    TelemetryWarmup,
    Surface(GpuiSurfaceKind),
    Content,
}

/// The one semantic surface visible in a GPUI window. Shared surfaces are
/// owned by application `InteractionState`; local surfaces own renderer-only
/// widget/presentation payloads. Root orchestration makes opening either side
/// replace the other, so input and rendering never observe two modal owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuiSurfaceKind {
    Shared(SurfaceKind),
    Local(WindowSurfaceKind),
}

impl super::RootView {
    #[must_use]
    pub const fn window_surface_kind(&self) -> Option<WindowSurfaceKind> {
        self.window_surface.kind()
    }

    #[must_use]
    pub const fn window_surface_open(&self) -> bool {
        self.window_surface.is_open() || self.shell.interaction.is_open()
    }

    #[must_use]
    pub const fn active_surface_kind(&self) -> Option<GpuiSurfaceKind> {
        if let Some(kind) = self.shell.interaction.kind() {
            Some(GpuiSurfaceKind::Shared(kind))
        } else if let Some(kind) = self.window_surface.kind() {
            Some(GpuiSurfaceKind::Local(kind))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn input_scope(&self) -> GpuiInputScope {
        if self.telemetry_frame_state.is_collecting() {
            GpuiInputScope::TelemetryWarmup
        } else if let Some(kind) = self.active_surface_kind() {
            GpuiInputScope::Surface(kind)
        } else {
            GpuiInputScope::Content
        }
    }

    pub(crate) fn active_window_surface(&self) -> Option<&WindowSurface> {
        self.window_surface.active()
    }

    pub(crate) fn open_window_surface(
        &mut self,
        surface: WindowSurface,
    ) -> WindowSurfaceTransition {
        self.dismiss_shared_surface_if_present(SurfaceDismissReason::PageChanged);
        let current = surface.kind();
        let transition = self
            .window_surface
            .reduce(WindowSurfaceEvent::Open(surface));
        if let WindowSurfaceTransition::Replaced { previous, .. } = transition
            && previous != current
        {
            self.cleanup_closed_surface(previous);
        }
        self.hovered = None;
        transition
    }

    fn reduce_shared_interaction(&mut self, event: InteractionEvent) -> InteractionReduction {
        if matches!(
            &event,
            InteractionEvent::OpenProcessProperties(_) | InteractionEvent::ArmConfirmation(_)
        ) {
            self.dismiss_current_window_surface(WindowSurfaceDismissReason::PageChanged);
        }
        let reduction = self.shell.interaction.reduce(event);
        self.cleanup_shared_transition(reduction.transition);
        self.hovered = None;
        reduction
    }

    fn cleanup_shared_transition(&mut self, transition: SurfaceTransition) {
        match transition {
            SurfaceTransition::Replaced { previous, .. }
            | SurfaceTransition::Dismissed {
                surface: previous, ..
            } => self.cleanup_closed_shared_surface(previous),
            SurfaceTransition::Unchanged
            | SurfaceTransition::Opened(_)
            | SurfaceTransition::Confirmed(_) => {}
        }
    }

    fn cleanup_closed_shared_surface(&mut self, kind: SurfaceKind) {
        if kind == SurfaceKind::ProcessProperties {
            self.process_insights.clear();
            self.shell.close_network_escalation();
        }
    }

    fn dismiss_shared_surface_if_present(&mut self, reason: SurfaceDismissReason) {
        if self.shell.interaction.is_open() {
            let reduction = self
                .shell
                .interaction
                .reduce(InteractionEvent::Dismiss(reason));
            self.cleanup_shared_transition(reduction.transition);
        }
    }

    pub(crate) fn arm_confirmation(&mut self, pending: PendingConfirmation) -> SurfaceTransition {
        self.reduce_shared_interaction(InteractionEvent::ArmConfirmation(pending))
            .transition
    }

    pub(crate) fn confirm_confirmation(
        &mut self,
        expected: ConfirmationKind,
    ) -> Option<PlatformEffect> {
        self.reduce_shared_interaction(InteractionEvent::Confirm(expected))
            .effect
    }

    pub(crate) fn open_shared_process_properties(
        &mut self,
        target: FrozenProcessIdentity,
    ) -> SurfaceTransition {
        self.reduce_shared_interaction(InteractionEvent::OpenProcessProperties(target))
            .transition
    }

    pub(crate) fn dismiss_shared_surface(
        &mut self,
        expected: SurfaceKind,
        reason: SurfaceDismissReason,
    ) -> bool {
        if self.shell.interaction.kind() != Some(expected) {
            return false;
        }
        let reduction = self.reduce_shared_interaction(InteractionEvent::Dismiss(reason));
        !matches!(reduction.transition, SurfaceTransition::Unchanged)
    }

    pub(crate) fn dismiss_current_surface(&mut self, reason: WindowSurfaceDismissReason) -> bool {
        if self.shell.interaction.is_open() {
            self.dismiss_shared_surface_if_present(shared_dismiss_reason(reason));
            true
        } else {
            !matches!(
                self.dismiss_current_window_surface(reason),
                WindowSurfaceTransition::Unchanged
            )
        }
    }

    pub fn dismiss_window_surface(
        &mut self,
        expected: WindowSurfaceKind,
        reason: WindowSurfaceDismissReason,
    ) -> WindowSurfaceTransition {
        let transition = self
            .window_surface
            .reduce(WindowSurfaceEvent::Dismiss { expected, reason });
        if let WindowSurfaceTransition::Dismissed { surface, .. } = transition {
            self.cleanup_closed_surface(surface);
        }
        transition
    }

    pub fn dismiss_current_window_surface(
        &mut self,
        reason: WindowSurfaceDismissReason,
    ) -> WindowSurfaceTransition {
        let transition = self
            .window_surface
            .reduce(WindowSurfaceEvent::DismissCurrent(reason));
        if let WindowSurfaceTransition::Dismissed { surface, .. } = transition {
            self.cleanup_closed_surface(surface);
        }
        transition
    }

    fn cleanup_closed_surface(&mut self, kind: WindowSurfaceKind) {
        match kind {
            WindowSurfaceKind::RunTask => {
                self.run_error = None;
                self.close_run_command_session();
            }
            WindowSurfaceKind::ProcessAffinity => {
                self.shell.close_process_affinity();
                self.processes_state.affinity_editor.cpus.clear();
                self.processes_state.affinity_editor.hover = None;
            }
            WindowSurfaceKind::ServiceDetails => {
                self.shell.service_dependencies.close();
                self.service_details.close();
            }
            WindowSurfaceKind::Settings
            | WindowSurfaceKind::Help
            | WindowSurfaceKind::SystemAbout
            | WindowSurfaceKind::About
            | WindowSurfaceKind::FirstRun
            | WindowSurfaceKind::DiagnosticBundle
            | WindowSurfaceKind::DiskSmart
            | WindowSurfaceKind::DashboardPanel => {}
        }
    }

    #[must_use]
    pub fn settings_open(&self) -> bool {
        self.window_surface.kind() == Some(WindowSurfaceKind::Settings)
    }

    #[must_use]
    pub fn help_open(&self) -> bool {
        self.window_surface.kind() == Some(WindowSurfaceKind::Help)
    }

    #[must_use]
    pub fn first_run_open(&self) -> bool {
        self.window_surface.kind() == Some(WindowSurfaceKind::FirstRun)
    }

    #[must_use]
    pub fn process_properties_pid(&self) -> Option<u32> {
        self.shell
            .interaction
            .process_properties()
            .map(|target| target.pid)
    }

    #[must_use]
    pub const fn process_properties_target(&self) -> Option<&FrozenProcessIdentity> {
        self.shell.interaction.process_properties()
    }

    #[must_use]
    pub fn process_termination_confirmation(&self) -> Option<&ProcessTerminationConfirmation> {
        match self.shell.interaction.pending_confirmation() {
            Some(PendingConfirmation::ProcessTermination(intent)) => Some(intent),
            _ => None,
        }
    }

    #[must_use]
    pub fn service_control_confirmation(&self) -> Option<&ServiceControlTarget> {
        match self.shell.interaction.pending_confirmation() {
            Some(PendingConfirmation::ServiceControl(intent)) => Some(intent),
            _ => None,
        }
    }

    #[must_use]
    pub fn process_batch_confirmation(&self) -> Option<&ProcessBatchIntent> {
        match self.shell.interaction.pending_confirmation() {
            Some(PendingConfirmation::ProcessBatch(intent)) => Some(intent),
            _ => None,
        }
    }

    #[must_use]
    pub fn system_health_confirmation(&self) -> Option<&SmartSelfTestIntent> {
        match self.shell.interaction.pending_confirmation() {
            Some(PendingConfirmation::SmartSelfTest(request)) => Some(request),
            _ => None,
        }
    }

    #[must_use]
    pub const fn pending_confirmation(&self) -> Option<&PendingConfirmation> {
        self.shell.interaction.pending_confirmation()
    }

    #[must_use]
    pub fn diagnostic_bundle_state(&self) -> Option<&DiagnosticBundleUiState> {
        match self.window_surface.active() {
            Some(WindowSurface::DiagnosticBundle(state)) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn diagnostic_bundle_state_mut(&mut self) -> Option<&mut DiagnosticBundleUiState> {
        match self.window_surface.active_mut() {
            Some(WindowSurface::DiagnosticBundle(state)) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub fn service_details_target(&self) -> Option<&ServiceId> {
        match self.window_surface.active() {
            Some(WindowSurface::ServiceDetails(service_id)) => Some(service_id),
            _ => None,
        }
    }

    #[must_use]
    pub fn disk_smart_target(&self) -> Option<usize> {
        match self.window_surface.active() {
            Some(WindowSurface::DiskSmart(index)) => Some(*index),
            _ => None,
        }
    }

    #[must_use]
    pub fn dashboard_panel(&self) -> Option<DashboardPanel> {
        match self.window_surface.active() {
            Some(WindowSurface::DashboardPanel(panel)) => Some(*panel),
            _ => None,
        }
    }

    #[must_use]
    pub fn process_affinity_pid(&self) -> Option<u32> {
        match self.window_surface.active() {
            Some(WindowSurface::ProcessAffinity(pid)) => Some(*pid),
            _ => None,
        }
    }

    pub fn show_settings(&mut self) {
        self.open_window_surface(WindowSurface::Settings);
    }

    pub fn toggle_settings(&mut self) {
        if self.settings_open() {
            self.dismiss_window_surface(
                WindowSurfaceKind::Settings,
                WindowSurfaceDismissReason::Cancel,
            );
        } else {
            self.show_settings();
        }
    }

    pub fn toggle_help(&mut self) {
        if self.help_open() {
            self.dismiss_window_surface(
                WindowSurfaceKind::Help,
                WindowSurfaceDismissReason::Cancel,
            );
        } else {
            self.open_window_surface(WindowSurface::Help);
        }
    }

    pub fn show_system_about(&mut self) {
        self.open_window_surface(WindowSurface::SystemAbout);
    }

    pub fn show_about(&mut self) {
        self.open_window_surface(WindowSurface::About);
    }

    pub fn show_first_run(&mut self) {
        self.open_window_surface(WindowSurface::FirstRun);
    }

    pub fn show_run_task(&mut self) {
        self.open_window_surface(WindowSurface::RunTask);
    }

    pub fn show_disk_smart(&mut self, index: usize) {
        self.open_window_surface(WindowSurface::DiskSmart(index));
    }

    pub fn show_dashboard_panel(&mut self, panel: DashboardPanel) {
        self.open_window_surface(WindowSurface::DashboardPanel(panel));
    }

    pub fn show_process_affinity(&mut self, pid: u32) {
        self.open_window_surface(WindowSurface::ProcessAffinity(pid));
    }
}

const fn shared_dismiss_reason(reason: WindowSurfaceDismissReason) -> SurfaceDismissReason {
    match reason {
        WindowSurfaceDismissReason::Cancel => SurfaceDismissReason::Cancel,
        WindowSurfaceDismissReason::Escape => SurfaceDismissReason::Escape,
        WindowSurfaceDismissReason::CloseButton => SurfaceDismissReason::CloseButton,
        WindowSurfaceDismissReason::Scrim => SurfaceDismissReason::Scrim,
        WindowSurfaceDismissReason::PageChanged => SurfaceDismissReason::PageChanged,
        WindowSurfaceDismissReason::TargetUnavailable => SurfaceDismissReason::TargetUnavailable,
        WindowSurfaceDismissReason::Completed => SurfaceDismissReason::Completed,
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/root/window_surface_tests.rs"]
mod tests;
