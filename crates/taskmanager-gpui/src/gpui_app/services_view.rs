//! Services view: filter-by-status + search controls over a virtualized list of
//! native service records (status badge + name + description) with an action
//! bar (Start / Stop / Restart / Enable / Disable).
//!
//! **State ownership:** the per-page UI state (status filter) lives on
//! `RootView` (`services_state.filter`); the search query is shell-owned
//! (`shell.services`) and threaded into [`filter_services`] by value each
//! render. The action feedback lives in the shell-owned typed slot (folded at
//! render by `RootView::services_feedback`).
//! The search box
//! (`Entity<TextInputState>`) and the table (`Entity<TableState<ServicesDelegate>>`) are
//! per-window too: both are created lazily by `RootView::render` and owned on the
//! `RootView` fields (`services_search` / `services_table`), so two windows never share an
//! entity (`Entity` is `!Send`). An `InputEvent::Change` subscription writes the new value back
//! into the shell's Services query so [`filter_services`] keeps working.

use std::collections::BTreeSet;
use std::rc::Rc;
use taskmanager_shell::InfoTable;
use taskmanager_shell::service_cycle_members;
use taskmanager_ui::icons_binding::icon;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use gpui::{
    App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    Stateful, Styled, Window, div, px,
};

use crate::gpui_app::list_view;
use crate::gpui_app::root::{Hover, RootView};
use taskforest_core::core::services::ServiceAction;
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskforest_core::core::source::SourceStatus;
use taskforest_core::core::target::ServiceId;
use taskmanager_application::RefreshRequest;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_shell::InfoSortCol;
use taskmanager_ui::data::table::{TableColumn, TableDelegate, TableEvent, TableState};
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use taskmanager_ui::primitives::button::ButtonState;
use taskmanager_ui_contract::IconId;

pub use crate::gpui_app::list_view::ActionFeedback;

mod action_bar;
mod details;
mod details_state;
mod projection;
use action_bar::{ActionBarProps, action_bar};
pub use details::{render_details, render_service_log_section};
pub(crate) use details_state::ServiceLogExportNotice;
pub use details_state::{ServiceDetailsSnapshot, ServiceDetailsState};
pub use projection::{ServiceFilter, filter_services, sorted_services};

// ── service details dialog content ────────────────────────────────────────

// ── persistent search-box Entity<InputState> ──────────────────────────────────
//
// `InputState::new` requires `&mut Window`, and the `Entity` it produces is `!Send`
// (FocusHandle inside). RootView owns the entity for the window that created it.

/// The search-box `Entity<TextInputState>`, owned per window on the
/// `RootView` that renders the Services page.
pub(crate) fn init_search_entity(cx: &mut Context<RootView>) -> gpui::Entity<TextInputState> {
    let entity = cx.new(|cx| {
        let mut state = TextInputState::new(cx);
        state.set_placeholder(i18n::t("search.services"), cx);
        state
    });
    crate::gpui_app::root::wire_debounced_search(&entity, cx, |rv, value| {
        rv.shell.services.set_query(value)
    });
    entity
}

/// Focus the Services search field from the shared command router. Returns
/// false before the page has rendered its input for the first time.
pub(crate) fn focus_search(
    view: &crate::gpui_app::root::RootView,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(input) = view.services_search.as_ref() else {
        return false;
    };
    input.read(cx).focus_handle().clone().focus(window, cx);
    true
}

// ── ServicesDelegate + persistent Table entity ──────────────────────────────
//
// `render_services` is a stateless free fn called fresh every render, but the
// `Entity<TableState<ServicesDelegate>>` MUST persist across renders (TableState
// owns the scroll position, selection, focus handle, etc.). The RootView owns
// one such entity per window (field `RootView::services_table`, created lazily
// on the first Services render), so scroll/sort/selection stay window-local.

/// Table delegate for the Services list. Holds the per-render snapshot of rows and
/// the resolved `Theme` (so `render_td` can color the Status column) plus a clone of
/// the RootView entity (so `render_tr`'s hover handler can publish
/// `Hover::Service(name)` back into the root hover slot for the cursor tooltip).
/// `pub` because `RootView::services_table` exposes the typed `TableState` entity;
/// the fields are private.
pub struct ServicesDelegate {
    rows: Rc<Vec<ServiceItem>>,
    columns: Vec<TableColumn>,
    theme: Theme,
    root: Entity<RootView>,
    /// Live search query for name-cell highlighting (set per render).
    query: String,
    /// Stable IDs participating in an observed typed service cycle.
    cycle_members: BTreeSet<ServiceId>,
}

impl ServicesDelegate {
    /// Build a delegate with an empty row set, the fixed 3-column header
    /// (Status 80 / Name 280 / Description 400), and the given theme + RootView
    /// handle. Row data is filled in per-render via [`ServicesDelegate::set_data`].
    fn new(theme: Theme, root: Entity<RootView>) -> Self {
        Self {
            rows: Rc::new(Vec::new()),
            query: String::new(),
            // 3 columns matching the old header_row widths: Status 80 / Name 280
            // / Description ~400. The Table has no native flex column, so 400px
            // is a reasonable fill that scrolls horizontally on narrow windows.
            // Status + Name carry the shared inventory sort (header click
            // routes through the shell-owned `InventorySorts` slot).
            columns: vec![
                TableColumn::new("status", i18n::t("common.status"))
                    .width(px(80.0))
                    .sortable(),
                TableColumn::new("name", i18n::t("common.name"))
                    .width(px(280.0))
                    .sortable(),
                TableColumn::new("description", i18n::t("common.description")).width(px(400.0)),
            ],
            theme,
            root,
            cycle_members: BTreeSet::new(),
        }
    }

    /// Map a table column index onto the shared inventory-sort vocabulary.
    /// `None` for columns that do not carry the interactive sort (Description)
    /// — their header events are ignored by the sort bridge.
    pub(crate) fn info_sort_column(&self, col_ix: usize) -> Option<InfoSortCol> {
        match col_ix {
            0 => Some(InfoSortCol::Status),
            1 => Some(InfoSortCol::Name),
            _ => None,
        }
    }

    /// Replace the row projection + theme snapshot. The projection is already
    /// an `Rc` memo owned by `RootView`; pointer identity makes an unchanged
    /// frame a pair of cheap handle checks instead of an O(N) comparison or
    /// clone.
    fn set_data(
        &mut self,
        rows: Rc<Vec<ServiceItem>>,
        theme: Theme,
        query: &str,
        cycle_members: BTreeSet<ServiceId>,
    ) {
        if !Rc::ptr_eq(&self.rows, &rows) {
            self.rows = rows;
        }
        self.theme = theme;
        if self.query != query {
            self.query = query.to_owned();
        }
        if self.cycle_members != cycle_members {
            self.cycle_members = cycle_members;
        }
    }
}

impl TableDelegate for ServicesDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &TableColumn {
        // col_ix is bounded by columns_count (== self.columns.len()) per the
        // Table contract; `columns` is immutable after construction, so this
        // cannot be out of range.
        self.columns
            .get(col_ix)
            .expect("col_ix < columns.len() (Table contract; columns immutable)")
    }

    /// Per-row container: keep the Table's default `div().id((_, ix))` shape
    /// so the framework owns row hover and selection styling. The service
    /// description is already visible in its bounded cell and the Details
    /// action exposes the full record, so rows do not add a floating tooltip
    /// over neighboring text.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div()
            .id(("svc-row", row_ix))
            .debug_selector(move || format!("tm-svc-row:{row_ix}"))
    }

    /// Row context menu (Win11 TM parity): Start/Stop/Restart/Enable/Disable.
    /// Right-click selects the row first (the menu actions read
    /// `RootView.selected_service`), exactly like the action-bar buttons.
    fn context_menu(
        &mut self,
        row_ix: usize,
        _menu: PopupMenuState,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenuState {
        let Some(svc) = self.rows.get(row_ix) else {
            return PopupMenuState::new(Vec::new(), cx);
        };
        let root = self.root.clone();
        let id = svc.id.clone();
        root.update(cx, |v, cx| {
            v.selected_service = Some(id.clone());
            cx.notify();
        });
        PopupMenuState::new(build_service_menu(root), cx)
    }

    /// Render one cell. Col 0 = Status (colored like the old badge: Active →
    /// theme.disk, Failed → theme.gpu, else theme.fg_dim); 1 = Name (theme.fg);
    /// 2 = Description (theme.fg_dim). All at text_size 12 to match the old rows.
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = self.theme;
        // rows can be swapped by an async refresh between the Table's rows_count
        // query and this render call; fall back to an empty cell instead of
        // panicking (render paths can't return Result — the table re-syncs next frame).
        let Some(s) = self.rows.get(row_ix) else {
            return div();
        };
        match col_ix {
            0 => {
                let status = s.status;
                let color = match status {
                    ServiceStatus::Active => theme.disk,
                    ServiceStatus::Failed => theme.gpu,
                    _ => theme.fg_dim,
                };
                let cell = div()
                    .text_size(font_size(tokens::FONT_12))
                    .text_color(hsla(color))
                    .child(status.as_str().to_string());
                // The status cell is the inventory's typed-active-state readout.
                // A test-support selector carries the same typed token that the
                // cell paints, so a frame test can prove the typed state (and not
                // a fabricated label) reached this row's cell.
                #[cfg(any(test, feature = "test-support"))]
                let cell = cell
                    .debug_selector(move || format!("tm-svc-status:{row_ix}:{}", status.as_str()));
                cell
            }
            1 => {
                let cycle = self.cycle_members.contains(&s.id);
                let mut cell = div()
                    .flex()
                    .items_center()
                    .gap(definite_length(tokens::SPACE_4))
                    .min_w(px(0.0))
                    .text_size(font_size(tokens::FONT_12))
                    .text_color(hsla(theme.fg));
                if cycle {
                    cell = cell.child(
                        icon(IconId::TriangleAlert)
                            .size(px(12.0))
                            .text_color(hsla(theme.warning)),
                    );
                }
                cell.child(div().flex_1().min_w(px(0.0)).truncate().child(
                    crate::gpui_app::elements::highlighted_text(&s.name, &self.query, &self.theme),
                ))
            }
            _ => div()
                .flex()
                .min_w(px(0.0))
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(s.description.clone()),
                ),
        }
    }
}

/// Create the persistent `Entity<TableState<ServicesDelegate>>` and wire its
/// `SelectRow` / `SortChanged` events back into RootView. Called at most once
/// per window via `RootView::services_table`'s `get_or_insert_with`.
pub(crate) fn init_table_entity(
    theme: Theme,
    cx: &mut Context<RootView>,
) -> Entity<TableState<ServicesDelegate>> {
    let delegate = ServicesDelegate::new(theme, cx.entity());
    let entity = cx.new(|new_cx| TableState::new(delegate, new_cx).row_selectable(true));
    // Table → RootView bridges. SelectRow: clicking a row publishes the
    // provider-issued target at that index to RootView. SortChanged: a header
    // sort click reports the widget's absolute post-cycle state, which is
    // applied VERBATIM to the shell-owned inventory-sort slot (single
    // authority) — the memo keys on it, so the row order follows on the next
    // projection build.
    cx.subscribe(&entity, |this, state_ent, ev: &TableEvent, cx| match ev {
        TableEvent::SelectRow(row_ix) => {
            if let Some(svc) = state_ent.read(cx).delegate().rows.get(*row_ix) {
                this.selected_service = (!svc.id.as_str().is_empty()).then(|| svc.id.clone());
                cx.notify();
            }
        }
        TableEvent::SortChanged { col_ix, sort } => {
            let column = state_ent.read(cx).delegate().info_sort_column(*col_ix);
            this.apply_table_sort(InfoTable::Services, column, *sort);
            cx.notify();
        }
        _ => {}
    })
    .detach();
    entity
}

// ── top-level render (signature: added `window` for TableState::new) ────────

/// Render the Services page: action bar (Start / Stop / Restart / Enable /
/// Disable / Details) over a status-filter pill row + a search box + a
/// virtualized native-service table.
///
/// `items` is the full materialized Services snapshot — this fn filters a copy
/// (status filter + search) and orders it through the shell-owned inventory
/// sort. `selected` is the live `RootView.selected_service` (native service
/// identifier), mapped back to a row index so the Table's selection stays in
/// sync. `filter` and `query` are the status filter + search-box values owned
/// by `RootView.services_state`; `feedback` is the render-time fold of the
/// shell-owned typed Services outcome (`RootView::services_feedback`).
/// `hovered` is the uniform hover slot root passes to every page render
/// (consumed by the action-bar buttons' hover overlays).
/// Build the service row context menu (Start/Stop/Restart/Enable/Disable).
/// Mirrors the action-bar buttons: destructive actions (Stop/Restart/Disable)
/// route through the confirmation dialog, Start/Enable apply directly.
fn build_service_menu(root: Entity<RootView>) -> Vec<MenuEntry> {
    let action = |label: &'static str, confirm: bool, act: ServiceAction| {
        let root = root.clone();
        MenuEntry::Item(MenuItem::new(label, move |_, cx| {
            root.update(cx, |v, cx| {
                if let Some(id) = v.selected_service.clone() {
                    if confirm {
                        v.request_service_control_confirmation(id, act);
                    } else {
                        v.request_service_action(id, act);
                    }
                }
                cx.notify();
            });
        }))
    };
    vec![
        action(i18n::t("svc.start"), false, ServiceAction::Start),
        action(i18n::t("svc.stop"), true, ServiceAction::Stop),
        action(i18n::t("svc.restart"), true, ServiceAction::Restart),
        action(i18n::t("svc.enable"), false, ServiceAction::Enable),
        action(i18n::t("svc.disable"), true, ServiceAction::Disable),
        action(
            i18n::t("svc.reload_daemon"),
            false,
            ServiceAction::ReloadDaemon,
        ),
    ]
}

/// All straight-through services render inputs (design-debt #1 props
/// consolidation); `window`/`cx` stay explicit render-lifetime handles.
pub struct ServicesViewProps<'a> {
    pub theme: &'a Theme,
    /// Frame-derived header/body slot allocation.
    pub page_budget: list_view::ListPageBudget,
    pub items: &'a [ServiceItem],
    pub sources: &'a [SourceStatus],
    pub selected: Option<&'a ServiceId>,
    pub hovered: Option<Hover>,
    pub filter: ServiceFilter,
    pub query: &'a str,
    /// Memoized filter+sort projection (see `RootView::services_rows`);
    /// `filter`/`query` stay for the controls row, the table body reads this.
    pub rows: std::rc::Rc<Vec<ServiceItem>>,
    pub feedback: Option<ActionFeedback>,
    pub search_input: gpui::Entity<TextInputState>,
    pub table_entity: Entity<TableState<ServicesDelegate>>,
    pub retry_button: Entity<ButtonState>,
}

pub fn render_services(
    props: ServicesViewProps<'_>,
    _window: &mut Window,
    cx: &mut Context<RootView>,
) -> Div {
    let ServicesViewProps {
        theme,
        page_budget,
        // The full list is no longer read here: the action bar keys off the
        // selected/hovered identities and the table body reads the memoized
        // `rows` projection.
        items,
        sources,
        selected,
        hovered,
        filter,
        query,
        rows,
        feedback,
        search_input,
        table_entity,
        retry_button,
    } = props;
    let theme = *theme;
    let selected = selected.cloned();
    let cycle_members = service_cycle_members(items);

    // The persistent Table entity, created lazily per window by `RootView::render`
    // (RootView::services_table) on the first Services render; reused after.
    let search_entity = search_input;

    // The filter+sort projection arrived pre-computed and memoized on the
    // caller (`RootView::services_rows`); same order as before — status rank,
    // then name — now computed once per (list, filter, query) change instead
    // of once per frame.
    let sorted: &Vec<ServiceItem> = &rows;

    // Map RootView.selected_service → row index in `sorted` (None if filtered out).
    let selected_row_ix = selected
        .as_ref()
        .and_then(|id| sorted.iter().position(|service| &service.id == id));
    let selected_name = selected.as_ref().and_then(|id| {
        sorted
            .iter()
            .find(|service| &service.id == id)
            .map(|service| service.name.as_str())
    });

    // The header is two whole bands, not one wrapping row: the command band
    // (buttons + status line) and the controls band (filter cluster + search
    // box). A single wrapping toolbar would strand the search box mid-line
    // and leave the right edge dead — the same defect the shared controls
    // band now owns for every list page.
    let header = div()
        .debug_selector(|| "tm-svc-chrome".to_string())
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_8))
        .child(action_bar(
            ActionBarProps {
                theme: &theme,
                selected: selected.as_ref(),
                selected_name,
                hovered: hovered.as_ref(),
                feedback,
            },
            cx,
        ))
        .child(list_view::controls_band(
            &theme,
            &ServiceFilter::ALL,
            filter,
            hovered.as_ref(),
            &cx.entity(),
            |f, v| v.services_state.filter = f,
            &search_entity,
        ));
    let body = if sorted.is_empty() {
        // EMPTY-STATE: 0 rows → a centered hint. An empty list caused by a
        // failed source (systemctl missing / permissions / init not
        // recognized) shows the typed reason instead of "No services" —
        // never let an unavailable source read as a genuine empty system.
        list_view::unavailable_source_state(
            &theme,
            sources,
            !query.trim().is_empty(),
            RefreshRequest::Services,
            &retry_button,
            &cx.entity(),
        )
        .unwrap_or_else(|| list_view::empty_state(&theme, i18n::t("svc.noun"), query))
    } else {
        let ent = table_entity;
        let horizontal_scroll = ent.read(cx).horizontal_scroll_handle.clone();
        // Push the new row data + theme, then sync selection RootView → Table.
        // Only (re)set when the desired row actually differs to avoid
        // spurious SelectRow emits (and thus extra frames) every render.
        ent.update(cx, |s, cx| {
            s.delegate_mut()
                .set_data(Rc::clone(&rows), theme, query.trim(), cycle_members.clone());
            if s.selected_row() != selected_row_ix {
                match selected_row_ix {
                    Some(ix) => s.set_selected_row(ix, cx),
                    None => s.clear_selection(cx),
                }
            }
        });
        // Table is size_full internally; wrap so it expands to fill the
        // remaining vertical space below the action bar + controls row.
        let mut body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(definite_length(tokens::SPACE_8));
        if let Some(notice) = list_view::source_notice(
            &theme,
            sources,
            RefreshRequest::Services,
            &retry_button,
            &cx.entity(),
        ) {
            body = body.child(notice);
        }
        body.child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .child(list_view::table_scroll_frame(
                    &ent,
                    horizontal_scroll,
                    &theme,
                    page_budget.scrollbar_band,
                    "tm-services-hscroll",
                )),
        )
        .into_any_element()
    };

    list_view::ListPageScaffold::new(&theme, header, body)
        .budget(page_budget)
        .render()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_services_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/services_view/row_memo_tests.rs"]
mod row_memo_tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/services_view/log_stream_tests.rs"]
mod log_stream_tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/services_view/tests.rs"]
mod services_view_page_tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/services_view/shadow_surface_tests.rs"]
mod shadow_surface_tests;
