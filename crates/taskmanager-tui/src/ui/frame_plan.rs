//! Immutable terminal frame geometry shared by rendering and input.
//!
//! The plan is deliberately a value-only projection: it contains terminal
//! rectangles, bounded table windows, and the active input scope, but no
//! Ratatui widgets and no mutable application state. A committed plan can be
//! reused by pointer input until the next frame is painted.

use ratatui::layout::{Constraint, Layout, Rect};
use taskmanager_application::AppPage;

use crate::{TuiApp, TuiInputScope};

use super::{
    batch_menu, pages, process_menu, process_properties::ProcessDetailsSection, process_table,
    service_menu, session_menu, startup_menu,
};

/// Rows occupied by the table border, header, and the header's bottom margin
/// before the first data row. The hit-test rule belongs to the frame plan so
/// it cannot drift from the table widget's row grammar.
pub(crate) const TABLE_DATA_ROW_OFFSET: u16 = 3;

/// The fixed outer frame bands shared by the renderer and pointer hit-tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameChromeLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) footer: Rect,
}

/// Resolve the terminal frame's outer chrome once for the current area.
#[must_use]
pub(crate) fn frame_chrome_layout(area: Rect) -> FrameChromeLayout {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .areas(area);
    FrameChromeLayout {
        header,
        body,
        footer,
    }
}

/// Resolve the shared centered popup geometry.  Every local/shared surface
/// uses this terminal-cell rule, so clamping and centering cannot drift between
/// menus, confirmations, help, and informational overlays.
#[must_use]
pub(crate) fn centered_popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// A bounded slice of a table's canonical row order plus the selected index
/// relative to that slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableWindow {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) selected: usize,
}

/// Compute the row window for a bordered table with one header row and the
/// shared header bottom margin. The selected row remains global while only
/// this bounded slice is materialized by the renderer.
#[must_use]
pub(crate) fn table_window(total: usize, selected: usize, area: Rect) -> TableWindow {
    if total == 0 {
        return TableWindow {
            start: 0,
            end: 0,
            selected: 0,
        };
    }
    let body_rows = usize::from(area.height.saturating_sub(4)).max(1);
    let visible = body_rows.min(total);
    let selected = selected.min(total - 1);
    let start = selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible));
    TableWindow {
        start,
        end: start + visible,
        selected: selected - start,
    }
}

/// A table's painted area, total canonical row count, and bounded painted
/// window. Pointer input consumes this same value instead of rebuilding the
/// page geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TablePanelProjection {
    pub(crate) area: Rect,
    pub(crate) total: usize,
    pub(crate) window: TableWindow,
}

impl TablePanelProjection {
    #[must_use]
    pub(crate) fn new(area: Rect, total: usize, selected: usize) -> Self {
        Self {
            area,
            total,
            window: table_window(total, selected, area),
        }
    }
}

/// Page-specific geometry already resolved from the frame's body. The page
/// renderers receive these values rather than recalculating their top-level
/// slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiPageLayout {
    Performance {
        selector: Rect,
        content: Rect,
    },
    Applications {
        process: process_table::ProcessTableLayout,
        table: TablePanelProjection,
    },
    Services {
        page: pages::ServicesPageLayout,
        table: TablePanelProjection,
    },
    Startup {
        page: pages::StartupPageLayout,
        table: TablePanelProjection,
    },
    Users {
        page: pages::UsersPageLayout,
        table: TablePanelProjection,
    },
    System {
        content: Rect,
    },
    AppHistory {
        content: Rect,
    },
}

/// The terminal focus owner for the current frame. A surface may own the
/// entire keyboard, while Applications has two navigable panels that share a
/// Tab cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiFocusTarget {
    Content,
    Search,
    ApplicationsTable,
    ApplicationsDetails,
    SharedSurface(taskmanager_application::SurfaceKind),
    LocalSurface(crate::TuiSurfaceKind),
    ServiceLog,
    Help,
    Suggestions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiFocusOrder {
    None,
    ApplicationsPanels,
}

/// The focused control inside the current focus owner.  The outer target says
/// which surface owns the keyboard; this value says what that surface is
/// currently addressing — the focused settings field, the highlighted menu
/// item, the active properties tab, or the scrolled viewport — so renderers
/// and key consumers read one projection instead of re-deriving it. Cursor
/// authority stays with the owning state; the plan only mirrors it per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiFocusControl {
    None,
    SearchField,
    TableCursor,
    DetailsViewport,
    PropertiesTab(ProcessDetailsSection),
    ConfirmationChoice,
    SettingsField(usize),
    MenuItem {
        surface: crate::TuiSurfaceKind,
        index: usize,
    },
    PaletteItem {
        index: usize,
    },
    Viewport,
}

/// A typed terminal hit result.  The page identity travels with the row so a
/// coordinate from a committed frame cannot be silently interpreted against a
/// later page selection.  Every actionable overlay control is resolved
/// explicitly here; unsupported cells stay blocked (`Overlay`) or `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiHitTarget {
    TableRow {
        page: AppPage,
        index: usize,
    },
    /// A cell inside the active modal's painted rectangle that addresses no
    /// actionable control (title, border, footer, gap). It stays a blocked
    /// no-op instead of letting the click fall through to the background.
    Overlay {
        scope: TuiInputScope,
    },
    /// An actionable control row painted inside the active overlay popup:
    /// an action-menu item, a column-menu row, or a command-palette command.
    /// The surface identity travels with the index so a coordinate from a
    /// committed frame cannot act on a different surface's state.
    OverlayControl {
        surface: crate::TuiSurfaceKind,
        index: usize,
    },
}

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

/// Immutable focus projection paired with the active input scope. It is a
/// small terminal analogue of the GUI focus shell: renderers consume the
/// target, while keyboard reducers still own state transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuiFocusPlan {
    pub(super) target: TuiFocusTarget,
    pub(super) order: TuiFocusOrder,
    pub(super) control: TuiFocusControl,
}

impl TuiFocusPlan {
    #[must_use]
    pub(crate) fn build(app: &TuiApp, scope: TuiInputScope) -> Self {
        match scope {
            TuiInputScope::Search => Self {
                target: TuiFocusTarget::Search,
                order: TuiFocusOrder::None,
                control: TuiFocusControl::SearchField,
            },
            TuiInputScope::DetailsPanel => Self {
                target: TuiFocusTarget::ApplicationsDetails,
                order: TuiFocusOrder::ApplicationsPanels,
                control: TuiFocusControl::DetailsViewport,
            },
            TuiInputScope::Content if app.page() == AppPage::Applications => Self {
                target: TuiFocusTarget::ApplicationsTable,
                order: TuiFocusOrder::ApplicationsPanels,
                control: TuiFocusControl::TableCursor,
            },
            TuiInputScope::Content => Self {
                target: TuiFocusTarget::Content,
                order: TuiFocusOrder::None,
                control: TuiFocusControl::None,
            },
            TuiInputScope::SharedSurface(surface) => {
                let control = match surface {
                    taskmanager_application::SurfaceKind::ProcessProperties => {
                        TuiFocusControl::PropertiesTab(
                            app.process_properties()
                                .map_or(ProcessDetailsSection::default(), |target| target.section),
                        )
                    }
                    taskmanager_application::SurfaceKind::Confirmation(_) => {
                        TuiFocusControl::ConfirmationChoice
                    }
                };
                Self {
                    target: TuiFocusTarget::SharedSurface(surface),
                    order: TuiFocusOrder::None,
                    control,
                }
            }
            TuiInputScope::LocalSurface(surface) => Self {
                target: TuiFocusTarget::LocalSurface(surface),
                order: TuiFocusOrder::None,
                control: local_surface_focus_control(app, surface),
            },
            TuiInputScope::ServiceLog => Self {
                target: TuiFocusTarget::ServiceLog,
                order: TuiFocusOrder::None,
                control: TuiFocusControl::Viewport,
            },
            TuiInputScope::Help => Self {
                target: TuiFocusTarget::Help,
                order: TuiFocusOrder::None,
                control: TuiFocusControl::Viewport,
            },
            TuiInputScope::Suggestions => Self {
                target: TuiFocusTarget::Suggestions,
                order: TuiFocusOrder::None,
                control: TuiFocusControl::Viewport,
            },
        }
    }

    #[must_use]
    pub(crate) const fn applications_details_focused(self) -> bool {
        matches!(self.target, TuiFocusTarget::ApplicationsDetails)
    }

    /// The settings field the plan's owner addresses, if it names one.  A
    /// renderer without a named field paints no focus marker instead of
    /// guessing a default.
    #[must_use]
    pub(crate) const fn settings_field(self) -> Option<usize> {
        match self.control {
            TuiFocusControl::SettingsField(field) => Some(field),
            _ => None,
        }
    }

    /// The highlighted item index for `surface`'s action menu, but only when
    /// this plan's control addresses that surface.  A differently owned menu
    /// (or a non-menu control) yields `None`, so a menu renderer fail-closes
    /// instead of highlighting a row the plan never named.
    #[must_use]
    pub(crate) fn menu_item(self, surface: crate::TuiSurfaceKind) -> Option<usize> {
        match self.control {
            TuiFocusControl::MenuItem {
                surface: named,
                index,
            } if named == surface => Some(index),
            _ => None,
        }
    }

    /// The palette row the plan's owner addresses, if it names one.
    #[must_use]
    pub(crate) const fn palette_item(self) -> Option<usize> {
        match self.control {
            TuiFocusControl::PaletteItem { index } => Some(index),
            _ => None,
        }
    }

    /// The properties tab the plan's owner addresses, if it names one.
    #[must_use]
    pub(crate) const fn properties_tab(self) -> Option<ProcessDetailsSection> {
        match self.control {
            TuiFocusControl::PropertiesTab(section) => Some(section),
            _ => None,
        }
    }

    /// Whether the inline Applications search field currently owns the
    /// keyboard, so its caret and title paint from the plan rather than a
    /// second `search_active` read drifting from the focus plan.
    #[must_use]
    pub(crate) const fn search_field_focused(self) -> bool {
        matches!(self.target, TuiFocusTarget::Search)
    }
}

fn local_surface_focus_control(app: &TuiApp, surface: crate::TuiSurfaceKind) -> TuiFocusControl {
    match surface {
        crate::TuiSurfaceKind::Settings => TuiFocusControl::SettingsField(app.settings_form.field),
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
        | crate::TuiSurfaceKind::Health
        | crate::TuiSurfaceKind::Containers => TuiFocusControl::Viewport,
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

/// The immutable geometry and input-scope plan for one painted terminal
/// frame. It is built from the current app state before painting and can be
/// retained by the runtime as the committed hit-test plan until the next draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuiFramePlan {
    pub(super) area: Rect,
    pub(super) chrome: FrameChromeLayout,
    pub(super) page: TuiPageLayout,
    pub(super) input_scope: TuiInputScope,
    pub(super) focus: TuiFocusPlan,
    pub(super) overlay: Option<TuiOverlayPlan>,
}

impl TuiFramePlan {
    #[must_use]
    pub(crate) fn build(app: &TuiApp, area: Rect) -> Self {
        let chrome = frame_chrome_layout(area);
        let body = chrome.body;
        let input_scope = app.input_scope();
        let page = match app.page() {
            AppPage::Performance => {
                let [selector, content] =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(body);
                TuiPageLayout::Performance { selector, content }
            }
            AppPage::Applications => {
                let process = process_table::process_table_layout(body);
                let table =
                    TablePanelProjection::new(process.table, app.visual_row_count(), app.selected);
                TuiPageLayout::Applications { process, table }
            }
            AppPage::Services => {
                let page = pages::services_page_layout(app, body);
                let table = TablePanelProjection::new(
                    page.table,
                    app.sorted_services().len(),
                    app.selected,
                );
                TuiPageLayout::Services { page, table }
            }
            AppPage::System => TuiPageLayout::System { content: body },
            AppPage::Startup => {
                let timeline = super::boot_timeline::project_timeline(
                    app.projection().startup_boot_evidence.as_ref(),
                );
                let page = pages::startup_page_layout(
                    body,
                    timeline.as_ref().map(|projection| projection.rows.len()),
                    app.projection().startup_source.as_deref(),
                );
                let table = TablePanelProjection::new(
                    page.table,
                    app.sorted_startup_entries().len(),
                    app.selected,
                );
                TuiPageLayout::Startup { page, table }
            }
            AppPage::Users => {
                let page = pages::users_page_layout(app, body);
                let table = TablePanelProjection::new(
                    page.table,
                    app.sorted_sessions().len(),
                    app.selected,
                );
                TuiPageLayout::Users { page, table }
            }
            AppPage::AppHistory => TuiPageLayout::AppHistory { content: body },
        };
        Self {
            area,
            chrome,
            page,
            input_scope,
            focus: TuiFocusPlan::build(app, input_scope),
            overlay: overlay_plan(app, area, input_scope),
        }
    }

    /// The exact overlay geometry painted for this frame, if any.
    #[must_use]
    pub(crate) const fn overlay(&self) -> Option<TuiOverlayPlan> {
        self.overlay
    }

    /// Whether the plan was built for the app page currently selected. A
    /// stale plan is still useful for identifying the painted frame, but it
    /// must not target a different page's current state.
    #[must_use]
    pub(crate) fn page_matches(&self, page: AppPage) -> bool {
        self.page_id() == page
    }

    /// The page whose geometry this plan paints.
    #[must_use]
    pub(crate) const fn page_id(&self) -> AppPage {
        match self.page {
            TuiPageLayout::Performance { .. } => AppPage::Performance,
            TuiPageLayout::Applications { .. } => AppPage::Applications,
            TuiPageLayout::Services { .. } => AppPage::Services,
            TuiPageLayout::System { .. } => AppPage::System,
            TuiPageLayout::Startup { .. } => AppPage::Startup,
            TuiPageLayout::Users { .. } => AppPage::Users,
            TuiPageLayout::AppHistory { .. } => AppPage::AppHistory,
        }
    }

    /// The table projection for pages with pointer-addressable rows.
    #[must_use]
    pub(crate) const fn table_panel(&self) -> Option<TablePanelProjection> {
        match self.page {
            TuiPageLayout::Applications { table, .. }
            | TuiPageLayout::Services { table, .. }
            | TuiPageLayout::Startup { table, .. }
            | TuiPageLayout::Users { table, .. } => Some(table),
            TuiPageLayout::Performance { .. }
            | TuiPageLayout::System { .. }
            | TuiPageLayout::AppHistory { .. } => None,
        }
    }

    /// Resolve one terminal cell through the committed frame's typed HitMap.
    /// Resolution order: an actionable overlay control row first (the popup
    /// owns those cells), then every other popup cell as a blocked overlay
    /// hit, then the background table row. This is intentionally the only
    /// place that turns a cell into a target.
    #[must_use]
    pub(crate) fn hit_target(&self, column: u16, row: u16) -> Option<TuiHitTarget> {
        if let Some(overlay) = self
            .overlay
            .filter(|overlay| overlay.popup.contains((column, row).into()))
        {
            if let Some(controls) = overlay.controls
                && let Some(index) = controls.row_at(column, row)
            {
                return Some(TuiHitTarget::OverlayControl {
                    surface: controls.surface,
                    index,
                });
            }
            return Some(TuiHitTarget::Overlay {
                scope: overlay.scope,
            });
        }
        let index = self.table_row_at(column, row)?;
        Some(TuiHitTarget::TableRow {
            page: self.page_id(),
            index,
        })
    }

    /// The painted overlay control rows for this frame, if any. The pointer
    /// seam reads the committed count to fail closed when a filtered palette's
    /// row inventory has changed since the frame the user clicked was painted.
    #[must_use]
    pub(crate) fn overlay_controls(&self) -> Option<TuiOverlayControls> {
        self.overlay.and_then(|overlay| overlay.controls)
    }

    /// Resolve a terminal cell into the global table row that this committed
    /// frame painted. Non-table pages, panel chrome, and cells below the
    /// bounded window return `None`.
    #[must_use]
    pub(crate) fn table_row_at(&self, column: u16, row: u16) -> Option<usize> {
        let panel = self.table_panel()?;
        if column < panel.area.x
            || column >= panel.area.x + panel.area.width
            || row < panel.area.y + TABLE_DATA_ROW_OFFSET
        {
            return None;
        }
        let offset = row - (panel.area.y + TABLE_DATA_ROW_OFFSET);
        let visible = panel.window.end.saturating_sub(panel.window.start);
        if offset >= u16::try_from(visible).unwrap_or(u16::MAX) {
            return None;
        }
        Some(panel.window.start + usize::from(offset))
    }
}

fn overlay_plan(app: &TuiApp, area: Rect, scope: TuiInputScope) -> Option<TuiOverlayPlan> {
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
    Some(TuiOverlayPlan {
        scope,
        popup,
        controls,
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
        | crate::TuiSurfaceKind::About
        | crate::TuiSurfaceKind::Health
        | crate::TuiSurfaceKind::Containers => return None,
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

/// Resolve an overlay popup from its typed input owner.  Surface renderers use
/// [`planned_popup`] for their compatibility/test entry points, while the
/// frame plan stores the same result for production rendering and hit-tests.
pub(crate) fn overlay_popup(area: Rect, scope: TuiInputScope) -> Option<Rect> {
    let size = match scope {
        TuiInputScope::SharedSurface(surface) => match surface {
            taskmanager_application::SurfaceKind::ProcessProperties => (96, 30),
            taskmanager_application::SurfaceKind::Confirmation(kind) => confirmation_size(kind),
        },
        TuiInputScope::LocalSurface(surface) => match surface {
            crate::TuiSurfaceKind::Settings => (68, 32),
            crate::TuiSurfaceKind::About => (72, 18),
            crate::TuiSurfaceKind::Health => (84, 30),
            crate::TuiSurfaceKind::Containers => (84, 22),
            crate::TuiSurfaceKind::ServiceMenu => (52, 13),
            crate::TuiSurfaceKind::ProcessMenu => (52, 17),
            crate::TuiSurfaceKind::BatchMenu => (52, 15),
            crate::TuiSurfaceKind::SessionMenu | crate::TuiSurfaceKind::StartupMenu => (52, 11),
            crate::TuiSurfaceKind::ColumnMenu => (
                44,
                u16::try_from(crate::TuiApp::toggleable_columns().len())
                    .unwrap_or(u16::MAX)
                    .saturating_add(4),
            ),
            crate::TuiSurfaceKind::CommandPalette => (72, 26),
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

/// Compatibility entry for isolated surface tests.  The scope is known to be
/// an overlay at every call site; returning a zero rectangle for an accidental
/// non-overlay scope keeps the production path panic-free.
#[must_use]
#[cfg(test)]
pub(crate) fn planned_popup(area: Rect, scope: TuiInputScope) -> Rect {
    overlay_popup(area, scope).unwrap_or(Rect::ZERO)
}

const fn confirmation_size(kind: taskmanager_application::ConfirmationKind) -> (u16, u16) {
    match kind {
        taskmanager_application::ConfirmationKind::EndTask
        | taskmanager_application::ConfirmationKind::ProcessTermination => (58, 9),
        taskmanager_application::ConfirmationKind::ServiceControl
        | taskmanager_application::ConfirmationKind::StartupControl
        | taskmanager_application::ConfirmationKind::SessionControl => (60, 9),
        taskmanager_application::ConfirmationKind::ProcessBatch
        | taskmanager_application::ConfirmationKind::SmartSelfTest => (62, 9),
    }
}
