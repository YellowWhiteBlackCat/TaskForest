use super::*;

impl TuiApp {
    pub(crate) const fn service_menu(&self) -> Option<&ServiceMenuTarget> {
        match self.local_surface() {
            Some(TuiSurface::ServiceMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) const fn process_menu(&self) -> Option<&ProcessMenuTarget> {
        match self.local_surface() {
            Some(TuiSurface::ProcessMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) const fn batch_menu(&self) -> Option<&BatchMenuTarget> {
        match self.local_surface() {
            Some(TuiSurface::BatchMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) const fn session_menu(&self) -> Option<&SessionMenuTarget> {
        match self.local_surface() {
            Some(TuiSurface::SessionMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) const fn startup_menu(&self) -> Option<&StartupMenuTarget> {
        match self.local_surface() {
            Some(TuiSurface::StartupMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) const fn column_menu_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::ColumnMenu { .. }))
    }
}

#[test]
fn local_surface_replaces_atomically_and_stale_close_is_rejected() {
    let mut state = TuiSurfaceState::default();
    assert_eq!(
        state.reduce(TuiSurfaceEvent::Open(Box::new(TuiSurface::Settings))),
        TuiSurfaceTransition::Opened(TuiSurfaceKind::Settings)
    );
    assert_eq!(
        state.reduce(TuiSurfaceEvent::Open(Box::new(TuiSurface::About))),
        TuiSurfaceTransition::Replaced {
            previous: TuiSurfaceKind::Settings,
            current: TuiSurfaceKind::About,
        }
    );
    assert_eq!(
        state.reduce(TuiSurfaceEvent::Dismiss(TuiSurfaceKind::Settings)),
        TuiSurfaceTransition::Unchanged
    );
    assert_eq!(state.kind(), Some(TuiSurfaceKind::About));
}

#[test]
fn process_properties_visibility_has_one_shared_authority() {
    let mut app = crate::demo_app();
    app.shell.application.active_page = taskmanager_application::AppPage::Applications;
    app.reconcile_applications_cursor();
    assert!(app.open_process_properties());
    assert_eq!(app.local_surface_kind(), None);
    assert_eq!(
        app.shell.interaction_surface(),
        Some(taskmanager_application::SurfaceKind::ProcessProperties)
    );
    assert!(app.process_properties().is_some());

    app.close_local_overlays();
    assert_eq!(app.shell.interaction_surface(), None);
    assert!(app.process_properties().is_none());
}

#[test]
fn opening_a_local_surface_installs_one_input_owner() {
    let mut app = crate::demo_app();
    app.open_local_surface(TuiSurface::CommandPalette(crate::CommandPalette::default()));
    assert_eq!(
        app.input_scope(),
        TuiInputScope::LocalSurface(TuiSurfaceKind::CommandPalette)
    );
    app.open_local_surface(TuiSurface::Health);
    assert_eq!(
        app.input_scope(),
        TuiInputScope::LocalSurface(TuiSurfaceKind::Health)
    );
    app.assert_surface_invariants();
}
