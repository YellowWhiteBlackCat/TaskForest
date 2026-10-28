//! Overlay geometry and painted control rows.
//!
//! One authority for the popup rectangle, the painted control-row bands, and
//! the settings form's pointer rows. The renderer and the committed frame plan
//! both consume these values.

use ratatui::layout::Rect;
use taskmanager_application::{ConfirmationKind, SurfaceKind};

use crate::ui::settings;
use crate::ui::{batch_menu, process_menu, service_menu, session_menu, startup_menu};
use crate::{TuiApp, TuiInputScope};

use super::centered_popup;

/// The active overlay's exact painted rectangle and owning input scope.  The
/// rectangle is part of the frame plan so an overlay renderer cannot quietly
/// recalculate a different popup from the same terminal area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuiOverlayPlan {
    pub(crate) scope: TuiInputScope,
    pub(crate) popup: Rect,
    /// The clickable control rows painted inside the popup, if the surface
    /// has any. Every other popup cell stays a blocked `Overlay` hit.
    pub(crate) controls: Option<TuiOverlayControls>,
    /// The settings form's painted field rows, when the popup is Settings.
    pub(crate) settings: Option<settings::SettingsOverlayLayout>,
}

/// The painted, pointer-addressable control rows of one overlay surface.
/// Geometry mirrors the surface renderer exactly — one border cell, the
/// renderer's own header/footer bands, and the same item inventory the
/// renderer iterates — and both consume the same committed popup rectangle,
/// so a click and the highlight paint can never drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuiOverlayControls {
    /// The surface whose popup paints these rows; it must still own the
    /// keyboard when a click on a stale plan is applied.
    pub(crate) surface: crate::TuiSurfaceKind,
    /// Absolute y of control row 0; every control paints one full-width row.
    pub(crate) first_row: u16,
    /// Absolute x/width of the control rows (the popup's inner width).
    pub(crate) x: u16,
    pub(crate) width: u16,
    /// Painted control count: the surface's item inventory clamped to the
    /// body rows the popup can actually show (the renderer clips the rest).
    pub(crate) count: u16,
}

impl TuiOverlayControls {
    /// The control index addressed by one cell, if the cell is on a painted
    /// control row. Border, header, footer, and gap cells yield `None`.
    #[must_use]
    pub(crate) fn row_at(self, column: u16, row: u16) -> Option<usize> {
        if column < self.x || column >= self.x.saturating_add(self.width) {
            return None;
        }
        if row < self.first_row {
            return None;
        }
        let offset = row - self.first_row;
        if offset >= self.count {
            return None;
        }
        Some(usize::from(offset))
    }
}

pub(super) fn overlay_plan(
    app: &TuiApp,
    area: Rect,
    scope: TuiInputScope,
) -> Option<TuiOverlayPlan> {
    let popup = overlay_popup(area, scope)?;
    let controls = match scope {
        TuiInputScope::LocalSurface(surface) => overlay_controls(app, surface, popup),
        TuiInputScope::SharedSurface(_)
        | TuiInputScope::ServiceLog
        | TuiInputScope::Help
        | TuiInputScope::Suggestions
        | TuiInputScope::Search
        | TuiInputScope::DetailsPanel
        | TuiInputScope::Content => None,
    };
    let settings = match scope {
        TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Settings) => Some(
            settings::settings_overlay_layout(app, popup, Some(app.settings_form.field)),
        ),
        TuiInputScope::LocalSurface(_) => None,
        TuiInputScope::SharedSurface(_)
        | TuiInputScope::ServiceLog
        | TuiInputScope::Help
        | TuiInputScope::Suggestions
        | TuiInputScope::Search
        | TuiInputScope::DetailsPanel
        | TuiInputScope::Content => None,
    };
    Some(TuiOverlayPlan {
        scope,
        popup,
        controls,
        settings,
    })
}

/// Project the clickable control rows one local surface paints inside its
/// popup. Every band mirrors that surface's renderer exactly: one border
/// cell, the renderer's own header/footer layout splits, and the same item
/// inventory the renderer iterates. Surfaces without actionable controls
/// (the settings form, the informational overlays) yield `None`, so every
/// cell there stays a blocked `Overlay` hit. A popup too small to guarantee
/// the painted body bands also yields `None`: an unresolvable cell is always
/// fail-closed blocked, never a guess.
fn overlay_controls(
    app: &TuiApp,
    surface: crate::TuiSurfaceKind,
    popup: Rect,
) -> Option<TuiOverlayControls> {
    // The bordered inner rectangle every overlay paints its content into
    // (`Block::inner` over `Borders::ALL`).
    let inner = Rect {
        x: popup.x.saturating_add(1),
        y: popup.y.saturating_add(1),
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };
    // (header rows above the first control, footer rows, item inventory) —
    // the exact band splits each renderer applies to the bordered inner area.
    let (header_rows, footer_rows, items) = match surface {
        // Service/Process/Batch/Session menus paint a frozen-target line and
        // a blank line before the first action row, above a 3-row footer.
        crate::TuiSurfaceKind::ServiceMenu => (2, 3, service_menu::MENU_ACTIONS.len()),
        crate::TuiSurfaceKind::ProcessMenu => (2, 3, process_menu::MENU_ACTIONS.len()),
        crate::TuiSurfaceKind::BatchMenu => (2, 3, batch_menu::MENU_ACTIONS.len()),
        crate::TuiSurfaceKind::SessionMenu => (2, 3, session_menu::MENU_ACTIONS.len()),
        // The startup menu paints its action rows directly at the body top.
        crate::TuiSurfaceKind::StartupMenu => (0, 3, startup_menu::MENU_ACTIONS.len()),
        // The column menu lists the toggleable columns with a 2-row footer.
        crate::TuiSurfaceKind::ColumnMenu => (0, 2, TuiApp::toggleable_columns().len()),
        // The palette paints a 3-row filter field, then the filtered rows,
        // then a 2-row footer. The filtered inventory is baked at plan-build
        // time so a later keystroke cannot retarget a committed click.
        crate::TuiSurfaceKind::CommandPalette => (3, 2, app.filtered_palette_rows().len()),
        crate::TuiSurfaceKind::Settings
        | crate::TuiSurfaceKind::SavedViews
        | crate::TuiSurfaceKind::SidebarEditor
        | crate::TuiSurfaceKind::About
        | crate::TuiSurfaceKind::SystemInformation
        | crate::TuiSurfaceKind::Health
        | crate::TuiSurfaceKind::Containers
        | crate::TuiSurfaceKind::ServiceDependencies
        | crate::TuiSurfaceKind::ProcessAffinity
        | crate::TuiSurfaceKind::DiagnosticBundle
        | crate::TuiSurfaceKind::FirstRun => return None,
    };
    let header_rows = u16::try_from(header_rows).unwrap_or(u16::MAX);
    let footer_rows = u16::try_from(footer_rows).unwrap_or(u16::MAX);
    let items = u16::try_from(items).unwrap_or(u16::MAX);
    // One body row beyond the bands must be guaranteed (every body slot is a
    // `Min` constraint) or the renderer's layout is degenerate and no control
    // row can be resolved honestly.
    if inner.width == 0 || inner.height < header_rows.saturating_add(footer_rows).saturating_add(1)
    {
        return None;
    }
    let body_rows = inner.height - footer_rows;
    let count = items.min(body_rows - header_rows);
    if count == 0 {
        return None;
    }
    Some(TuiOverlayControls {
        surface,
        first_row: inner.y + header_rows,
        x: inner.x,
        width: inner.width,
        count,
    })
}

/// Resolve an overlay popup from its typed input owner. The frame plan stores
/// this result for production rendering and hit-tests; isolated render tests
/// call the same function directly.
pub(crate) fn overlay_popup(area: Rect, scope: TuiInputScope) -> Option<Rect> {
    let size = match scope {
        TuiInputScope::SharedSurface(surface) => match surface {
            SurfaceKind::ProcessProperties => (96, 30),
            SurfaceKind::Confirmation(kind) => confirmation_size(kind),
        },
        TuiInputScope::LocalSurface(surface) => match surface {
            crate::TuiSurfaceKind::Settings => (68, 32),
            crate::TuiSurfaceKind::SavedViews => (68, 24),
            crate::TuiSurfaceKind::SidebarEditor => (68, 24),
            crate::TuiSurfaceKind::About => (72, 20),
            crate::TuiSurfaceKind::SystemInformation => (84, 30),
            crate::TuiSurfaceKind::Health => (84, 30),
            crate::TuiSurfaceKind::Containers => (84, 22),
            crate::TuiSurfaceKind::ServiceMenu => (52, 13),
            crate::TuiSurfaceKind::ProcessMenu => (52, 19),
            crate::TuiSurfaceKind::BatchMenu => (52, 15),
            crate::TuiSurfaceKind::SessionMenu | crate::TuiSurfaceKind::StartupMenu => (52, 11),
            crate::TuiSurfaceKind::ColumnMenu => (
                44,
                u16::try_from(crate::TuiApp::toggleable_columns().len())
                    .unwrap_or(u16::MAX)
                    .saturating_add(4),
            ),
            crate::TuiSurfaceKind::CommandPalette => (72, 26),
            crate::TuiSurfaceKind::ServiceDependencies => (72, 20),
            crate::TuiSurfaceKind::ProcessAffinity => (64, 16),
            crate::TuiSurfaceKind::DiagnosticBundle => (78, 24),
            // Three complete descriptor groups, copy/docs hints and the
            // fixed action/scroll footer fit the normal review. Compact
            // terminals clamp this slot and scroll only the body.
            crate::TuiSurfaceKind::FirstRun => (60, 24),
        },
        TuiInputScope::Help => (68, 24),
        TuiInputScope::Suggestions => (74, 22),
        TuiInputScope::ServiceLog
        | TuiInputScope::Search
        | TuiInputScope::DetailsPanel
        | TuiInputScope::Content => return None,
    };
    Some(centered_popup(area, size.0, size.1))
}

const fn confirmation_size(kind: ConfirmationKind) -> (u16, u16) {
    match kind {
        ConfirmationKind::EndTask => (58, 9),
        ConfirmationKind::ServiceControl
        | ConfirmationKind::StartupControl
        | ConfirmationKind::SessionControl => (60, 9),
        ConfirmationKind::SmartSelfTest => (62, 9),
        ConfirmationKind::ProcessBatch => (64, 15),
    }
}
