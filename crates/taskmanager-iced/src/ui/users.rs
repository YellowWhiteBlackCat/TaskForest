//! Iced Users page projection.
//!
//! This module renders one row per login session, matching the existing GPUI
//! Users vocabulary while routing session control through the shared shell
//! effect boundary.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};
use std::rc::Rc;
// Shared locale catalog for the Users-page body chrome (column headers, action
// buttons, Yes/No) that was previously hard-coded English.
use taskmanager_application::i18n::t;
use taskmanager_application::{RefreshRequest, SessionControlAction};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::missing_value;

use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_theme::{Theme, tokens};

use super::tables::{
    InventoryTableKey, info_header_cell, inventory_row_height, inventory_table_key,
    plain_header_cell,
};
use super::tables::{ListState, message_panel, source_notice_banner, source_state_panel};
use super::virtual_list::{ColumnWidth, TableColumn};
use super::{
    VIRTUAL_TABLE_HEADER_HEIGHT, VirtualWindow, virtual_table, virtual_table_body,
    virtual_table_key, virtual_table_row,
};

/// Typed column specs for the Users table — the same vocabulary as the
/// process-column contract (id/label/width/alignment), page-owned because
/// session-table semantics have no cross-frontend contract. The header row
/// and every body row read the SAME specs, which keeps the sticky header
/// pixel-aligned over its columns; the logon column is the flexible
/// remainder.
#[derive(Clone, Copy)]
pub(super) struct UsersColumns {
    pub(super) session: TableColumn,
    pub(super) name: TableColumn,
    pub(super) seat: TableColumn,
    pub(super) tty: TableColumn,
    pub(super) remote: TableColumn,
    pub(super) logon: TableColumn,
}

pub(super) fn users_columns() -> UsersColumns {
    UsersColumns {
        session: TableColumn::text(
            "Session",
            taskmanager_shell::InfoSortCol::Session.label(),
            ColumnWidth::Fixed(120.0),
        ),
        name: TableColumn::text(
            "Name",
            taskmanager_shell::InfoSortCol::Name.label(),
            ColumnWidth::Fixed(170.0),
        ),
        seat: TableColumn::text(
            "Seat",
            taskmanager_shell::InfoSortCol::Seat.label(),
            ColumnWidth::Fixed(100.0),
        ),
        tty: TableColumn::text("Tty", "users.tty", ColumnWidth::Fixed(100.0)),
        remote: TableColumn::text("Remote", "users.remote", ColumnWidth::Fixed(90.0)),
        logon: TableColumn::text("Logon", "users.logon", ColumnWidth::Fill),
    }
}

/// Render the Users page from the shared session inventory.
pub(crate) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let (rows, _visible_indices, projection_generation) = app.users_projection();
    let row_count = rows.len();
    let list_state = user_list_state(shell);
    let compact = app.compact_density();
    let row_padding = crate::theme::row_padding(compact);

    let body: Element<'_, Message, iced::Theme, iced::Renderer> = match list_state {
        ListState::Loading => message_panel(theme_snapshot, t("users.inventory_waiting")),
        ListState::Empty => source_state_panel(
            theme_snapshot,
            shell.projection().sessions_source.as_deref(),
            RefreshRequest::Sessions,
        )
        .unwrap_or_else(|| message_panel(theme_snapshot, t("users.no_sessions"))),
        ListState::Ready => {
            // Clickable sort headers: the active column wears ▲/▼, clicks
            // route to the shell's shared per-table sort slot. Widths come
            // from the typed column specs the body cells also read.
            let columns = users_columns();
            let header = container(
                row![
                    info_header_cell(
                        theme_snapshot,
                        taskmanager_shell::InfoTable::Users,
                        taskmanager_shell::InfoSortCol::Session,
                        shell.sessions_sort,
                        columns.session.length(),
                    ),
                    info_header_cell(
                        theme_snapshot,
                        taskmanager_shell::InfoTable::Users,
                        taskmanager_shell::InfoSortCol::Name,
                        shell.sessions_sort,
                        columns.name.length(),
                    ),
                    info_header_cell(
                        theme_snapshot,
                        taskmanager_shell::InfoTable::Users,
                        taskmanager_shell::InfoSortCol::Seat,
                        shell.sessions_sort,
                        columns.seat.length(),
                    ),
                    plain_header_cell(&columns.tty),
                    plain_header_cell(&columns.remote),
                    plain_header_cell(&columns.logon),
                ]
                .spacing(8)
                .padding(4)
                .width(Length::Fill),
            )
            .height(Length::Fixed(VIRTUAL_TABLE_HEADER_HEIGHT))
            .width(Length::Fill);

            let (scroll_y, viewport_height) = app.users_virtual_scroll();
            // Sticky header: the header row sits outside the body scrollable,
            // so the body window carries no header prefix.
            let window = VirtualWindow::for_sticky_rows(
                rows.len(),
                scroll_y,
                viewport_height,
                inventory_row_height(compact),
            );
            let table_theme = *theme_snapshot;
            let selected = shell.selected;
            let base_key = inventory_table_key(InventoryTableKey {
                theme_snapshot,
                generation: projection_generation,
                table: taskmanager_shell::InfoTable::Users,
                sort: shell.sessions_sort,
                query: "",
                search_active: false,
                selected,
                row_count: rows.len(),
                compact,
            });
            let body_rows = Rc::clone(&rows);
            let columns = users_columns();
            let table_body = iced::widget::lazy(virtual_table_key(base_key, window), move |_| {
                let rows = Rc::clone(&body_rows);
                virtual_table_body(window, Length::Fill, move |start, end| {
                    rows.get(start..end)
                        .unwrap_or(&[])
                        .iter()
                        .enumerate()
                        .map(|(offset, session)| {
                            let index = start + offset;
                            let is_selected = index == selected;
                            let zebra = crate::theme::zebra_index(index);
                            let remote_cell: Element<
                                'static,
                                Message,
                                iced::Theme,
                                iced::Renderer,
                            > = {
                                let mut cell = text(remote_text(session.remote))
                                    .width(columns.remote.length());
                                if session.remote {
                                    cell = cell
                                        .color(crate::theme::color(table_theme.palette().accent));
                                }
                                cell.into()
                            };
                            let row = row![
                                text(session.id.clone()).width(columns.session.length()),
                                text(session.user.clone()).width(columns.name.length()),
                                text(optional_text(session.seat.as_deref()))
                                    .width(columns.seat.length()),
                                text(optional_text(session.tty.as_deref()))
                                    .width(columns.tty.length()),
                                remote_cell,
                                text(optional_text(session.timestamp.as_deref()))
                                    .width(columns.logon.length()),
                            ]
                            .spacing(8)
                            .padding(row_padding)
                            .width(Length::Fill);
                            let row = focus::selectable_row_with_menu(
                                &table_theme,
                                taskmanager_application::AppPage::Users,
                                index,
                                container(row)
                                    .style(move |_| {
                                        theme::row_style(&table_theme, is_selected, zebra)
                                    })
                                    .width(Length::Fill)
                                    .into(),
                                Message::OpenUserRowMenu(index),
                            );
                            virtual_table_row(row, inventory_row_height(compact))
                        })
                        .collect()
                })
            });
            let table = virtual_table(
                app.users_scroll_id(),
                header.into(),
                table_body.into(),
                Length::Fill,
                iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::default(),
                ),
                Message::UsersScrolled,
            );
            if let Some(banner) = source_notice_banner(
                theme_snapshot,
                shell.projection().sessions_source.as_deref(),
                RefreshRequest::Sessions,
            ) {
                column![banner, table]
                    .spacing(8)
                    .height(Length::Fill)
                    .into()
            } else {
                table
            }
        }
    };

    let actions = session_action_bar(theme_snapshot, shell, rows.as_ref());

    column![
        text(user_heading(list_state, row_count)).size(f32::from(tokens::FONT_16)),
        body,
        user_row_menu(app, theme_snapshot),
        actions,
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

/// The open Users row context menu (Disconnect / Lock, GPUI parity). Rendered
/// as a compact panel above the action bar so it never covers the row that
/// opened it; each entry routes through the same shared session-control path
/// as the action-bar buttons. `None` while closed renders nothing.
fn user_row_menu<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let Some(session) = app.user_menu_session() else {
        return column![].into();
    };
    let label = text(format!("{} {}", t("hint.selected_session"), session.id))
        .size(f32::from(tokens::FONT_12))
        .color(crate::theme::muted_text_color(theme_snapshot));
    container(
        column![
            label,
            row![
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::UserRowMenuDisconnect,
                    t("users.disconnect").to_string(),
                    Message::RequestSessionControl(SessionControlAction::Disconnect),
                    false,
                ),
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::UserRowMenuLock,
                    t("users.lock").to_string(),
                    Message::RequestSessionControl(SessionControlAction::Lock),
                    false,
                ),
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::UserRowMenuClose,
                    t("chrome.cancel").to_string(),
                    Message::CloseUserRowMenu,
                    false,
                ),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .padding(8),
    )
    .style(move |_| theme::panel_style(theme_snapshot))
    .width(Length::Fill)
    .into()
}

fn session_action_bar<'a>(
    theme_snapshot: &'a Theme,
    shell: &ShellApp,
    rows: &[UserRow],
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let enabled = rows.get(shell.selected).is_some();
    let selected = rows.get(shell.selected).map_or_else(
        || t("hint.select_session").to_string(),
        |session| {
            let tty_str = session.tty.as_deref().unwrap_or("—");
            let remote_str = if session.remote {
                t("users.remote")
            } else {
                t("common.none")
            };
            format!(
                "{} {} · {} (UID {}) · TTY {} · {}",
                t("hint.selected_session"),
                session.id,
                session.user,
                session.uid,
                tty_str,
                remote_str
            )
        },
    );
    let feedback = session_feedback_line(theme_snapshot, shell);
    row![
        session_action_button(
            theme_snapshot,
            t("users.disconnect"),
            SessionControlAction::Disconnect,
            FocusTarget::SessionDisconnect,
            enabled,
        ),
        session_action_button(
            theme_snapshot,
            t("users.lock"),
            SessionControlAction::Lock,
            FocusTarget::SessionLock,
            enabled,
        ),
        feedback.unwrap_or_else(|| {
            text(selected)
                .size(f32::from(tokens::FONT_12))
                .color(theme::muted_text_color(theme_snapshot))
                .into()
        }),
    ]
    .spacing(8)
    .padding(4)
    .into()
}

/// The point-of-action feedback line (GPUI `feedback_status_line` parity):
/// the last accepted session-control outcome, colored by success/failure. A
/// newer request clears the shell slot, so the hint falls back to the
/// selection text.
fn session_feedback_line<'a>(
    theme_snapshot: &'a Theme,
    shell: &ShellApp,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let outcome = shell.projection().session_control_feedback.as_ref()?;
    let action = match outcome.action {
        SessionControlAction::Disconnect => t("users.disconnect"),
        SessionControlAction::Lock => t("users.lock"),
    };
    let detail = match &outcome.result {
        Ok(()) => None,
        Err(error) => Some(&format!("{error:?}")),
    };
    let text_value = match detail {
        Some(detail) => t("feedback.action_failed_detail")
            .replace("{action}", action)
            .replace("{target}", outcome.session_id.as_str())
            .replace("{detail}", detail),
        None => t("feedback.action_succeeded")
            .replace("{action}", action)
            .replace("{target}", outcome.session_id.as_str()),
    };
    let color = match outcome.result {
        Ok(()) => crate::theme::color(theme_snapshot.palette().success),
        Err(_) => crate::theme::color(theme_snapshot.palette().danger),
    };
    Some(
        text(text_value)
            .size(f32::from(tokens::FONT_12))
            .color(color)
            .width(Length::Fill)
            .into(),
    )
}

fn session_action_button<'a>(
    theme_snapshot: &'a Theme,
    label: &'static str,
    action: SessionControlAction,
    target: FocusTarget,
    enabled: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    if enabled {
        focus::button(
            theme_snapshot,
            target,
            label,
            Message::RequestSessionControl(action),
            false,
        )
    } else {
        button(text(label)).into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserRow {
    id: String,
    uid: u32,
    user: String,
    seat: Option<String>,
    tty: Option<String>,
    remote: bool,
    timestamp: Option<String>,
}

fn user_list_state(shell: &ShellApp) -> ListState {
    match shell.projection().sessions.as_deref() {
        None => ListState::Loading,
        Some([]) => ListState::Empty,
        Some(_) => ListState::Ready,
    }
}

pub(crate) fn user_rows(shell: &ShellApp) -> Vec<UserRow> {
    // Rows project through the shared indexed sort order (provider order until
    // a header click picks a column), so selection maps to the same visible
    // order without materializing a borrowed row vector first.
    let sessions = shell.projection().sessions.as_deref().unwrap_or(&[]);
    shell
        .sorted_session_indices()
        .into_iter()
        .filter_map(|index| {
            let session = sessions.get(index)?;
            Some(UserRow {
                id: session.id.clone(),
                uid: session.uid,
                user: session.user.clone(),
                seat: session.seat.clone(),
                tty: session.tty.clone(),
                remote: session.remote,
                timestamp: session.timestamp.clone(),
            })
        })
        .collect()
}

fn user_heading(state: ListState, row_count: usize) -> String {
    match state {
        ListState::Loading => t("users.inventory_waiting").to_string(),
        ListState::Empty => format!(
            "{} · {}",
            t("tab.users"),
            t("users.reported").replace("{count}", "0")
        ),
        ListState::Ready => format!(
            "{} · {}",
            t("tab.users"),
            t("users.reported").replace("{count}", &row_count.to_string())
        ),
    }
}

fn optional_text(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map_or_else(missing_value, str::to_owned)
}

fn remote_text(remote: bool) -> &'static str {
    if remote {
        t("common.yes")
    } else {
        t("common.no")
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/users_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/users_feedback_tests.rs"]
mod feedback_tests;
