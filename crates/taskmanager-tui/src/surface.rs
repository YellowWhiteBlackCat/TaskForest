//! Typed TUI-local surface and input-ownership machine.
//!
//! Shared dangerous confirmations and Process Properties are owned by the
//! application `InteractionState`. This module owns the one terminal-local
//! modal/menu. Its private slot replaces the former collection of booleans
//! and independent `Option` fields, making the modal precedence chain
//! unrepresentable.

use taskmanager_application::SurfaceKind;

use crate::{
    BatchMenuTarget, CommandPalette, ProcessMenuTarget, ServiceMenuTarget, SessionMenuTarget,
    StartupMenuTarget, TuiApp,
};

/// Target for the interactive service-dependencies modal.
#[derive(Clone, Debug)]
pub struct ServiceDependenciesTarget {
    pub service_id: taskmanager_core::core::target::ServiceId,
    pub service_name: String,
    pub scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TuiSurfaceKind {
    Settings,
    About,
    Health,
    Containers,
    ServiceMenu,
    ProcessMenu,
    BatchMenu,
    SessionMenu,
    StartupMenu,
    ColumnMenu,
    CommandPalette,
    ServiceDependencies,
}

#[derive(Clone, Debug)]
pub(crate) enum TuiSurface {
    Settings,
    About,
    Health,
    Containers,
    ServiceMenu(ServiceMenuTarget),
    ProcessMenu(Box<ProcessMenuTarget>),
    BatchMenu(BatchMenuTarget),
    SessionMenu(SessionMenuTarget),
    StartupMenu(StartupMenuTarget),
    ColumnMenu { selection: usize },
    CommandPalette(CommandPalette),
    ServiceDependencies(ServiceDependenciesTarget),
}

impl TuiSurface {
    pub(crate) const fn kind(&self) -> TuiSurfaceKind {
        match self {
            Self::Settings => TuiSurfaceKind::Settings,
            Self::About => TuiSurfaceKind::About,
            Self::Health => TuiSurfaceKind::Health,
            Self::Containers => TuiSurfaceKind::Containers,
            Self::ServiceMenu(_) => TuiSurfaceKind::ServiceMenu,
            Self::ProcessMenu(_) => TuiSurfaceKind::ProcessMenu,
            Self::BatchMenu(_) => TuiSurfaceKind::BatchMenu,
            Self::SessionMenu(_) => TuiSurfaceKind::SessionMenu,
            Self::StartupMenu(_) => TuiSurfaceKind::StartupMenu,
            Self::ColumnMenu { .. } => TuiSurfaceKind::ColumnMenu,
            Self::CommandPalette(_) => TuiSurfaceKind::CommandPalette,
            Self::ServiceDependencies(_) => TuiSurfaceKind::ServiceDependencies,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum TuiSurfaceEvent {
    Open(Box<TuiSurface>),
    DismissCurrent,
    Dismiss(TuiSurfaceKind),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TuiSurfaceTransition {
    #[default]
    Unchanged,
    Opened(TuiSurfaceKind),
    Replaced {
        previous: TuiSurfaceKind,
        current: TuiSurfaceKind,
    },
    Dismissed(TuiSurfaceKind),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TuiSurfaceState {
    active: Option<TuiSurface>,
}

impl TuiSurfaceState {
    pub(crate) const fn active(&self) -> Option<&TuiSurface> {
        self.active.as_ref()
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut TuiSurface> {
        self.active.as_mut()
    }

    pub(crate) const fn kind(&self) -> Option<TuiSurfaceKind> {
        match self.active.as_ref() {
            Some(surface) => Some(surface.kind()),
            None => None,
        }
    }

    pub(crate) fn reduce(&mut self, event: TuiSurfaceEvent) -> TuiSurfaceTransition {
        match event {
            TuiSurfaceEvent::Open(surface) => {
                let surface = *surface;
                let current = surface.kind();
                self.active.replace(surface).map_or(
                    TuiSurfaceTransition::Opened(current),
                    |previous| TuiSurfaceTransition::Replaced {
                        previous: previous.kind(),
                        current,
                    },
                )
            }
            TuiSurfaceEvent::Dismiss(expected) => {
                if self.kind() != Some(expected) {
                    return TuiSurfaceTransition::Unchanged;
                }
                self.dismiss_current()
            }
            TuiSurfaceEvent::DismissCurrent => self.dismiss_current(),
        }
    }

    pub(crate) fn take(&mut self, expected: TuiSurfaceKind) -> Option<TuiSurface> {
        (self.kind() == Some(expected))
            .then(|| self.active.take())
            .flatten()
    }

    fn dismiss_current(&mut self) -> TuiSurfaceTransition {
        self.active
            .take()
            .map_or(TuiSurfaceTransition::Unchanged, |surface| {
                TuiSurfaceTransition::Dismissed(surface.kind())
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiInputScope {
    SharedSurface(SurfaceKind),
    LocalSurface(TuiSurfaceKind),
    ServiceLog,
    Help,
    Suggestions,
    Search,
    DetailsPanel,
    Content,
}

impl TuiInputScope {
    pub(crate) const fn blocks_pointer(self) -> bool {
        !matches!(self, Self::Content)
    }
}

impl TuiApp {
    pub(crate) const fn local_surface(&self) -> Option<&TuiSurface> {
        self.local_surface.active()
    }

    pub(crate) fn local_surface_mut(&mut self) -> Option<&mut TuiSurface> {
        self.local_surface.active_mut()
    }

    pub(crate) const fn local_surface_kind(&self) -> Option<TuiSurfaceKind> {
        self.local_surface.kind()
    }

    #[must_use]
    pub const fn settings_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Settings))
    }

    #[must_use]
    pub const fn about_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::About))
    }

    #[must_use]
    pub const fn health_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Health))
    }

    #[must_use]
    pub const fn containers_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Containers))
    }

    pub(crate) fn service_menu_mut(&mut self) -> Option<&mut ServiceMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ServiceMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn process_menu_mut(&mut self) -> Option<&mut ProcessMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ProcessMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn batch_menu_mut(&mut self) -> Option<&mut BatchMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::BatchMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn session_menu_mut(&mut self) -> Option<&mut SessionMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::SessionMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn startup_menu_mut(&mut self) -> Option<&mut StartupMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::StartupMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn service_dependencies_mut(&mut self) -> Option<&mut ServiceDependenciesTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ServiceDependencies(target)) => Some(target),
            _ => None,
        }
    }

    pub(crate) const fn column_menu_selection(&self) -> Option<usize> {
        match self.local_surface() {
            Some(TuiSurface::ColumnMenu { selection }) => Some(*selection),
            _ => None,
        }
    }

    pub(crate) fn column_menu_selection_mut(&mut self) -> Option<&mut usize> {
        match self.local_surface_mut() {
            Some(TuiSurface::ColumnMenu { selection }) => Some(selection),
            _ => None,
        }
    }

    pub(crate) const fn command_palette(&self) -> Option<&CommandPalette> {
        match self.local_surface() {
            Some(TuiSurface::CommandPalette(palette)) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn command_palette_mut(&mut self) -> Option<&mut CommandPalette> {
        match self.local_surface_mut() {
            Some(TuiSurface::CommandPalette(palette)) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn process_properties(&self) -> Option<&crate::ProcessPropertiesTarget> {
        self.shell
            .process_properties_target()
            .and(self.process_properties_view.as_ref())
    }

    pub(crate) fn process_properties_mut(&mut self) -> Option<&mut crate::ProcessPropertiesTarget> {
        self.shell
            .process_properties_target()
            .and(self.process_properties_view.as_mut())
    }

    pub(crate) fn open_local_surface(&mut self, surface: TuiSurface) {
        self.shell.dismiss_overlay();
        self.shell.close_service_log();
        self.shell.dismiss_informational_overlay();
        self.shell.close_search();
        self.focus_panel = crate::FocusPanel::Table;
        let _ = self
            .local_surface
            .reduce(TuiSurfaceEvent::Open(Box::new(surface)));
    }

    pub(crate) fn dismiss_local_surface(&mut self) {
        let _ = self.local_surface.reduce(TuiSurfaceEvent::DismissCurrent);
    }

    pub(crate) fn dismiss_local_surface_kind(&mut self, expected: TuiSurfaceKind) {
        let _ = self
            .local_surface
            .reduce(TuiSurfaceEvent::Dismiss(expected));
    }

    pub(crate) fn take_local_surface(&mut self, expected: TuiSurfaceKind) -> Option<TuiSurface> {
        self.local_surface.take(expected)
    }

    pub(crate) fn input_scope(&self) -> TuiInputScope {
        if let Some(surface) = self.shell.interaction_surface() {
            TuiInputScope::SharedSurface(surface)
        } else if let Some(surface) = self.local_surface_kind() {
            TuiInputScope::LocalSurface(surface)
        } else if self.shell.service_log.is_some() {
            TuiInputScope::ServiceLog
        } else if self.shell.help_open() {
            TuiInputScope::Help
        } else if self.shell.suggestions_open() {
            TuiInputScope::Suggestions
        } else if self.shell.search_active() {
            TuiInputScope::Search
        } else if self.focus_panel == crate::FocusPanel::Details {
            TuiInputScope::DetailsPanel
        } else {
            TuiInputScope::Content
        }
    }

    pub(crate) fn assert_surface_invariants(&self) {
        let owner_count = usize::from(self.shell.interaction_surface().is_some())
            + usize::from(self.local_surface_kind().is_some())
            + usize::from(self.shell.service_log.is_some())
            + usize::from(self.shell.help_open())
            + usize::from(self.shell.suggestions_open())
            + usize::from(self.shell.search_active())
            + usize::from(self.focus_panel == crate::FocusPanel::Details);
        debug_assert!(owner_count <= 1, "TUI input surfaces must be exclusive");
    }
}

#[cfg(test)]
#[path = "../tests/gui/surface_state_tests.rs"]
mod tests;
