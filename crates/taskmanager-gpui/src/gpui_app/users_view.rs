//! Users view: one row per **login session** (`SessionItem`) — the Win11 TM /
//! Mission Centre model — driven by the platform session ports. Not a per-user
//! rollup.
//!
//! Columns: Session(120) / User(140) / Seat(80) / TTY(80) / Remote(70) /
//! Logon(140). An action bar offers Disconnect (`terminate-session`) and Lock
//! (`lock-session`), disabled when no row is selected; transient feedback
//! (success/failure) lands in a status line, reusing the shared
//! [`crate::gpui_app::list_view::ActionFeedback`] / `feedback_status_line`
//! scaffolding (same type services/startup use). Empty-state reads "No
//! sessions" when `scan()` yields nothing.
//!
//! **State ownership:** each window's `RootView.users_table` owns the persistent
//! `taskmanager_ui::data::table::Table` entity (scroll position / selection /
//! focus handle); no process-global or thread-local widget state exists.
//! Per-page state (selected session id + action feedback) lives on
//! `RootView` (`selected_session` / the shell-owned typed feedback slot folded
//! at render by `RootView::session_feedback`), threaded into
//! [`render_users`] each render; the row projection is memoized on the
//! RootView's session snapshot generation + the shell-owned inventory sort,
//! and the per-cell
//! telemetry→display fold lives in the data-layer `row_vm` module (ARCH.md
//! §8.1) — `render_td` only styles and paints the pre-folded strings.

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
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_shell::InfoSortCol;
use taskmanager_ui::data::table::{Table, TableColumn, TableDelegate, TableEvent, TableState};
use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use taskmanager_ui::primitives::button::ButtonState;
use taskmanager_ui::primitives::toolbar::Toolbar;
use taskmanager_ui_contract::IconId;

// ── transient action feedback (shared with services/startup) ─────────────────
//
// Re-exported so `root.rs`'s `users_view::ActionFeedback` state path keeps
// resolving unchanged. The feedback payload + its `from_result` constructor +
// the `feedback_status_line` renderer all live in `list_view` now; the only
// Users-specific bit is the `name` passed to `from_result` — `format!("session
// {}", id)` so the rendered text reads "Disconnect: session <id> succeeded"
// (preserving the prior local `make_feedback` wording exactly).

pub use crate::gpui_app::list_view::ActionFeedback;

mod row_vm;

use row_vm::{UserRowVm, user_row_vm};

// ── UsersDelegate + persistent Table entity ──────────────────────────────────

/// Table delegate for the sessions list. Holds the per-render snapshot of rows +
/// the resolved `Theme` (so `render_td` colors cells) and a clone of the
/// RootView entity (so `render_tr`'s hover handler can publish `Hover::User(name)`
/// back into the root hover slot for the cursor tooltip — the existing variant
/// is reused to avoid touching the `Hover` enum).
pub struct UsersDelegate {
    rows: Rc<Vec<SessionItem>>,
    /// Pre-folded cell strings for `rows`, rebuilt only when the row set
    /// actually swaps (see [`UsersDelegate::set_data`]) — `render_td` paints
    /// these instead of re-running the per-cell folds every frame.
    vms: Rc<Vec<UserRowVm>>,
    columns: Vec<TableColumn>,
    theme: Theme,
    root: Entity<RootView>,
    /// Live search query for name-cell highlighting (set per render).
    query: String,
}

impl UsersDelegate {
    /// Build a delegate with an empty row set, the fixed 6-column header
    /// (Session / User / Seat / TTY / Remote / Logon), and the given theme +
    /// RootView handle. Row data is filled in per-render via
    /// [`UsersDelegate::set_data`].
    fn new(theme: Theme, root: Entity<RootView>) -> Self {
        Self {
            rows: Rc::new(Vec::new()),
            vms: Rc::new(Vec::new()),
            query: String::new(),
            // 6 columns per spec: Session 120 / User 140 / Seat 80 / TTY 80 /
            // Remote 70 / Logon 140. Fixed widths match services/startup (the
            // Table has no native flex column). Session / User / Seat carry
            // the shared inventory sort (header click routes through the
            // shell-owned `InventorySorts` slot).
            columns: vec![
                TableColumn::new("session", i18n::t("users.session"))
                    .width(px(120.0))
                    .sortable(),
                TableColumn::new("user", i18n::t("common.user"))
                    .width(px(140.0))
                    .sortable(),
                TableColumn::new("seat", i18n::t("users.seat"))
                    .width(px(80.0))
                    .sortable(),
                TableColumn::new("tty", i18n::t("users.tty")).width(px(80.0)),
                TableColumn::new("remote", i18n::t("users.remote")).width(px(70.0)),
                TableColumn::new("logon", i18n::t("users.logon")).width(px(140.0)),
            ],
            theme,
            root,
        }
    }

    /// Map a table column index onto the shared inventory-sort vocabulary.
    /// `None` for columns that do not carry the interactive sort (TTY /
    /// Remote / Logon).
    pub(crate) fn info_sort_column(&self, col_ix: usize) -> Option<InfoSortCol> {
        match col_ix {
            0 => Some(InfoSortCol::Session),
            1 => Some(InfoSortCol::Name),
            2 => Some(InfoSortCol::Seat),
            _ => None,
        }
    }

    /// Replace the row projection + theme snapshot. `RootView` memoizes the
    /// provider-order session vector by snapshot generation, so an unchanged
    /// frame only clones an `Rc` handle and never copies the rows. The
    /// telemetry→display fold ([`user_row_vm`]) runs once per actual swap —
    /// `render_td` replays the pre-folded strings every frame.
    fn set_data(&mut self, rows: Rc<Vec<SessionItem>>, theme: Theme, query: &str) {
        if !Rc::ptr_eq(&self.rows, &rows) {
            self.vms = Rc::new(rows.iter().map(user_row_vm).collect());
            self.rows = rows;
        }
        self.theme = theme;
        if self.query != query {
            self.query = query.to_owned();
        }
    }
}

impl TableDelegate for UsersDelegate {
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
    /// framework chains its own `on_click` / hover / selection styling on top) and
    /// add an `on_hover` that publishes `Hover::User(user)` to RootView so the
    /// existing cursor-following tooltip (root.rs) keeps working. The user name
    /// (not session id) is published so the tooltip reads as an owner label.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let user = self.rows.get(row_ix).map(|s| s.user.clone());
        let root = self.root.clone();
        div()
            .id(("session-row", row_ix))
            .on_hover(move |is_hov: &bool, _win, cx: &mut App| {
                if let Some(n) = &user {
                    root.update(cx, |v, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::User(n.clone()))
                            } else {
                                None
                            },
                            cx,
                        );
                    });
                }
            })
    }

    /// Row context menu (Win11 parity): disconnect / lock the session.
    /// Right-click selects the row first; both actions are direct (no
    /// confirmation dialog), mirroring the session-control buttons.
    fn context_menu(
        &mut self,
        row_ix: usize,
        _menu: PopupMenuState,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenuState {
        let Some(session) = self.rows.get(row_ix).cloned() else {
            return PopupMenuState::new(Vec::new(), cx);
        };
        let root = self.root.clone();
        let session_id = session.id.clone();
        root.update(cx, |v, cx| {
            v.selected_session = Some(session_id.clone());
            cx.notify();
        });
        let items = [
            (
                i18n::t("users.disconnect"),
                SessionControlAction::Disconnect,
            ),
            (i18n::t("users.lock"), SessionControlAction::Lock),
        ]
        .into_iter()
        .map(|(label, action)| {
            let root = root.clone();
            let session_id = session_id.clone();
            MenuEntry::Item(MenuItem::new(label, move |_, cx| {
                root.update(cx, |v, cx| {
                    v.request_session_control(session_id.clone(), action);
                    cx.notify();
                });
            }))
        })
        .collect();
        PopupMenuState::new(items, cx)
    }

    /// Render one cell from the pre-folded `UserRowVm` strings (built once
    /// per data swap) — this method only styles and paints:
    /// - 0 Session (theme.fg), 1 User (theme.fg + search highlight), 2 Seat
    ///   (theme.fg_dim), 3 TTY (theme.fg_dim), 4 Remote (theme.cpu when remote
    ///   else theme.fg_dim), 5 Logon (theme.fg_dim).
    ///
    /// The dash placeholders for missing Seat / TTY / Logon and the localized
    /// yes/no are folded into the VM; the remote COLOR is keyed on the source
    /// row's `remote` bool here (color is render-layer). text_size 12 matches
    /// the other list rows.
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
        // panicking (render paths can't return Result — the table re-syncs next
        // frame). `vms` is rebuilt in the same swap, so it stays length-paired
        // with `rows`.
        let Some(vm) = self.vms.get(row_ix) else {
            return div();
        };
        let remote = self.rows.get(row_ix).is_some_and(|s| s.remote);
        match col_ix {
            0 => div()
                .flex()
                .min_w(px(0.0))
                .text_size(tokens::FONT_12)
                .text_color(theme.fg)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(vm.session.clone()),
                ),
            1 => {
                div()
                    .flex()
                    .min_w(px(0.0))
                    .text_size(tokens::FONT_12)
                    .text_color(theme.fg)
                    .child(div().flex_1().min_w(px(0.0)).truncate().child(
                        elements::highlighted_text(&vm.user, &self.query, &self.theme),
                    ))
            }
            2 => div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(vm.seat.clone()),
            3 => div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(vm.tty.clone()),
            // Remote: accent CPU token when remote (so it stands out), else dim.
            4 => {
                let color = if remote { theme.cpu } else { theme.fg_dim };
                div()
                    .text_size(tokens::FONT_12)
                    .text_color(color)
                    .child(vm.remote_label)
            }
            _ => div()
                .flex()
                .min_w(px(0.0))
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .child(vm.logon.clone()),
                ),
        }
    }
}

/// Create the persistent `Entity<TableState<UsersDelegate>>` and wire its
/// `SelectRow` / `SortChanged` events back into `RootView.selected_session` /
/// the shell-owned inventory sorts. Called exactly once per window through
/// `RootView.users_table.get_or_insert_with`.
pub(crate) fn init_table_entity(
    theme: Theme,
    cx: &mut Context<RootView>,
) -> Entity<TableState<UsersDelegate>> {
    let delegate = UsersDelegate::new(theme, cx.entity());
    let entity = cx.new(|new_cx| TableState::new(delegate, new_cx).row_selectable(true));
    // Table → RootView bridges. SelectRow: clicking a row looks up the session
    // id at that index and publishes it to selected_session. SortChanged: a
    // header sort click reports the widget's absolute post-cycle state, which
    // is applied VERBATIM to the shell-owned inventory-sort slot (single
    // authority) — the memo keys on it, so the row order follows on the next
    // projection build.
    cx.subscribe(&entity, |this, state_ent, ev: &TableEvent, cx| match ev {
        TableEvent::SelectRow(row_ix) => {
            if let Some(s) = state_ent.read(cx).delegate().rows.get(*row_ix) {
                this.selected_session = Some(s.id.clone());
                cx.notify();
            }
        }
        TableEvent::SortChanged { col_ix, sort } => {
            let column = state_ent.read(cx).delegate().info_sort_column(*col_ix);
            this.apply_table_sort(taskmanager_shell::InfoTable::Users, column, *sort);
            cx.notify();
        }
        _ => {}
    })
    .detach();
    entity
}

// ── top-level render (signature mirrors render_services / render_startup) ────

/// Render the Users (sessions) page: action bar + sessions Table.
///
/// `selected` is the live `RootView.selected_session` (session id). `feedback`
/// is the render-time fold of the shell-owned typed session outcome
/// (`RootView::session_feedback`). `hovered` is the uniform hover slot root
/// passes to every page render (consumed by the action bar's hover overlays).
/// All straight-through users render inputs (design-debt #1 props
/// consolidation); `window`/`cx` stay explicit render-lifetime handles.
pub struct UsersViewProps<'a> {
    pub theme: &'a Theme,
    pub rows: Rc<Vec<SessionItem>>,
    pub sources: &'a [SourceStatus],
    pub selected: Option<&'a str>,
    pub feedback: Option<ActionFeedback>,
    pub hovered: Option<Hover>,
    pub search_query: &'a str,
    pub table_entity: &'a Entity<TableState<UsersDelegate>>,
    pub retry_button: Entity<ButtonState>,
}

pub fn render_users(
    props: UsersViewProps<'_>,
    _window: &mut Window,
    cx: &mut Context<RootView>,
) -> Div {
    let UsersViewProps {
        theme,
        rows,
        sources,
        selected,
        feedback,
        hovered,
        search_query,
        table_entity,
        retry_button,
    } = props;
    let theme = *theme;
    let selected = selected.map(|s| s.to_string());

    let count = rows.len();

    // Map RootView.selected_session → row index in `rows` (None if absent).
    let selected_row_ix = selected
        .as_ref()
        .and_then(|id| rows.iter().position(|s| &s.id == id));

    let header = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(action_bar(
            &theme,
            selected.as_deref(),
            hovered.as_ref(),
            feedback,
            cx,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_6)
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(taskmanager_icons::icon(IconId::Users).size(px(14.0)))
                .child(format!("{} {}", count, i18n::t("users.sessions"))),
        );
    let body = if rows.is_empty() {
        // EMPTY-STATE: no sessions → centered hint. An empty list caused by
        // a failed source (loginctl missing) shows the typed reason instead
        // of "No sessions".
        list_view::unavailable_source_state(
            &theme,
            sources,
            false,
            RefreshRequest::Sessions,
            &retry_button,
            &cx.entity(),
        )
        .unwrap_or_else(|| list_view::empty_state(&theme, i18n::t("users.sessions"), ""))
    } else {
        let ent = &table_entity;
        {
            // Push the new row data + theme, then sync selection RootView → Table.
            // Only (re)set when the desired row actually differs to avoid spurious
            // SelectRow emits (and thus extra frames) every render.
            ent.update(cx, |s, cx| {
                s.delegate_mut()
                    .set_data(Rc::clone(&rows), theme, search_query.trim());
                if s.selected_row() != selected_row_ix {
                    match selected_row_ix {
                        Some(ix) => s.set_selected_row(ix, cx),
                        None => s.clear_selection(cx),
                    }
                }
            });
            // Table is size_full internally; wrap so it expands to fill the
            // remaining vertical space below the action bar + count header.
            let mut body = div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .gap(tokens::SPACE_8);
            if let Some(notice) = list_view::source_notice(
                &theme,
                sources,
                RefreshRequest::Sessions,
                &retry_button,
                &cx.entity(),
            ) {
                body = body.child(notice);
            }
            body.child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(Table::new(ent, theme.palette()).bordered(true)),
            )
            .into_any_element()
        }
    };

    list_view::ListPageScaffold::new(header, body).render()
}

// ── action bar (Disconnect / Lock) ───────────────────────────────────────────

/// Build the Users action bar: Disconnect (`terminate-session`) / Lock
/// (`lock-session`), each inert until a row is selected, plus a trailing status
/// line. Disconnect queues a background session refresh, which clears a stale
/// selection when its snapshot arrives; Lock leaves the session listed. Both
/// land their typed outcome in the shell-owned feedback slot, folded at render
/// by `RootView::session_feedback` and rendered (or falling back to a selection
/// hint) via the shared [`list_view::feedback_status_line`]. Both actions are
/// queued through the application port and never invoke native session tools
/// on the UI thread.
fn action_bar(
    theme: &Theme,
    selected: Option<&str>,
    hovered: Option<&Hover>,
    feedback: Option<ActionFeedback>,
    cx: &mut Context<RootView>,
) -> Div {
    // Selection hint — the fallback status line when no action feedback is
    // present. The feedback itself (success/failure color + text) is rendered
    // by the shared `list_view::feedback_status_line`, identical to the
    // services/startup action bars.
    let hint = match selected {
        Some(id) => format!("{} {}", i18n::t("hint.selected_session"), id),
        None => i18n::t("hint.select_session").to_string(),
    };
    // RootView entity, cloned into each action button's on_click / on_hover so
    // `elements::tool_btn` stays decoupled from `root` — the same entity-binding
    // pattern services_view uses.
    let entity = cx.entity();

    Toolbar::new()
        // Submit disconnect through the native session-control port. Queue a
        // refresh so the terminated session disappears without scanning here.
        .child({
            let label = i18n::t("users.disconnect");
            let enabled = selected.is_some();
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                label,
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(id) = v.selected_session.clone() {
                            v.request_session_control(id, SessionControlAction::Disconnect);
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
        // Submit lock through the native session-control port. The session
        // remains listed after locking.
        .child({
            let label = i18n::t("users.lock");
            let enabled = selected.is_some();
            let hovered_bool = enabled && hovered == Some(&Hover::Static(label));
            let ent_c = entity.clone();
            let ent_h = entity.clone();
            elements::tool_btn(
                theme,
                label,
                label,
                enabled,
                hovered_bool,
                move |_win: &mut Window, cx: &mut App| {
                    ent_c.update(cx, |v, cx| {
                        if let Some(id) = v.selected_session.clone() {
                            v.request_session_control(id, SessionControlAction::Lock);
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
        .render()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_users_view_row_memo_tests.rs"]
mod row_memo_tests;
