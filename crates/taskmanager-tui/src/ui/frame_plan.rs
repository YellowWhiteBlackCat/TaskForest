//! Immutable terminal frame geometry shared by rendering and input.
//!
//! The plan is deliberately a value-only projection: it contains terminal
//! rectangles, bounded table windows, and the active input scope, but no
//! Ratatui widgets and no mutable application state. A committed plan can be
//! reused by pointer input until the next frame is painted.

use ratatui::layout::{Constraint, Layout, Rect};
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_application::{AppPage, SurfaceKind};
use taskmanager_ui_contract::navigation::NavOrientation;

use crate::{TuiApp, TuiInputScope};
mod focus;
pub(crate) mod overlays;
use focus::local_surface_focus_control;
use overlays::{TuiOverlayControls, TuiOverlayPlan, overlay_plan};

use super::{pages, process_properties::ProcessDetailsSection, process_table};

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
pub(crate) fn frame_chrome_layout(area: Rect, page: FrameChromePage) -> FrameChromeLayout {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(
            if area.height < 20 && page == FrameChromePage::HistoryReview {
                2
            } else {
                4
            },
        ),
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameChromePage {
    Standard,
    HistoryReview,
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
    SharedSurface(SurfaceKind),
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
    NavigationPage(AppPage),
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
    /// A form row painted inside the settings overlay. The index is the
    /// stable settings-field index the keyboard focus walks.
    SettingsField {
        index: usize,
    },
    /// The Applications search field's painted content row. The column is
    /// resolved into a query byte through the same cell geometry the
    /// renderer used.
    SearchCaret {
        column: u16,
    },
}

/// The Applications search field's painted text slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchFieldPlan {
    /// The content row (inside the field's border).
    pub(crate) row: u16,
    /// The first cell of the query text (the search glyph and its gap
    /// occupy the two cells before it).
    pub(crate) text_x: u16,
    /// The clickable content span, border excluded.
    pub(crate) left: u16,
    pub(crate) right: u16,
}

impl SearchFieldPlan {
    /// The query byte boundary under one painted cell.
    #[must_use]
    pub(crate) fn byte_at(self, query: &str, column: u16) -> usize {
        super::text::byte_at_cell(query, usize::from(column.saturating_sub(self.text_x)))
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
                    SurfaceKind::ProcessProperties => TuiFocusControl::PropertiesTab(
                        app.process_properties()
                            .map_or(ProcessDetailsSection::default(), |target| target.section),
                    ),
                    SurfaceKind::Confirmation(_) => TuiFocusControl::ConfirmationChoice,
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

/// The immutable geometry and input-scope plan for one painted terminal
/// frame. It is built from the current app state before painting and can be
/// retained by the runtime as the committed hit-test plan until the next draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuiFramePlan {
    pub(super) area: Rect,
    pub(super) chrome: FrameChromeLayout,
    pub(super) navigation: Option<Rect>,
    pub(super) page: TuiPageLayout,
    pub(super) input_scope: TuiInputScope,
    pub(super) focus: TuiFocusPlan,
    pub(super) overlay: Option<TuiOverlayPlan>,
    pub(super) search: Option<SearchFieldPlan>,
}

impl TuiFramePlan {
    #[must_use]
    pub(crate) fn build(app: &TuiApp, area: Rect) -> Self {
        let mut chrome = frame_chrome_layout(
            area,
            if app.page() == AppPage::AppHistory
                || (app.page() == AppPage::System
                    && app.system_section == SystemPageSection::Dashboard)
                || (app.page() == AppPage::Performance && app.history_replay_open())
            {
                FrameChromePage::HistoryReview
            } else {
                FrameChromePage::Standard
            },
        );
        let navigation = if app.nav_orientation == NavOrientation::Vertical {
            let [rail, body] =
                Layout::horizontal([Constraint::Length(10), Constraint::Min(1)]).areas(chrome.body);
            chrome.body = body;
            Some(rail)
        } else {
            None
        };
        let body = chrome.body;
        let input_scope = app.input_scope();
        let page = match app.page() {
            AppPage::Performance if app.history_replay_open() => TuiPageLayout::Performance {
                selector: Rect::ZERO,
                content: body,
            },
            AppPage::Performance => {
                let [selector, content] =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(body);
                TuiPageLayout::Performance { selector, content }
            }
            AppPage::Applications => {
                let process = process_table::process_table_layout(body, app.focus_panel);
                let table =
                    TablePanelProjection::new(process.table, app.visual_row_count(), app.selected);
                TuiPageLayout::Applications { process, table }
            }
            AppPage::Services => {
                let page = pages::services_page_layout(app, body);
                let table = TablePanelProjection::new(
                    page.table,
                    app.shell.visible_service_count(),
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
        let search = match page {
            TuiPageLayout::Applications { process, .. } if process.search.height >= 3 => {
                Some(SearchFieldPlan {
                    row: process.search.y.saturating_add(1),
                    text_x: process.search.x.saturating_add(3),
                    left: process.search.x.saturating_add(1),
                    right: process.search.right().saturating_sub(1),
                })
            }
            TuiPageLayout::Services { page, .. } if page.search.height >= 3 => {
                Some(SearchFieldPlan {
                    row: page.search.y.saturating_add(1),
                    text_x: page.search.x.saturating_add(3),
                    left: page.search.x.saturating_add(1),
                    right: page.search.right().saturating_sub(1),
                })
            }
            TuiPageLayout::Performance { .. }
            | TuiPageLayout::Applications { .. }
            | TuiPageLayout::Services { .. }
            | TuiPageLayout::Startup { .. }
            | TuiPageLayout::Users { .. }
            | TuiPageLayout::System { .. }
            | TuiPageLayout::AppHistory { .. } => None,
        };
        Self {
            area,
            chrome,
            navigation,
            page,
            input_scope,
            focus: TuiFocusPlan::build(app, input_scope),
            overlay: overlay_plan(app, area, input_scope),
            search,
        }
    }

    /// The Applications search field's painted slot for this frame.
    #[must_use]
    pub(crate) const fn search_field(&self) -> Option<SearchFieldPlan> {
        self.search
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
            if let Some(settings) = overlay.settings
                && settings.body.contains((column, row).into())
                && let Some(index) = settings.field_at(row)
            {
                return Some(TuiHitTarget::SettingsField { index });
            }
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
        if let Some(rail) = self
            .navigation
            .filter(|rail| rail.contains((column, row).into()))
        {
            let index = usize::from(row.saturating_sub(rail.y));
            if let Some(page) = AppPage::ALL.get(index) {
                return Some(TuiHitTarget::NavigationPage(*page));
            }
            return None;
        }
        if let Some(search) = self.search
            && row == search.row
            && column >= search.left
            && column < search.right
        {
            return Some(TuiHitTarget::SearchCaret { column });
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
