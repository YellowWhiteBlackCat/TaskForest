//! Local surface focus projection over stable renderer identities.
use super::{TuiApp, TuiFocusControl};

pub(super) fn local_surface_focus_control(
    app: &TuiApp,
    surface: crate::TuiSurfaceKind,
) -> TuiFocusControl {
    match surface {
        crate::TuiSurfaceKind::Settings => TuiFocusControl::SettingsField(app.settings_form.field),
        crate::TuiSurfaceKind::SavedViews => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::SavedViews { selected, .. } => {
                selected.and_then(|id| app.saved_views.rows.iter().position(|entry| entry.id == id))
            }
            _ => None,
        }),
        crate::TuiSurfaceKind::SidebarEditor => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::SidebarEditor { selected } => selected.as_ref().and_then(|key| {
                app.sidebar_entries()
                    .iter()
                    .position(|entry| &entry.key == key)
            }),
            _ => None,
        }),
        crate::TuiSurfaceKind::CommandPalette => TuiFocusControl::PaletteItem {
            index: app.command_palette().map_or(0, |palette| palette.selection),
        },
        crate::TuiSurfaceKind::ServiceMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::ServiceMenu(menu) => Some(menu.selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::ProcessMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::ProcessMenu(menu) => Some(menu.selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::BatchMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::BatchMenu(menu) => Some(menu.selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::SessionMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::SessionMenu(menu) => Some(menu.selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::StartupMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::StartupMenu(menu) => Some(menu.selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::ColumnMenu => menu_index(app, surface, |surface| match surface {
            crate::TuiSurface::ColumnMenu { selection } => Some(*selection),
            _ => None,
        }),
        crate::TuiSurfaceKind::About
        | crate::TuiSurfaceKind::SystemInformation
        | crate::TuiSurfaceKind::Health
        | crate::TuiSurfaceKind::Containers
        | crate::TuiSurfaceKind::ServiceDependencies
        | crate::TuiSurfaceKind::ProcessAffinity
        | crate::TuiSurfaceKind::DiagnosticBundle
        | crate::TuiSurfaceKind::FirstRun => TuiFocusControl::Viewport,
    }
}

fn menu_index(
    app: &TuiApp,
    surface: crate::TuiSurfaceKind,
    index: impl FnOnce(&crate::TuiSurface) -> Option<usize>,
) -> TuiFocusControl {
    TuiFocusControl::MenuItem {
        surface,
        index: app.local_surface().and_then(index).unwrap_or(0),
    }
}
