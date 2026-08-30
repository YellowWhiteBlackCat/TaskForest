use super::*;
use taskmanager_core::core::process::ProcessLiveKey;

impl IcedApp {
    pub(crate) const fn settings_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::Settings))
    }

    pub(crate) const fn about_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::About))
    }

    pub(crate) const fn health_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::Health))
    }

    pub(crate) const fn containers_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::Containers))
    }

    pub(crate) const fn alert_center_open(&self) -> bool {
        matches!(self.local_surface(), Some(LocalSurface::AlertCenter))
    }

    pub(crate) const fn disk_smart_index(&self) -> Option<usize> {
        match self.local_surface() {
            Some(LocalSurface::DiskSmart { index }) => Some(*index),
            _ => None,
        }
    }

    pub(crate) const fn user_menu_row(&self) -> Option<usize> {
        match self.context_menu() {
            Some(ContextMenu::User { visual_index, .. }) => Some(*visual_index),
            _ => None,
        }
    }

    pub(crate) fn process_properties_open(&self) -> bool {
        self.shell.process_properties_target().is_some()
    }
}

#[test]
fn local_surface_replacement_and_branch_matched_dismiss_are_explicit() {
    let mut state = LocalSurfaceState::default();
    assert_eq!(
        state.reduce(LocalSurfaceEvent::Open(LocalSurface::Settings)),
        LocalSurfaceTransition::Opened(LocalSurfaceKind::Settings)
    );
    assert_eq!(
        state.reduce(LocalSurfaceEvent::Open(LocalSurface::About)),
        LocalSurfaceTransition::Replaced {
            previous: LocalSurfaceKind::Settings,
            current: LocalSurfaceKind::About,
        }
    );
    assert_eq!(
        state.reduce(LocalSurfaceEvent::Dismiss(LocalSurfaceKind::Settings)),
        LocalSurfaceTransition::Unchanged,
        "a stale Settings close must not consume the About surface"
    );
    assert_eq!(state.kind(), Some(LocalSurfaceKind::About));
    assert_eq!(
        state.reduce(LocalSurfaceEvent::Dismiss(LocalSurfaceKind::About)),
        LocalSurfaceTransition::Dismissed(LocalSurfaceKind::About)
    );
    assert_eq!(state.kind(), None);
}

#[test]
fn context_menu_is_one_typed_slot_and_rejects_a_stale_close() {
    let mut state = ContextMenuState::default();
    assert_eq!(
        state.reduce(ContextMenuEvent::Open(Box::new(ContextMenu::Process {
            identity: ProcessLiveKey::from_parts(41, 41).expect("fixture identity"),
        }))),
        ContextMenuTransition::Opened(ContextMenuKind::Process)
    );
    assert_eq!(
        state.reduce(ContextMenuEvent::Open(Box::new(
            ContextMenu::ProcessColumns
        ))),
        ContextMenuTransition::Replaced {
            previous: ContextMenuKind::Process,
            current: ContextMenuKind::ProcessColumns,
        }
    );
    assert_eq!(
        state.reduce(ContextMenuEvent::Dismiss(ContextMenuKind::Process)),
        ContextMenuTransition::Unchanged
    );
    assert_eq!(state.kind(), Some(ContextMenuKind::ProcessColumns));
}

#[test]
fn opening_a_new_input_owner_closes_the_previous_owner() {
    let mut app = IcedApp::demo();
    app.open_local_surface(LocalSurface::RunTask);
    assert_eq!(
        app.input_scope(),
        InputScope::LocalSurface(LocalSurfaceKind::RunTask)
    );

    app.open_context_menu(ContextMenu::Process {
        identity: ProcessLiveKey::from_parts(1810, 1810).expect("fixture identity"),
    });
    assert_eq!(app.local_surface_kind(), None);
    assert_eq!(
        app.input_scope(),
        InputScope::ContextMenu(ContextMenuKind::Process)
    );

    app.open_local_surface(LocalSurface::Health);
    assert_eq!(app.context_menu_kind(), None);
    assert_eq!(
        app.input_scope(),
        InputScope::LocalSurface(LocalSurfaceKind::Health)
    );
    app.assert_surface_invariants();
}

#[test]
fn run_task_and_alert_center_are_real_modal_input_scopes() {
    for kind in [LocalSurfaceKind::RunTask, LocalSurfaceKind::AlertCenter] {
        let scope = InputScope::LocalSurface(kind);
        assert!(scope.modal_open());
        assert!(scope.opaque_modal_open());
    }
    assert!(!InputScope::ContextMenu(ContextMenuKind::Process).modal_open());
}
