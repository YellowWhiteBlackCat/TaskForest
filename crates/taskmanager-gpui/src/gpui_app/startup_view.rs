//! Startup view: filter-by-status + search controls over a virtualized list of autostart
//! entries (status + name + source + command) with an action bar (Enable / Disable).
//!
//! Mirrors [`crate::gpui_app::services_view`] 1:1 in structure.
//!
//! **State ownership:** the status-filter + search-query live IN [`RootView`]
//! (`startup.filter` / `startup.query`), and the action feedback lives in the
//! shell-owned typed slot (folded at render by
//! `RootView::startup_feedback`), passed into [`render_startup`] as params
//! each render — the module holds NO UI state of its own (the old module-level
//! `Mutex<UiState>` is gone). The search box
//! (`Entity<TextInputState>`) and the table (`Entity<TableState<StartupDelegate>>`) are
//! per-window too: both are created lazily by `RootView::render` and owned on
//! the `RootView` fields (`startup_search` / `startup_table`), so two windows never share an
//! entity. An `InputEvent::Change` subscription writes the new value back into
//! `RootView.startup_state.query` so [`filter_startup`] keeps working.

use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use taskmanager_ui::data::table::{Table, TableColumn, TableDelegate, TableEvent, TableState};
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use taskmanager_ui::primitives::button::ButtonState;
use taskmanager_ui::primitives::toolbar::Toolbar;
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::list_view::{self, FilterSpec};
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::RefreshRequest;
use taskmanager_application::i18n;
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupEntryId, StartupImpact, StartupImpactEvidence,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_shell::{InfoSortCol, SortDir};

pub use crate::gpui_app::list_view::ActionFeedback;

mod boot_evidence;
use boot_evidence::boot_evidence_strip;
mod layout;
pub use layout::{StartupPageBudget, TimelinePresentation};
mod source_notice;
use source_notice::startup_source_detail;

// ── module-local UI state (filter + search) ──────────────────────────────────

/// Status filter for the Startup list. `All` matches every entry; the other variants
/// match the corresponding enabled state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupFilter {
    All,
    Enabled,
    Disabled,
}

impl StartupFilter {
    fn matches(self, enabled: bool) -> bool {
        match self {
            StartupFilter::All => true,
            StartupFilter::Enabled => enabled,
            StartupFilter::Disabled => !enabled,
        }
    }

    const ALL: [StartupFilter; 3] = [
        StartupFilter::All,
        StartupFilter::Enabled,
        StartupFilter::Disabled,
    ];
}

/// `FilterSpec` impl so [`list_view::filter_pill_row`] can render the Startup
/// status-filter pills generically. `id` is also the `Hover::Static`
/// discriminator so the active/hover overlay resolves correctly.
impl FilterSpec for StartupFilter {
    fn label(self) -> &'static str {
        match self {
            StartupFilter::All => i18n::t("common.all"),
            StartupFilter::Enabled => i18n::t("common.enabled"),
            StartupFilter::Disabled => i18n::t("common.disabled"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            StartupFilter::All => "startup-filter-all",
            StartupFilter::Enabled => "startup-filter-enabled",
            StartupFilter::Disabled => "startup-filter-disabled",
        }
    }

    /// Leading glyph per state. Enabled/Disabled get a clear on/off glyph (circled
    /// check / circled X); `All` stays text — there is no "all/every" semantic
    /// id, so forcing one would be misleading.
    fn icon(self) -> Option<IconId> {
        match self {
            StartupFilter::All => None,
            StartupFilter::Enabled => Some(IconId::CircleCheck),
            StartupFilter::Disabled => Some(IconId::CircleX),
        }
    }
}

/// Pure filter: by enabled state (`StartupFilter`) AND by case-insensitive substring of
/// name OR exec. Public so it can be unit-tested like `services_view::filter_services`.
/// The Startup-page row memo inputs: the filter+sort projection is a pure
/// function of (list generation, filter, query, inventory sort). The sort key
/// mirrors the shell-owned `InventorySorts` slot — this struct only caches the
/// projection, it never owns the "what column? what order?" answer.
/// Filter + order the Startup list once per (list, filter, query, sort)
/// change: status filter + search through [`filter_startup`], then the
/// shell-owned inventory ordering (`None` keeps provider order — the same
/// semantics `ShellApp::sorted_startup_entries` gives the shell track).
pub fn sorted_startup(
    entries: &[StartupEntry],
    filter: StartupFilter,
    query: &str,
    sort: Option<(InfoSortCol, SortDir)>,
) -> Vec<StartupEntry> {
    let mut filtered = filter_startup(entries, filter, query);
    taskmanager_shell::order_startup_rows(&mut filtered, sort);
    filtered
}

pub fn filter_startup(
    entries: &[StartupEntry],
    filter: StartupFilter,
    query: &str,
) -> Vec<StartupEntry> {
    let q = query.trim();
    entries
        .iter()
        .filter(|e| {
            filter.matches(e.enabled)
                && (q.is_empty()
                    || taskmanager_core::core::text::contains_ascii_ci(&e.name, q)
                    || taskmanager_core::core::text::contains_ascii_ci(&e.exec, q))
        })
        .cloned()
        .collect()
}

// ── persistent search-box Entity<InputState> ──────────────────────────────────
//
// `InputState::new` requires `&mut Window`, and the `Entity` it produces is `!Send`
// (FocusHandle inside). The UI is single-threaded, so a `thread_local` holds the entity
// across renders — exactly the pattern the services `TABLE` uses.

/// The search-box `Entity<TextInputState>`, owned per window on the
/// `RootView` that renders the Startup page (mirrors the Apps-page pattern —
/// a shared `thread_local` crosses window boundaries and re-enters
/// `root.update` from the Change subscription, panicking on real input).
pub(crate) fn init_search_entity(
    cx: &mut Context<RootView>,
) -> gpui::Entity<taskmanager_ui::inputs::text_input::TextInputState> {
    let entity = cx.new(|cx| {
        let mut state = taskmanager_ui::inputs::text_input::TextInputState::new(cx);
        state.set_placeholder(i18n::t("search.startup"), cx);
        state
    });
    crate::gpui_app::root::wire_debounced_search(&entity, cx, |rv, value| {
        rv.startup_state.query = value
    });
    entity
}

/// Focus the Startup search field from the shared command router. Returns
/// false before the page has rendered its input for the first time.
pub(crate) fn focus_search(
    view: &crate::gpui_app::root::RootView,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(input) = view.startup_search.as_ref() else {
        return false;
    };
    input.read(cx).focus_handle().clone().focus(window);
    true
}

// ── StartupDelegate + persistent Table entity ────────────────────────────────
//
// `render_startup` is a stateless free fn called fresh every render, but the
// `Entity<TableState<StartupDelegate>>` MUST persist across renders (TableState owns
// the scroll position, selection, focus handle, etc.). The RootView owns one such
// entity per window (field `RootView::startup_table`, created lazily on the first
// Startup render) — never a shared `thread_local`, which would cross window
// boundaries and leak scroll/sort/selection between windows.

/// Table delegate for the Startup list. Holds the per-render snapshot of rows + the
/// resolved `Theme` (so `render_td` can color the Status column) and a clone of the
/// RootView entity (so `render_tr`'s hover handler can publish the stable startup id
/// back into the root hover slot for the cursor tooltip).
/// `pub` because `RootView::startup_table` exposes the typed `TableState` entity;
/// the fields are private.
pub struct StartupDelegate {
    rows: Rc<Vec<StartupEntry>>,
    columns: Vec<TableColumn>,
    theme: Theme,
    root: Entity<RootView>,
    /// Live search query for name-cell highlighting (set per render).
    query: String,
}

impl StartupDelegate {
    /// Build a delegate with an empty row set, the fixed 5-column header
    /// (Status / Name / Impact / Source / Command), and the given theme +
    /// RootView handle. Row data is filled in per-render via
    /// [`StartupDelegate::set_data`].
    fn new(theme: Theme, root: Entity<RootView>) -> Self {
        Self {
            rows: Rc::new(Vec::new()),
            query: String::new(),
            // 5 columns: Status 80 / Name 280 / Impact 90 / Source 140 / Command 400.
            // Impact sits right after Name (where Win11 TM puts it) so the boot-cost
            // badge is visible without horizontal scrolling. Fixed widths (no native
            // flex column) — matching services_view. Status + Name carry the shared
            // inventory sort (header click routes through the shell-owned
            // `InventorySorts` slot).
            columns: vec![
                TableColumn::new("status", i18n::t("common.status"))
                    .width(px(80.0))
                    .sortable(),
                TableColumn::new("name", i18n::t("common.name"))
                    .width(px(280.0))
                    .sortable(),
                TableColumn::new("impact", i18n::t("startup.impact")).width(px(90.0)),
                TableColumn::new("source", i18n::t("startup.source")).width(px(140.0)),
                TableColumn::new("command", i18n::t("startup.command")).width(px(400.0)),
            ],
            theme,
            root,
        }
    }

    /// Map a table column index onto the shared inventory-sort vocabulary.
    /// `None` for columns that do not carry the interactive sort (Impact /
    /// Source / Command).
    pub(crate) fn info_sort_column(&self, col_ix: usize) -> Option<InfoSortCol> {
        match col_ix {
            0 => Some(InfoSortCol::Status),
            1 => Some(InfoSortCol::Name),
            _ => None,
        }
    }

    /// Replace the row projection + theme snapshot. The projection is already
    /// an `Rc` memo owned by `RootView`; pointer identity avoids comparing or
    /// cloning every entry on an unchanged frame.
    fn set_data(&mut self, rows: Rc<Vec<StartupEntry>>, theme: Theme, query: &str) {
        if !Rc::ptr_eq(&self.rows, &rows) {
            self.rows = rows;
        }
        self.theme = theme;
        if self.query != query {
            self.query = query.to_owned();
        }
    }
}

impl TableDelegate for StartupDelegate {
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

    /// Per-row container: keep the Table's default `div().id((_, ix))` shape (the
    /// framework chains its own `on_click` / hover / selection styling on top) and add
    /// an `on_hover` that publishes `Hover::Startup(id)` to RootView so the existing
    /// cursor-following tooltip (root.rs) keeps working.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let id = self.rows.get(row_ix).map(|entry| entry.id.clone());
        let root = self.root.clone();
        div()
            .id(("startup-row", row_ix))
            .debug_selector(move || format!("tm-startup-row:{row_ix}"))
            .on_hover(move |is_hov: &bool, _win, cx: &mut App| {
                if let Some(id) = &id {
                    root.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Startup(id.clone()))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                }
            })
    }

    /// Row context menu (Win11 parity): enable/disable the startup entry.
    /// Right-click selects the row first; actions route through the
    /// confirmation dialog exactly like the controls-row buttons.
    fn context_menu(
        &mut self,
        row_ix: usize,
        _menu: PopupMenuState,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenuState {
        let Some(entry) = self.rows.get(row_ix).cloned() else {
            return PopupMenuState::new(Vec::new(), cx);
        };
        let root = self.root.clone();
        let entry_for_select = entry.clone();
        root.update(cx, |v, cx| {
            v.selected_startup = Some(entry_for_select.id.clone());
            cx.notify();
        });
        let items = [true, false]
            .into_iter()
            .map(|enable| {
                let root = root.clone();
                let entry = entry.clone();
                MenuEntry::Item(MenuItem::new(
                    i18n::t(if enable {
                        "startup.enable"
                    } else {
                        "startup.disable"
                    }),
                    move |_, cx| {
                        root.update(cx, |v, cx| {
                            v.request_startup_control_confirmation(entry.clone(), enable);
                            cx.notify();
                        });
                    },
                ))
            })
            .collect();
        PopupMenuState::new(items, cx)
    }

    /// Render one cell. Col 0 = Status ("Enabled"/"Disabled", colored: Enabled →
    /// theme.disk, Disabled → theme.fg_dim); 1 = Name (theme.fg, truncate);
    /// 2 = Impact (High → theme.gpu, Medium → theme.disk, Low → theme.cpu,
    /// None → theme.fg_dim — mirrors how services_view colors its status badges);
    /// 3 = Source (theme.fg_dim); 4 = Command (theme.fg_dim, truncate).
    /// All at text_size 12 to match the services rows.
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
        let Some(e) = self.rows.get(row_ix) else {
            return div();
        };
        match col_ix {
            0 => {
                let color = if e.enabled { theme.disk } else { theme.fg_dim };
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                    .text_color(taskmanager_ui::theme_binding::hsla(color))
                    .child(if e.enabled {
                        i18n::t("common.enabled")
                    } else {
                        i18n::t("common.disabled")
                    })
            }
            1 => div()
                .flex()
                .min_w(px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(
                    crate::gpui_app::elements::highlighted_text(&e.name, &self.query, &self.theme),
                )),
            2 => {
                // Impact badge — colored like services status: High=error-red
                // (theme.gpu), Medium=amber (theme.disk), Low=blue (theme.cpu),
                // None=dim.
                let color = match e.impact {
                    StartupImpact::High => theme.gpu,
                    StartupImpact::Medium => theme.disk,
                    StartupImpact::Low => theme.cpu,
                    StartupImpact::None => theme.fg_dim,
                };
                let label = match e.impact_evidence {
                    StartupImpactEvidence::Measured { duration_ms } => {
                        format!("{} · {duration_ms} ms", i18n::t(e.impact.i18n_key()))
                    }
                    StartupImpactEvidence::Unknown { .. } => {
                        i18n::t("startup.impact_unknown").to_string()
                    }
                };
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                    .text_color(taskmanager_ui::theme_binding::hsla(color))
                    .child(label)
            }
            3 => div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(e.source.as_str().to_string()),
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
                        .child(e.exec.clone()),
                ),
        }
    }
}

/// Create the persistent `Entity<TableState<StartupDelegate>>` and wire its
/// `SelectRow` / `SortChanged` events back into RootView. Called at most once
/// per window via `RootView::startup_table`'s `get_or_insert_with`.
pub(crate) fn init_table_entity(
    theme: Theme,
    cx: &mut Context<RootView>,
) -> Entity<TableState<StartupDelegate>> {
    let delegate = StartupDelegate::new(theme, cx.entity());
    let entity = cx.new(|new_cx| TableState::new(delegate, new_cx).row_selectable(true));
    // Table → RootView bridges. SelectRow: clicking a row publishes the
    // provider-issued entry identity at that index to RootView. SortChanged: a
    // header sort click reports the widget's absolute post-cycle state, which
    // is applied VERBATIM to the shell-owned inventory-sort slot (single
    // authority) — the memo keys on it, so the row order follows on the next
    // projection build.
    cx.subscribe(&entity, |this, state_ent, ev: &TableEvent, cx| match ev {
        TableEvent::SelectRow(row_ix) => {
            if let Some(entry) = state_ent.read(cx).delegate().rows.get(*row_ix) {
                this.selected_startup = Some(entry.id.clone());
                cx.notify();
            }
        }
        TableEvent::SortChanged { col_ix, sort } => {
            let column = state_ent.read(cx).delegate().info_sort_column(*col_ix);
            this.apply_table_sort(taskmanager_shell::InfoTable::Startup, column, *sort);
            cx.notify();
        }
        _ => {}
    })
    .detach();
    entity
}

// ── top-level render (signature mirrors render_services) ─────────────────────

/// All straight-through render inputs for the Startup page, consolidated into
/// one borrow-carrying props value (design-debt #1: builder/props instead of
/// a bare 13-argument render function). The two gpui context arguments
/// (`window` / `cx`) stay explicit because they are render-lifetime handles,
/// not view state.
pub struct StartupViewProps<'a> {
    pub theme: &'a Theme,
    pub entries: &'a [StartupEntry],
    pub sources: &'a [SourceStatus],
    pub selected: Option<&'a StartupEntryId>,
    pub hovered: Option<Hover>,
    pub filter: StartupFilter,
    pub query: &'a str,
    /// Memoized filter+sort projection (see `RootView::startup_rows`);
    /// `filter`/`query` stay for the controls row, the table body reads this.
    pub rows: std::rc::Rc<Vec<StartupEntry>>,
    /// The previous boot's waterfall (opt-in boot history, roadmap #5) —
    /// `None` without persistence or before any comparison exists, in which
    /// case the waterfall renders exactly as before.
    pub boot_baseline: Option<&'a taskmanager_core::core::BootTimeline>,
    pub feedback: Option<ActionFeedback>,
    pub search_input: gpui::Entity<taskmanager_ui::inputs::text_input::TextInputState>,
    pub table_entity: Entity<TableState<StartupDelegate>>,
    pub evidence: Option<&'a StartupBootEvidenceSnapshot>,
    pub retry_button: Entity<ButtonState>,
    /// Page allocation derived once from the shared viewport profile.
    pub layout: StartupPageBudget,
}

/// Render the Startup page: action bar (Enable / Disable) over a status-filter
/// pill row + a search box + a virtualized autostart-entry Table. Mirrors
/// [`crate::gpui_app::services_view::render_services`] 1:1 in structure.
///
/// `entries` is the full materialized Startup snapshot — this fn filters
/// a copy (status filter + search) and orders it through the shell-owned
/// inventory sort. `selected` is the live provider-issued `RootView.selected_startup`, mapped
/// back to a row index so the Table's selection stays in sync. `filter` /
/// `query` are the status-filter + search-box values owned by
/// `RootView.startup_state`; `feedback` is the render-time fold of the
/// shell-owned typed Startup outcome (`RootView::startup_feedback`).
/// `hovered` is the uniform hover slot root passes to every page render
/// (consumed by the action-bar buttons' hover overlays).
pub fn render_startup(
    props: StartupViewProps<'_>,
    _window: &mut Window,
    cx: &mut Context<RootView>,
) -> Div {
    let StartupViewProps {
        theme,
        entries,
        sources,
        selected,
        hovered,
        filter,
        query,
        rows,
        boot_baseline,
        feedback,
        search_input,
        table_entity,
        evidence,
        retry_button,
        layout,
    } = props;
    let theme = *theme;
    let selected = selected.cloned();

    // The persistent Table entity, created lazily per window by `RootView::render`
    // (RootView::startup_table) on the first Startup render; reused after.
    let search_entity = search_input;

    // The filter+sort projection arrived pre-computed and memoized on the
    // caller (`RootView::startup_rows`); same order as before — enabled-first,
    // then case-insensitive name — now computed once per (list, filter,
    // query) change instead of once per frame.
    let sorted = rows;

    // Map RootView.selected_startup → row index in `sorted` (None if filtered out).
    let selected_row_ix = selected
        .as_ref()
        .and_then(|id| sorted.iter().position(|entry| &entry.id == id));

    let header = div()
        .debug_selector(|| "tm-startup-chrome".to_string())
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(action_bar(
            &theme,
            entries,
            selected.as_ref(),
            hovered.as_ref(),
            feedback,
            cx,
        ))
        .child(controls_row(
            &theme,
            filter,
            hovered.as_ref(),
            &search_entity,
            &cx.entity(),
        ))
        .child(boot_evidence_strip(&theme, evidence));
    let body = if sorted.is_empty() {
        // EMPTY-STATE: 0 rows → a centered hint. An empty list caused by a
        // failed source (systemctl missing / init not recognized) shows the
        // typed reason instead of "No startup entries".
        list_view::unavailable_source_state(
            &theme,
            sources,
            !query.trim().is_empty(),
            RefreshRequest::Startup,
            &retry_button,
            &cx.entity(),
        )
        .unwrap_or_else(|| list_view::empty_state(&theme, i18n::t("startup.noun"), query))
    } else {
        let ent = table_entity;
        // Push the new row data + theme, then sync selection RootView → Table.
        // Only (re)set when the desired row actually differs to avoid spurious
        // SelectRow emits (and thus extra frames) every render.
        ent.update(cx, |s, cx| {
            s.delegate_mut()
                .set_data(Rc::clone(&sorted), theme, query.trim());
            if s.selected_row() != selected_row_ix {
                match selected_row_ix {
                    Some(ix) => s.set_selected_row(ix, cx),
                    None => s.clear_selection(cx),
                }
            }
        });
        // Table is size_full internally; wrap so it expands to fill the remaining
        // vertical space below the action bar + controls row.
        let mut primary = div()
            .debug_selector(|| "tm-startup-primary".to_string())
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ));
        let source_detail = startup_source_detail(sources);
        if let Some(notice) = list_view::source_notice_with_detail_presentation(
            &theme,
            sources,
            RefreshRequest::Startup,
            &retry_button,
            &cx.entity(),
            source_detail.as_deref(),
            layout.source_notice,
        ) {
            primary = primary.child(
                div()
                    .debug_selector(|| "tm-startup-source-notice".to_string())
                    .flex_shrink_0()
                    .child(notice),
            );
        }
        let primary = primary.child(
            div()
                .debug_selector(|| "tm-startup-primary-table".to_string())
                .flex_1()
                .min_h(px(layout.table_min_height))
                .child(Table::new(&ent, theme.palette()).bordered(true)),
        );

        layout::compose_content(&theme, evidence, boot_baseline, layout, primary)
    };

    list_view::ListPageScaffold::new(header, body).render()
}

// ── controls row: status-filter pills (left) + search box (right) ────────────

/// Layout the controls row: status-filter pills (left), a flex spacer, then the
/// search box (right). The pill cluster + the box come from [`list_view`]; only
/// the per-view `on_select` (which `RootView` field the chosen filter writes
/// into) is wired here.
fn controls_row(
    theme: &Theme,
    filter: StartupFilter,
    hovered: Option<&Hover>,
    search_entity: &Entity<TextInputState>,
    entity: &Entity<RootView>,
) -> Div {
    Toolbar::new()
        // Status-filter pill cluster + the search box both come from [`list_view`]
        // (shared with services_view); only the per-view `on_select` — which
        // `RootView` field the chosen filter writes into — is wired here.
        .child(list_view::filter_pill_row(
            theme,
            &StartupFilter::ALL,
            filter,
            hovered,
            entity,
            |f, v| v.startup_state.filter = f,
        ))
        // Flex spacer: pushes the search box to the right edge.
        .child(div().flex_1())
        .child(list_view::search_box(&theme.palette(), search_entity))
        .render()
}

// ── action bar (Enable / Disable) ────────────────────────────────────────────

/// Build the Startup action bar: Enable / Disable, each inert until a row is
/// selected, plus a trailing status line. Each button queues a platform-neutral
/// request; the typed worker result lands in the shell-owned feedback slot and
/// a successful result queues a startup-only refresh. The status line renders
/// that feedback (or a selection hint) via the shared
/// [`list_view::feedback_status_line`].
fn action_bar(
    theme: &Theme,
    entries: &[StartupEntry],
    selected: Option<&StartupEntryId>,
    hovered: Option<&Hover>,
    feedback: Option<ActionFeedback>,
    cx: &mut Context<RootView>,
) -> Div {
    // Transient action feedback (folded at render time from the shell-owned
    // typed outcome by `RootView::startup_feedback`); when present it takes the
    // status line over the selection hint.
    let selected_entry = selected.and_then(|id| entries.iter().find(|entry| &entry.id == id));
    let hint = match selected_entry {
        Some(entry) => format!("{} {}", i18n::t("hint.selected"), entry.name),
        None => i18n::t("hint.select_startup").to_string(),
    };

    // Each action closure gets its own owned snapshot of `entries` to find the selected
    // entry by stable identity (the snapshot is at most
    // one frame stale since action_bar is rebuilt every render).
    let entries_enable: Vec<StartupEntry> = entries.to_vec();
    let entries_disable: Vec<StartupEntry> = entries.to_vec();
    let can_enable = selected_entry.is_some_and(|entry| {
        !entry.enabled && entry.control_policy != StartupControlPolicy::Unsupported
    });
    let can_disable = selected_entry.is_some_and(|entry| {
        entry.enabled && entry.control_policy != StartupControlPolicy::Unsupported
    });
    // Bind the RootView entity once; each tool_btn clones it for its on_click/on_hover
    // closures — the same entity-binding pattern `filter_pill` uses for the status pills.
    let entity = cx.entity();

    Toolbar::new()
        // Enable: built on `elements::tool_btn` (the unified action-button primitive
        // shared across views). Disabled (no selection) → tool_btn renders inert
        // (dimmed, no hover overlay, no click). `on_click`/`on_hover` bind the entity
        // via `ent.update(...)` so the action body runs under `&mut RootView` +
        // `&mut Context<RootView>` exactly as the old local `action_btn` did via
        // `cx.listener`. The `Hover::Static("Enable")` discriminator is reused (only
        // one page is visible at a time, so it never collides with services' ids).
        .child(elements::tool_btn(
            theme,
            "Enable",
            i18n::t("common.enable"),
            can_enable,
            hovered == Some(&Hover::Static("Enable")),
            {
                let ent = entity.clone();
                move |_win: &mut Window, cx: &mut App| {
                    ent.update(cx, |v, cx| {
                        if let Some(id) = v.selected_startup.clone()
                            && let Some(entry) = entries_enable.iter().find(|entry| entry.id == id)
                        {
                            v.request_startup_control_confirmation(entry.clone(), true);
                            cx.notify();
                        }
                    });
                }
            },
            {
                let ent = entity.clone();
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static("Enable"))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                }
            },
        ))
        // Disable: same shape as Enable; only the action + hover discriminator differ.
        .child(elements::tool_btn(
            theme,
            "Disable",
            i18n::t("common.disable"),
            can_disable,
            hovered == Some(&Hover::Static("Disable")),
            {
                let ent = entity.clone();
                move |_win: &mut Window, cx: &mut App| {
                    ent.update(cx, |v, cx| {
                        if let Some(id) = v.selected_startup.clone()
                            && let Some(entry) = entries_disable.iter().find(|entry| entry.id == id)
                        {
                            v.request_startup_control_confirmation(entry.clone(), false);
                            cx.notify();
                        }
                    });
                }
            },
            {
                let ent = entity.clone();
                move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
                    ent.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static("Disable"))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                }
            },
        ))
        .child(list_view::feedback_status_line(
            theme,
            feedback.as_ref(),
            &hint,
        ))
        .render()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_startup_view_row_memo_tests.rs"]
mod row_memo_tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_startup_view_layout_geometry_tests.rs"]
mod layout_geometry_tests;
