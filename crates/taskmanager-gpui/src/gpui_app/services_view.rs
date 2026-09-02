//! Services view: filter-by-status + search controls over a virtualized list of
//! native service records (status badge + name + description) with an action
//! bar (Start / Stop / Restart / Enable / Disable).
//!
//! **State ownership:** the per-page UI state (status filter, search query)
//! lives on `RootView` (`services.filter` / `services.query`), threaded into
//! [`render_services`] by value each render; the action feedback lives in the
//! shell-owned typed slot (folded at render by `RootView::services_feedback`).
//! The search box
//! (`Entity<TextInputState>`) and the table (`Entity<TableState<ServicesDelegate>>`) are
//! per-window too: both are created lazily by `RootView::render` and owned on the
//! `RootView` fields (`services_search` / `services_table`), so two windows never share an
//! entity (`Entity` is `!Send`). An `InputEvent::Change` subscription writes the new value back
//! into `RootView.services_state.query` so [`filter_services`] keeps working.

use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::list_view;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::RefreshRequest;
use taskmanager_application::i18n;
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::target::ServiceId;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_shell::InfoSortCol;
use taskmanager_ui::data::table::{Table, TableColumn, TableDelegate, TableEvent, TableState};
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use taskmanager_ui::primitives::button::ButtonState;
use taskmanager_ui::primitives::toolbar::Toolbar;

pub use crate::gpui_app::list_view::ActionFeedback;

mod details;
mod details_state;
mod projection;
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
pub(crate) fn init_search_entity(
    cx: &mut Context<RootView>,
) -> gpui::Entity<taskmanager_ui::inputs::text_input::TextInputState> {
    let entity = cx.new(|cx| {
        let mut state = taskmanager_ui::inputs::text_input::TextInputState::new(cx);
        state.set_placeholder(i18n::t("search.services"), cx);
        state
    });
    crate::gpui_app::root::wire_debounced_search(&entity, cx, |rv, value| {
        rv.services_state.query = value
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
    input.read(cx).focus_handle().clone().focus(window);
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
    fn set_data(&mut self, rows: Rc<Vec<ServiceItem>>, theme: Theme, query: &str) {
        if !Rc::ptr_eq(&self.rows, &rows) {
            self.rows = rows;
        }
        self.theme = theme;
        if self.query != query {
            self.query = query.to_owned();
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
    /// (the framework chains its own `on_click` / hover / selection styling on
    /// top) and add an `on_hover` that publishes `Hover::Service(name)` to
    /// RootView so the existing cursor-following tooltip (root.rs) keeps working.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let name = self.rows.get(row_ix).map(|s| s.name.clone());
        let root = self.root.clone();
        div()
            .id(("svc-row", row_ix))
            .debug_selector(move || format!("tm-svc-row:{row_ix}"))
            .on_hover(move |is_hov: &bool, _win, cx: &mut App| {
                if let Some(n) = &name {
                    root.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Service(n.clone()))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                }
            })
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
                let color = match s.status {
                    ServiceStatus::Active => theme.disk,
                    ServiceStatus::Failed => theme.gpu,
                    _ => theme.fg_dim,
                };
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                    .text_color(taskmanager_ui::theme_binding::hsla(color))
                    .child(s.status.as_str().to_string())
            }
            1 => div()
                .flex()
                .min_w(px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(
                    crate::gpui_app::elements::highlighted_text(&s.name, &self.query, &self.theme),
                )),
            _ => div()
                .flex()
                .min_w(px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
            this.apply_table_sort(taskmanager_shell::InfoTable::Services, column, *sort);
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
    ]
}

/// All straight-through services render inputs (design-debt #1 props
/// consolidation); `window`/`cx` stay explicit render-lifetime handles.
pub struct ServicesViewProps<'a> {
    pub theme: &'a Theme,
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
    pub search_input: gpui::Entity<taskmanager_ui::inputs::text_input::TextInputState>,
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
        // The full list is no longer read here: the action bar keys off the
        // selected/hovered identities and the table body reads the memoized
        // `rows` projection.
        items: _,
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

    let header = action_bar(
        ActionBarProps {
            theme: &theme,
            selected: selected.as_ref(),
            selected_name,
            hovered: hovered.as_ref(),
            feedback,
            filter,
            search_entity: &search_entity,
        },
        cx,
    );
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
        // Push the new row data + theme, then sync selection RootView → Table.
        // Only (re)set when the desired row actually differs to avoid
        // spurious SelectRow emits (and thus extra frames) every render.
        ent.update(cx, |s, cx| {
            s.delegate_mut()
                .set_data(Rc::clone(&rows), theme, query.trim());
            if s.selected_row() != selected_row_ix {
                match selected_row_ix {
                    Some(ix) => s.set_selected_row(ix, cx),
                    None => s.clear_selection(cx),
                }
            }
        });
        // Table is size_full internally; wrap so it expands to fill the
        // remaining vertical space below the action bar + controls row.
        let mut body = div().flex_1().min_h(px(0.0)).flex().flex_col().gap(
            taskmanager_ui::theme_binding::definite_length(tokens::SPACE_8),
        );
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
                .child(Table::new(&ent, theme.palette()).bordered(true)),
        )
        .into_any_element()
    };

    list_view::ListPageScaffold::new(header, body).render()
}

// ── action bar (Start / Stop / Restart / Enable / Disable) ───────────────────

/// Build the Services action bar: Start / Stop / Restart / Enable / Disable /
/// Details, each inert until a row is selected, plus a trailing status line.
/// Each button re-scans services on success; the typed outcome lands in the
/// shell-owned feedback slot and the status line renders its render-time fold
/// (`RootView::services_feedback`) or a selection hint via the shared
/// [`list_view::feedback_status_line`].
struct ActionBarProps<'a> {
    theme: &'a Theme,
    selected: Option<&'a ServiceId>,
    selected_name: Option<&'a str>,
    hovered: Option<&'a Hover>,
    feedback: Option<ActionFeedback>,
    filter: ServiceFilter,
    search_entity: &'a Entity<TextInputState>,
}

fn action_bar(props: ActionBarProps<'_>, cx: &mut Context<RootView>) -> Div {
    let ActionBarProps {
        theme,
        selected,
        selected_name,
        hovered,
        feedback,
        filter,
        search_entity,
    } = props;
    // Transient action feedback (folded at render time from the shell-owned
    // typed outcome by `RootView::services_feedback`); when present it takes
    // the status line over the selection hint.
    let hint = match selected_name {
        Some(name) => format!("{} {}", i18n::t("hint.selected"), name),
        None => i18n::t("hint.select_service").to_string(),
    };
    // RootView entity, captured (cloned) into each action button's on_click /
    // on_hover closures so `elements::tool_btn` stays decoupled from `root` — the
    // same entity-binding pattern `filter_pill` (above) uses for the status pills.
    // `hovered_bool` mirrors filter_pill's `is_hov`: enabled AND this label hovered.
    let entity = cx.entity();
    Toolbar::new()
        .debug_selector("tm-svc-action-bar")
        .child({
            let label = "Start";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("svc.start"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.request_service_action(n, ServiceAction::Start);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child({
            let label = "Stop";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("svc.stop"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.request_service_control_confirmation(n, ServiceAction::Stop);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child({
            let label = "Restart";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("svc.restart"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.request_service_control_confirmation(n, ServiceAction::Restart);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child({
            let label = "Enable";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("common.enable"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.request_service_action(n, ServiceAction::Enable);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child({
            let label = "Disable";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("common.disable"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.request_service_control_confirmation(n, ServiceAction::Disable);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child({
            let label = "Details";
            let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                i18n::t("common.details"),
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(n) = v.selected_service.clone() {
                            v.open_service_details(n);
                            cx.notify();
                        }
                    });
                },
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent_h.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static(label))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                },
            )
        })
        .child(list_view::feedback_status_line(
            theme,
            feedback.as_ref(),
            &hint,
        ))
        .child(div().flex_1())
        .child(list_view::filter_pill_row(
            theme,
            &ServiceFilter::ALL,
            filter,
            hovered,
            &entity,
            |f, v| v.services_state.filter = f,
        ))
        .child(list_view::search_box(&theme.palette(), search_entity))
        .render()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_services_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/services_view/row_memo_tests.rs"]
mod row_memo_tests;
