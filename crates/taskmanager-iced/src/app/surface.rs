//! Typed renderer-local surface and input-ownership machines.
//!
//! Shared confirmations and Process Properties remain authoritative in
//! `taskmanager-application::InteractionState`. This module owns only Iced's
//! local primary surface and context menu. Both slots are private and mutate
//! through exhaustive events, so two local modals or two row menus cannot be
//! represented at the same time.

use taskforest_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskforest_core::core::services::ServiceItem;
use taskforest_core::core::session::SessionItem;
use taskforest_core::core::startup::StartupEntry;
use taskforest_core::core::target::ServiceId;
use taskmanager_application::ConfirmationKind;
use taskmanager_application::SurfaceKind;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_shell::presentation::system_information::SystemInformationGroup;
use taskmanager_ui_contract::{OverlayEvent, OverlayKindOf, OverlaySlot, OverlayTransition};

/// Stable identity of every Iced-owned primary surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LocalSurfaceKind {
    Settings,
    SidebarEditor,
    SavedViews,
    About,
    SystemInformation,
    Health,
    Containers,
    DiskSmart,
    ProcessAffinity,
    ServiceDetails,
    RunTask,
    AlertCenter,
    DiagnosticBundle,
    /// Optional setup review, opened explicitly from Settings when the
    /// application controller has an observed descriptor (ADR-040).
    FirstRun,
}

/// Payload carried by the one active Iced-owned primary surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocalSurface {
    Settings,
    SidebarEditor,
    SavedViews,
    About,
    SystemInformation(Vec<SystemInformationGroup>),
    Health,
    Containers,
    DiskSmart {
        index: usize,
    },
    ProcessAffinity {
        target: FrozenProcessIdentity,
    },
    ServiceDetails {
        service_id: ServiceId,
    },
    RunTask,
    AlertCenter,
    DiagnosticBundle(DiagnosticBundleUiState),
    /// Carries no payload: the dialog's state lives in
    /// [`taskmanager_application::first_run::FirstRunUiState`].
    FirstRun,
}

impl LocalSurface {
    pub(crate) fn kind(&self) -> LocalSurfaceKind {
        match self {
            Self::Settings => LocalSurfaceKind::Settings,
            Self::SidebarEditor => LocalSurfaceKind::SidebarEditor,
            Self::SavedViews => LocalSurfaceKind::SavedViews,
            Self::About => LocalSurfaceKind::About,
            Self::SystemInformation(_) => LocalSurfaceKind::SystemInformation,
            Self::Health => LocalSurfaceKind::Health,
            Self::Containers => LocalSurfaceKind::Containers,
            Self::DiskSmart { .. } => LocalSurfaceKind::DiskSmart,
            Self::ProcessAffinity { .. } => LocalSurfaceKind::ProcessAffinity,
            Self::ServiceDetails { .. } => LocalSurfaceKind::ServiceDetails,
            Self::RunTask => LocalSurfaceKind::RunTask,
            Self::AlertCenter => LocalSurfaceKind::AlertCenter,
            Self::DiagnosticBundle(_) => LocalSurfaceKind::DiagnosticBundle,
            Self::FirstRun => LocalSurfaceKind::FirstRun,
        }
    }
}

impl OverlayKindOf for LocalSurface {
    type Kind = LocalSurfaceKind;

    fn kind(&self) -> Self::Kind {
        LocalSurface::kind(self)
    }
}

pub(crate) type LocalSurfaceEvent = OverlayEvent<LocalSurface, LocalSurfaceKind, ()>;
pub(crate) type LocalSurfaceTransition = OverlayTransition<LocalSurfaceKind, ()>;

#[derive(Debug, Default)]
pub(crate) struct LocalSurfaceState(OverlaySlot<LocalSurface>);

impl std::ops::Deref for LocalSurfaceState {
    type Target = OverlaySlot<LocalSurface>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for LocalSurfaceState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl LocalSurfaceState {
    pub(super) fn diagnostic_bundle_mut(&mut self) -> Option<&mut DiagnosticBundleUiState> {
        match self.active_mut() {
            Some(LocalSurface::DiagnosticBundle(state)) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn reduce(&mut self, event: LocalSurfaceEvent) -> LocalSurfaceTransition {
        self.0.apply(event)
    }
}

/// Stable identity of every Iced-owned context menu branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ContextMenuKind {
    User,
    Process,
    Service,
    Startup,
    ProcessColumns,
    ProcessActions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContextMenu {
    User {
        visual_index: usize,
        session: SessionItem,
    },
    Process {
        identity: ProcessLiveKey,
    },
    Service {
        source_index: usize,
        service: Box<ServiceItem>,
    },
    Startup {
        source_index: usize,
        entry: StartupEntry,
    },
    ProcessColumns,
    /// The Applications action-bar overflow: the actions that do not fit as
    /// complete groups on the action strip.
    ProcessActions,
}

impl ContextMenu {
    pub(crate) fn kind(&self) -> ContextMenuKind {
        match self {
            Self::User { .. } => ContextMenuKind::User,
            Self::Process { .. } => ContextMenuKind::Process,
            Self::Service { .. } => ContextMenuKind::Service,
            Self::Startup { .. } => ContextMenuKind::Startup,
            Self::ProcessColumns => ContextMenuKind::ProcessColumns,
            Self::ProcessActions => ContextMenuKind::ProcessActions,
        }
    }
}

impl OverlayKindOf for ContextMenu {
    type Kind = ContextMenuKind;

    fn kind(&self) -> Self::Kind {
        ContextMenu::kind(self)
    }
}

pub(crate) type ContextMenuEvent = OverlayEvent<ContextMenu, ContextMenuKind, ()>;
pub(crate) type ContextMenuTransition = OverlayTransition<ContextMenuKind, ()>;

#[derive(Debug, Default)]
pub(crate) struct ContextMenuState(OverlaySlot<ContextMenu>);

impl std::ops::Deref for ContextMenuState {
    type Target = OverlaySlot<ContextMenu>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ContextMenuState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ContextMenuState {
    pub(crate) fn reduce(&mut self, event: ContextMenuEvent) -> ContextMenuTransition {
        self.0.apply(event)
    }
}

/// The single keyboard owner derived from semantic surface state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputScope {
    SharedSurface(SurfaceKind),
    LocalSurface(LocalSurfaceKind),
    ServiceLog,
    Help,
    Suggestions,
    ContextMenu(ContextMenuKind),
    Search,
    Content,
}

impl InputScope {
    pub(crate) fn modal_open(self) -> bool {
        matches!(
            self,
            Self::SharedSurface(_)
                | Self::LocalSurface(_)
                | Self::ServiceLog
                | Self::Help
                | Self::Suggestions
        )
    }

    pub(crate) fn opaque_modal_open(self) -> bool {
        match self {
            Self::SharedSurface(SurfaceKind::ProcessProperties) => true,
            Self::SharedSurface(SurfaceKind::Confirmation(kind)) => !matches!(
                kind,
                ConfirmationKind::EndTask
                    | ConfirmationKind::ServiceControl
                    | ConfirmationKind::SessionControl
                    | ConfirmationKind::SmartSelfTest
            ),
            Self::LocalSurface(_) | Self::ServiceLog | Self::Help | Self::Suggestions => true,
            Self::ContextMenu(_) | Self::Search | Self::Content => false,
        }
    }
}

/// Exhaustive before/after presence transition consumed by focus and motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PresenceTransition {
    StableClosed,
    Opened,
    StableOpen,
    Closed,
}

impl PresenceTransition {
    pub(crate) fn between(previous: bool, current: bool) -> Self {
        match (previous, current) {
            (false, false) => Self::StableClosed,
            (false, true) => Self::Opened,
            (true, true) => Self::StableOpen,
            (true, false) => Self::Closed,
        }
    }

    pub(crate) fn is_open(self) -> bool {
        matches!(self, Self::Opened | Self::StableOpen)
    }

    pub(crate) fn was_open(self) -> bool {
        matches!(self, Self::StableOpen | Self::Closed)
    }
}

/// One immutable interaction receipt used by focus and entrance-motion systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InteractionSnapshot {
    pub(crate) scope: InputScope,
}

impl InteractionSnapshot {
    pub(crate) fn modal_open(self) -> bool {
        self.scope.modal_open()
    }

    pub(crate) fn opaque_modal_open(self) -> bool {
        self.scope.opaque_modal_open()
    }

    pub(crate) fn process_properties_open(self) -> bool {
        matches!(
            self.scope,
            InputScope::SharedSurface(SurfaceKind::ProcessProperties)
        )
    }

    pub(crate) fn modal_transition(self, next: Self) -> PresenceTransition {
        PresenceTransition::between(self.modal_open(), next.modal_open())
    }

    pub(crate) fn process_properties_transition(self, next: Self) -> PresenceTransition {
        PresenceTransition::between(
            self.process_properties_open(),
            next.process_properties_open(),
        )
    }
}

use super::IcedApp;

impl IcedApp {
    pub(crate) fn local_surface(&self) -> Option<&LocalSurface> {
        OverlaySlot::active(&self.local_surface)
    }
    pub(crate) fn local_surface_kind(&self) -> Option<LocalSurfaceKind> {
        self.local_surface.kind()
    }

    pub(crate) fn context_menu(&self) -> Option<&ContextMenu> {
        OverlaySlot::active(&self.input.context_menu)
    }

    pub(crate) fn context_menu_kind(&self) -> Option<ContextMenuKind> {
        self.input.context_menu.kind()
    }

    pub(crate) fn run_task_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::RunTask))
    }

    pub(crate) fn affinity_target(&self) -> Option<&FrozenProcessIdentity> {
        match self.local_surface() {
            Some(LocalSurface::ProcessAffinity { target }) => Some(target),
            _ => None,
        }
    }

    pub(crate) fn affinity_open(&self) -> bool {
        self.affinity_target().is_some()
    }

    pub(crate) fn service_details_target(&self) -> Option<&ServiceId> {
        match self.local_surface() {
            Some(LocalSurface::ServiceDetails { service_id }) => Some(service_id),
            _ => None,
        }
    }

    pub(crate) fn user_menu_session(&self) -> Option<&SessionItem> {
        match self.context_menu() {
            Some(ContextMenu::User { session, .. }) => Some(session),
            _ => None,
        }
    }

    pub(crate) fn process_menu_identity(&self) -> Option<ProcessLiveKey> {
        match self.context_menu() {
            Some(ContextMenu::Process { identity }) => Some(*identity),
            _ => None,
        }
    }

    pub(crate) fn service_menu_index(&self) -> Option<usize> {
        match self.context_menu() {
            Some(ContextMenu::Service { source_index, .. }) => Some(*source_index),
            _ => None,
        }
    }

    pub(crate) fn service_menu_target(&self) -> Option<&ServiceItem> {
        match self.context_menu() {
            Some(ContextMenu::Service { service, .. }) => Some(service),
            _ => None,
        }
    }

    pub(crate) fn startup_menu_index(&self) -> Option<usize> {
        match self.context_menu() {
            Some(ContextMenu::Startup { source_index, .. }) => Some(*source_index),
            _ => None,
        }
    }

    pub(crate) fn startup_menu_entry(&self) -> Option<&StartupEntry> {
        match self.context_menu() {
            Some(ContextMenu::Startup { entry, .. }) => Some(entry),
            _ => None,
        }
    }

    pub(crate) fn process_columns_menu_open(&self) -> bool {
        matches!(self.context_menu(), Some(ContextMenu::ProcessColumns))
    }

    pub(crate) fn process_actions_menu_open(&self) -> bool {
        matches!(self.context_menu(), Some(ContextMenu::ProcessActions))
    }

    pub(super) fn open_local_surface(&mut self, surface: LocalSurface) {
        if self.local_surface_kind() == Some(LocalSurfaceKind::DiagnosticBundle) {
            self.diagnostics.close();
        }
        self.close_context_menus();
        self.close_shell_modals();
        if self.local_surface_kind() == Some(LocalSurfaceKind::ServiceDetails)
            && surface.kind() != LocalSurfaceKind::ServiceDetails
        {
            self.shell.service_dependencies.close();
            self.service_details.close();
        }
        if self.local_surface_kind() == Some(LocalSurfaceKind::ProcessAffinity) {
            self.process_presentation.affinity_cpus = None;
        }
        let _ = self.local_surface.reduce(LocalSurfaceEvent::Open(surface));
    }

    pub(super) fn dismiss_local_surface(&mut self) {
        let previous = self.local_surface_kind();
        if previous == Some(LocalSurfaceKind::DiagnosticBundle) {
            self.diagnostics.close();
        }
        let _ = self
            .local_surface
            .reduce(LocalSurfaceEvent::DismissCurrent(()));
        if previous == Some(LocalSurfaceKind::ServiceDetails) {
            self.shell.service_dependencies.close();
            self.service_details.close();
        }
        if previous == Some(LocalSurfaceKind::ProcessAffinity) {
            self.process_presentation.affinity_cpus = None;
        }
    }

    pub(super) fn dismiss_local_surface_kind(&mut self, expected: LocalSurfaceKind) {
        if expected == LocalSurfaceKind::DiagnosticBundle
            && self.local_surface_kind() == Some(expected)
        {
            self.diagnostics.close();
        }
        let transition = self
            .local_surface
            .reduce(LocalSurfaceEvent::Dismiss(expected, ()));
        if matches!(
            transition,
            LocalSurfaceTransition::Dismissed(LocalSurfaceKind::ProcessAffinity, ())
        ) {
            self.process_presentation.affinity_cpus = None;
        }
        if matches!(
            transition,
            LocalSurfaceTransition::Dismissed(LocalSurfaceKind::ServiceDetails, ())
        ) {
            self.shell.service_dependencies.close();
            self.service_details.close();
        }
    }

    pub(super) fn open_context_menu(&mut self, menu: ContextMenu) {
        self.dismiss_local_surface();
        self.close_shell_modals();
        let _ = self.input.context_menu.reduce(ContextMenuEvent::Open(menu));
    }

    pub(super) fn dismiss_context_menu(&mut self) {
        let _ = self
            .input
            .context_menu
            .reduce(ContextMenuEvent::DismissCurrent(()));
    }

    pub(super) fn dismiss_context_menu_kind(&mut self, expected: ContextMenuKind) {
        let _ = self
            .input
            .context_menu
            .reduce(ContextMenuEvent::Dismiss(expected, ()));
    }

    /// Derive the sole keyboard owner. The order is defensive; normal open
    /// transitions close competing primary surfaces before installing a new
    /// branch, and `assert_surface_invariants` checks that contract.
    pub(crate) fn input_scope(&self) -> InputScope {
        if let Some(surface) = self.shell.interaction_surface() {
            InputScope::SharedSurface(surface)
        } else if let Some(surface) = self.local_surface_kind() {
            InputScope::LocalSurface(surface)
        } else if self.shell.service_log.is_some() {
            InputScope::ServiceLog
        } else if self.shell.help_open() {
            InputScope::Help
        } else if self.shell.suggestions_open() {
            InputScope::Suggestions
        } else if let Some(menu) = self.context_menu_kind() {
            InputScope::ContextMenu(menu)
        } else if self.shell.search_active() {
            InputScope::Search
        } else {
            InputScope::Content
        }
    }

    pub(crate) fn interaction_snapshot(&self) -> InteractionSnapshot {
        InteractionSnapshot {
            scope: self.input_scope(),
        }
    }

    pub(crate) fn assert_surface_invariants(&self) {
        let primary_count = usize::from(self.shell.interaction_surface().is_some())
            + usize::from(self.local_surface_kind().is_some())
            + usize::from(self.shell.service_log.is_some())
            + usize::from(self.shell.help_open())
            + usize::from(self.shell.suggestions_open())
            + usize::from(self.context_menu_kind().is_some());
        debug_assert!(
            primary_count <= 1,
            "Iced input surfaces must be mutually exclusive"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/surface_state_tests.rs"]
mod tests;
