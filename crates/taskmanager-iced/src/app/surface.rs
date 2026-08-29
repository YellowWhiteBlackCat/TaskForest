//! Typed renderer-local surface and input-ownership machines.
//!
//! Shared confirmations and Process Properties remain authoritative in
//! `taskmanager-application::InteractionState`. This module owns only Iced's
//! local primary surface and context menu. Both slots are private and mutate
//! through exhaustive events, so two local modals or two row menus cannot be
//! represented at the same time.

use taskmanager_application::SurfaceKind;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::target::ServiceId;

/// Stable identity of every Iced-owned primary surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LocalSurfaceKind {
    Settings,
    About,
    Health,
    Containers,
    DiskSmart,
    ProcessAffinity,
    ServiceDetails,
    RunTask,
    AlertCenter,
    /// The optional-setup first-run dialog. Visibility is decided by the
    /// `ui::first_run` fold's transitions (the boot observation's answer),
    /// never opened directly by a user trigger.
    FirstRun,
}

/// Payload carried by the one active Iced-owned primary surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocalSurface {
    Settings,
    About,
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
    /// Carries no payload: the dialog's state lives in
    /// [`crate::ui::first_run::FirstRunUiState`].
    FirstRun,
}

impl LocalSurface {
    pub(crate) const fn kind(&self) -> LocalSurfaceKind {
        match self {
            Self::Settings => LocalSurfaceKind::Settings,
            Self::About => LocalSurfaceKind::About,
            Self::Health => LocalSurfaceKind::Health,
            Self::Containers => LocalSurfaceKind::Containers,
            Self::DiskSmart { .. } => LocalSurfaceKind::DiskSmart,
            Self::ProcessAffinity { .. } => LocalSurfaceKind::ProcessAffinity,
            Self::ServiceDetails { .. } => LocalSurfaceKind::ServiceDetails,
            Self::RunTask => LocalSurfaceKind::RunTask,
            Self::AlertCenter => LocalSurfaceKind::AlertCenter,
            Self::FirstRun => LocalSurfaceKind::FirstRun,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocalSurfaceEvent {
    Open(LocalSurface),
    DismissCurrent,
    Dismiss(LocalSurfaceKind),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LocalSurfaceTransition {
    #[default]
    Unchanged,
    Opened(LocalSurfaceKind),
    Replaced {
        previous: LocalSurfaceKind,
        current: LocalSurfaceKind,
    },
    Dismissed(LocalSurfaceKind),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalSurfaceState {
    active: Option<LocalSurface>,
}

impl LocalSurfaceState {
    pub(crate) const fn active(&self) -> Option<&LocalSurface> {
        self.active.as_ref()
    }

    pub(crate) const fn kind(&self) -> Option<LocalSurfaceKind> {
        match self.active.as_ref() {
            Some(surface) => Some(surface.kind()),
            None => None,
        }
    }

    pub(crate) fn reduce(&mut self, event: LocalSurfaceEvent) -> LocalSurfaceTransition {
        match event {
            LocalSurfaceEvent::Open(surface) => {
                let current = surface.kind();
                self.active.replace(surface).map_or(
                    LocalSurfaceTransition::Opened(current),
                    |previous| LocalSurfaceTransition::Replaced {
                        previous: previous.kind(),
                        current,
                    },
                )
            }
            LocalSurfaceEvent::Dismiss(expected) => {
                if self.kind() != Some(expected) {
                    return LocalSurfaceTransition::Unchanged;
                }
                self.dismiss_current()
            }
            LocalSurfaceEvent::DismissCurrent => self.dismiss_current(),
        }
    }

    fn dismiss_current(&mut self) -> LocalSurfaceTransition {
        self.active
            .take()
            .map_or(LocalSurfaceTransition::Unchanged, |surface| {
                LocalSurfaceTransition::Dismissed(surface.kind())
            })
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContextMenu {
    User {
        visual_index: usize,
        session: SessionItem,
    },
    Process {
        pid: u32,
    },
    Service {
        source_index: usize,
        service: ServiceItem,
    },
    Startup {
        source_index: usize,
        entry: StartupEntry,
    },
    ProcessColumns,
}

impl ContextMenu {
    pub(crate) const fn kind(&self) -> ContextMenuKind {
        match self {
            Self::User { .. } => ContextMenuKind::User,
            Self::Process { .. } => ContextMenuKind::Process,
            Self::Service { .. } => ContextMenuKind::Service,
            Self::Startup { .. } => ContextMenuKind::Startup,
            Self::ProcessColumns => ContextMenuKind::ProcessColumns,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContextMenuEvent {
    Open(Box<ContextMenu>),
    DismissCurrent,
    Dismiss(ContextMenuKind),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ContextMenuTransition {
    #[default]
    Unchanged,
    Opened(ContextMenuKind),
    Replaced {
        previous: ContextMenuKind,
        current: ContextMenuKind,
    },
    Dismissed(ContextMenuKind),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContextMenuState {
    active: Option<ContextMenu>,
}

impl ContextMenuState {
    pub(crate) const fn active(&self) -> Option<&ContextMenu> {
        self.active.as_ref()
    }

    pub(crate) const fn kind(&self) -> Option<ContextMenuKind> {
        match self.active.as_ref() {
            Some(menu) => Some(menu.kind()),
            None => None,
        }
    }

    pub(crate) fn reduce(&mut self, event: ContextMenuEvent) -> ContextMenuTransition {
        match event {
            ContextMenuEvent::Open(menu) => {
                let menu = *menu;
                let current = menu.kind();
                self.active.replace(menu).map_or(
                    ContextMenuTransition::Opened(current),
                    |previous| ContextMenuTransition::Replaced {
                        previous: previous.kind(),
                        current,
                    },
                )
            }
            ContextMenuEvent::Dismiss(expected) => {
                if self.kind() != Some(expected) {
                    return ContextMenuTransition::Unchanged;
                }
                self.dismiss_current()
            }
            ContextMenuEvent::DismissCurrent => self.dismiss_current(),
        }
    }

    fn dismiss_current(&mut self) -> ContextMenuTransition {
        self.active
            .take()
            .map_or(ContextMenuTransition::Unchanged, |menu| {
                ContextMenuTransition::Dismissed(menu.kind())
            })
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
    pub(crate) const fn modal_open(self) -> bool {
        matches!(
            self,
            Self::SharedSurface(_)
                | Self::LocalSurface(_)
                | Self::ServiceLog
                | Self::Help
                | Self::Suggestions
        )
    }

    pub(crate) const fn opaque_modal_open(self) -> bool {
        match self {
            Self::SharedSurface(SurfaceKind::ProcessProperties) => true,
            Self::SharedSurface(SurfaceKind::Confirmation(kind)) => !matches!(
                kind,
                taskmanager_application::ConfirmationKind::EndTask
                    | taskmanager_application::ConfirmationKind::ServiceControl
                    | taskmanager_application::ConfirmationKind::SmartSelfTest
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
    pub(crate) const fn between(previous: bool, current: bool) -> Self {
        match (previous, current) {
            (false, false) => Self::StableClosed,
            (false, true) => Self::Opened,
            (true, true) => Self::StableOpen,
            (true, false) => Self::Closed,
        }
    }

    pub(crate) const fn is_open(self) -> bool {
        matches!(self, Self::Opened | Self::StableOpen)
    }

    pub(crate) const fn was_open(self) -> bool {
        matches!(self, Self::StableOpen | Self::Closed)
    }
}

/// One immutable interaction receipt used by focus and entrance-motion systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InteractionSnapshot {
    pub(crate) scope: InputScope,
}

impl InteractionSnapshot {
    pub(crate) const fn modal_open(self) -> bool {
        self.scope.modal_open()
    }

    pub(crate) const fn opaque_modal_open(self) -> bool {
        self.scope.opaque_modal_open()
    }

    pub(crate) const fn process_properties_open(self) -> bool {
        matches!(
            self.scope,
            InputScope::SharedSurface(SurfaceKind::ProcessProperties)
        )
    }

    pub(crate) const fn modal_transition(self, next: Self) -> PresenceTransition {
        PresenceTransition::between(self.modal_open(), next.modal_open())
    }

    pub(crate) const fn process_properties_transition(self, next: Self) -> PresenceTransition {
        PresenceTransition::between(
            self.process_properties_open(),
            next.process_properties_open(),
        )
    }
}

use super::IcedApp;

impl IcedApp {
    pub(crate) const fn local_surface(&self) -> Option<&LocalSurface> {
        self.local_surface.active()
    }

    pub(crate) const fn local_surface_kind(&self) -> Option<LocalSurfaceKind> {
        self.local_surface.kind()
    }

    pub(crate) const fn context_menu(&self) -> Option<&ContextMenu> {
        self.input.context_menu.active()
    }

    pub(crate) const fn context_menu_kind(&self) -> Option<ContextMenuKind> {
        self.input.context_menu.kind()
    }

    pub(crate) const fn run_task_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::RunTask))
    }

    pub(crate) const fn affinity_target(
        &self,
    ) -> Option<&taskmanager_core::core::process::FrozenProcessIdentity> {
        match self.local_surface() {
            Some(LocalSurface::ProcessAffinity { target }) => Some(target),
            _ => None,
        }
    }

    pub(crate) const fn affinity_open(&self) -> bool {
        self.affinity_target().is_some()
    }

    pub(crate) const fn service_details_target(&self) -> Option<&ServiceId> {
        match self.local_surface() {
            Some(LocalSurface::ServiceDetails { service_id }) => Some(service_id),
            _ => None,
        }
    }

    pub(crate) const fn user_menu_session(&self) -> Option<&SessionItem> {
        match self.context_menu() {
            Some(ContextMenu::User { session, .. }) => Some(session),
            _ => None,
        }
    }

    pub(crate) const fn process_menu_pid(&self) -> Option<u32> {
        match self.context_menu() {
            Some(ContextMenu::Process { pid }) => Some(*pid),
            _ => None,
        }
    }

    pub(crate) const fn service_menu_index(&self) -> Option<usize> {
        match self.context_menu() {
            Some(ContextMenu::Service { source_index, .. }) => Some(*source_index),
            _ => None,
        }
    }

    pub(crate) const fn service_menu_target(&self) -> Option<&ServiceItem> {
        match self.context_menu() {
            Some(ContextMenu::Service { service, .. }) => Some(service),
            _ => None,
        }
    }

    pub(crate) const fn startup_menu_index(&self) -> Option<usize> {
        match self.context_menu() {
            Some(ContextMenu::Startup { source_index, .. }) => Some(*source_index),
            _ => None,
        }
    }

    pub(crate) const fn startup_menu_entry(&self) -> Option<&StartupEntry> {
        match self.context_menu() {
            Some(ContextMenu::Startup { entry, .. }) => Some(entry),
            _ => None,
        }
    }

    pub(crate) const fn process_columns_menu_open(&self) -> bool {
        matches!(self.context_menu(), Some(ContextMenu::ProcessColumns))
    }

    pub(super) fn open_local_surface(&mut self, surface: LocalSurface) {
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
        let _ = self.local_surface.reduce(LocalSurfaceEvent::DismissCurrent);
        if previous == Some(LocalSurfaceKind::ServiceDetails) {
            self.shell.service_dependencies.close();
            self.service_details.close();
        }
        if previous == Some(LocalSurfaceKind::ProcessAffinity) {
            self.process_presentation.affinity_cpus = None;
        }
    }

    pub(super) fn dismiss_local_surface_kind(&mut self, expected: LocalSurfaceKind) {
        let transition = self
            .local_surface
            .reduce(LocalSurfaceEvent::Dismiss(expected));
        if matches!(
            transition,
            LocalSurfaceTransition::Dismissed(LocalSurfaceKind::ProcessAffinity)
        ) {
            self.process_presentation.affinity_cpus = None;
        }
        if matches!(
            transition,
            LocalSurfaceTransition::Dismissed(LocalSurfaceKind::ServiceDetails)
        ) {
            self.shell.service_dependencies.close();
            self.service_details.close();
        }
    }

    pub(super) fn open_context_menu(&mut self, menu: ContextMenu) {
        self.dismiss_local_surface();
        self.close_shell_modals();
        let _ = self
            .input
            .context_menu
            .reduce(ContextMenuEvent::Open(Box::new(menu)));
    }

    pub(super) fn dismiss_context_menu(&mut self) {
        let _ = self
            .input
            .context_menu
            .reduce(ContextMenuEvent::DismissCurrent);
    }

    pub(super) fn dismiss_context_menu_kind(&mut self, expected: ContextMenuKind) {
        let _ = self
            .input
            .context_menu
            .reduce(ContextMenuEvent::Dismiss(expected));
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
