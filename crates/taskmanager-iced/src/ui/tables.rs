//! Typed Services projections and shared inventory helpers for the Iced frontend.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};
// Shared locale catalog for the shared-page body chrome (System / Services /
// Startup column headers, action labels, confirm/cancel) that was previously
// hard-coded English.
use std::rc::Rc;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppPage, RefreshRequest};
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::services::{ServiceAction, ServiceStatus};

use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::tokens;

pub(super) use super::components::{
    message_panel, search_input, source_notice_banner, source_state_panel,
};
use super::virtual_list::{ColumnWidth, TableColumn};
use super::{
    VIRTUAL_TABLE_HEADER_HEIGHT, VirtualWindow, virtual_table, virtual_table_body,
    virtual_table_key, virtual_table_row,
};
use crate::app::{FocusTarget, Message};
use crate::ui::components::highlight;
use crate::{IcedApp, focus, theme};
use taskmanager_shell::presentation::MISSING_VALUE;

mod headings;
pub(super) use headings::service_heading;

/// A clickable column-header cell for the shared inventory tables (Services /
/// Startup / Users). The active column shows the ▲/▼ direction marker; clicking
/// routes through [`Message::SortInfoTable`] to the shell's single per-table
/// sort slot. The caption comes from [`InfoSortCol::label`] (the shell's
/// single source of truth) — never duplicated here.
pub(super) fn info_header_cell<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    table: InfoTable,
    column: InfoSortCol,
    active: Option<(InfoSortCol, SortDir)>,
    width: Length,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let is_active = active.is_some_and(|(active_column, _)| active_column == column);
    let label: Element<'a, Message, iced::Theme, iced::Renderer> = match active {
        Some((active_column, direction)) if active_column == column => {
            let marker = match direction {
                SortDir::Asc => "▲",
                SortDir::Desc => "▼",
            };
            row![
                text(t(column.label())),
                text(marker).size(f32::from(taskmanager_theme::tokens::FONT_CAPTION)),
            ]
            .spacing(4)
            .width(Length::Fill)
            .align_y(iced::Alignment::Center)
            .into()
        }
        _ => text(t(column.label())).width(Length::Fill).into(),
    };
    button(label)
        .on_press(Message::SortInfoTable { table, column })
        .style(move |_theme, status| theme::header_button_style(theme_snapshot, status, is_active))
        .padding([2.0, 4.0])
        .width(width)
        .into()
}

/// A non-sortable column-header cell: a plain caption text at the spec's
/// width. The caption key comes from the spec's shared-catalog `label`, so
/// header and body read the same typed column description.
pub(super) fn plain_header_cell(
    spec: &TableColumn,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    text(t(spec.label)).width(spec.length()).into()
}

/// Typed column specs for the Services table — the same vocabulary as the
/// process-column contract (id/label/width/alignment), page-owned because
/// service-table semantics have no cross-frontend contract. The header row
/// and every body row read the SAME specs, which keeps the sticky header
/// pixel-aligned over its columns. In the compact layout the description
/// column is dropped and its width goes to the action strip.
#[derive(Clone, Copy)]
pub(super) struct ServicesColumns {
    pub(super) name: TableColumn,
    pub(super) status: TableColumn,
    pub(super) description: Option<TableColumn>,
    pub(super) actions: TableColumn,
}

pub(super) fn services_columns(compact: bool) -> ServicesColumns {
    ServicesColumns {
        name: TableColumn::text("Name", InfoSortCol::Name.label(), ColumnWidth::Fixed(220.0)),
        status: TableColumn::text(
            "Status",
            InfoSortCol::Status.label(),
            ColumnWidth::Fixed(90.0),
        ),
        description: (!compact)
            .then(|| TableColumn::text("Description", "common.description", ColumnWidth::Fill)),
        actions: TableColumn::text(
            "Actions",
            "common.actions",
            ColumnWidth::Fixed(if compact { 300.0 } else { 450.0 }),
        ),
    }
}

/// Render the services inventory and the typed control confirmation bar.
pub(super) fn services_page(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let query = app.services_query();
    let (rows, filtered_indices, projection_generation) = app.services_projection(query);
    let row_count = rows.len();
    let list_state = service_list_state(shell);
    // A compact density preference and a compact viewport share the same
    // bounded row geometry. The latter must also drive action wrapping or a
    // normal-density 720px window would still paint the seven-button strip
    // beyond the right edge.
    let compact = app.compact_density() || app.compact_layout();
    let row_padding = theme::row_padding(compact);

    let body: Element<'_, Message, iced::Theme, iced::Renderer> = match list_state {
        ListState::Loading => message_panel(theme_snapshot, t("common.waiting_inventory")),
        ListState::Empty => source_state_panel(
            theme_snapshot,
            shell.projection().services_source.as_deref(),
            RefreshRequest::Services,
        )
        .unwrap_or_else(|| message_panel(theme_snapshot, t("empty.no_services_reported"))),
        ListState::Ready => {
            // The per-page name filter (GPUI parity: each inventory page owns
            // its query; the shared process query is untouched).
            // Clickable sort headers: the active column wears ▲/▼, clicks
            // route to the shell's shared per-table sort slot. Widths come
            // from the typed column specs the body cells also read.
            let columns = services_columns(compact);
            let mut header_cells: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = vec![
                info_header_cell(
                    theme_snapshot,
                    InfoTable::Services,
                    InfoSortCol::Name,
                    shell.services_sort,
                    columns.name.length(),
                ),
                info_header_cell(
                    theme_snapshot,
                    InfoTable::Services,
                    InfoSortCol::Status,
                    shell.services_sort,
                    columns.status.length(),
                ),
            ];
            if let Some(description) = columns.description.as_ref() {
                header_cells.push(plain_header_cell(description));
            }
            header_cells.push(plain_header_cell(&columns.actions));
            let header_row: Element<'_, Message, iced::Theme, iced::Renderer> = row(header_cells)
                .spacing(8)
                .padding(row_padding)
                .width(Length::Fill)
                .into();
            let header = container(header_row)
                .height(Length::Fixed(VIRTUAL_TABLE_HEADER_HEIGHT))
                .width(Length::Fill);

            let (scroll_y, viewport_height) = app.services_virtual_scroll();
            // Sticky header: the header row sits outside the body scrollable,
            // so the body window carries no header prefix.
            let window = VirtualWindow::for_sticky_rows(
                filtered_indices.len(),
                scroll_y,
                viewport_height,
                service_row_height(compact),
            );
            // The compact two-cell identity block derives from the same
            // specs: name + status + the shared column gutter.
            let compact_cell_width = columns.name.width.fixed_px()
                + columns.status.width.fixed_px()
                + theme::table_column_spacing();
            let query_text = query.to_owned();
            let table_theme = *theme_snapshot;
            let selected = shell.selected;
            // The open Services-row menu re-hosts its row; its source index is
            // both the lazy-invalidation marker and the mount condition.
            let open_menu_service = app.service_menu_index();
            let base_key = inventory_table_key(InventoryTableKey {
                theme_snapshot,
                generation: projection_generation,
                table: InfoTable::Services,
                sort: shell.services_sort,
                query: &query_text,
                search_active: !query_text.trim().is_empty(),
                selected,
                row_count: filtered_indices.len(),
                compact,
                open_menu: open_menu_service.map(|index| index.to_string()),
            });
            let body_rows = Rc::clone(&rows);
            let body_indices = Rc::clone(&filtered_indices);
            let body_query = query_text.clone();
            let table_body = iced::widget::lazy(virtual_table_key(base_key, window), move |_| {
                let rows = Rc::clone(&body_rows);
                let filtered_indices = Rc::clone(&body_indices);
                let query_text = body_query.clone();
                let columns = services_columns(compact);
                virtual_table_body(window, Length::Fill, move |start, end| {
                    filtered_indices
                        .get(start..end)
                        .unwrap_or(&[])
                        .iter()
                        .enumerate()
                        .filter_map(|(offset, source_row_index)| {
                            let index = start + offset;
                            let service = rows.get(*source_row_index)?;
                            let is_selected = index == selected;
                            let zebra = theme::zebra_index(index);
                            let cell_content: Element<
                                'static,
                                Message,
                                iced::Theme,
                                iced::Renderer,
                            > = if compact {
                                container(
                                    row![
                                        highlight::cell(
                                            &table_theme,
                                            service.name.as_str(),
                                            query_text.as_str(),
                                            !query_text.trim().is_empty(),
                                            columns.name.length(),
                                        ),
                                        text(service.status.as_str())
                                            .width(columns.status.length()),
                                    ]
                                    .spacing(8),
                                )
                                .width(Length::Fixed(compact_cell_width))
                                .into()
                            } else {
                                container(row![
                                    highlight::cell(
                                        &table_theme,
                                        service.name.as_str(),
                                        query_text.as_str(),
                                        !query_text.trim().is_empty(),
                                        columns.name.length(),
                                    ),
                                    text(service.status.as_str()).width(columns.status.length()),
                                    text(
                                        service_description(service.description.as_str())
                                            .to_owned(),
                                    )
                                    .width(Length::Fill),
                                ])
                                .into()
                            };
                            let cells = focus::selectable_row_with_menu(
                                &table_theme,
                                AppPage::Services,
                                index,
                                container(cell_content)
                                    .style(move |_| {
                                        theme::row_style(&table_theme, is_selected, zebra)
                                    })
                                    .padding(row_padding)
                                    .width(if compact {
                                        Length::Fixed(compact_cell_width)
                                    } else {
                                        Length::Fill
                                    })
                                    .into(),
                                Message::OpenServiceRowMenu {
                                    visual_index: index,
                                    source_index: service.source_index,
                                },
                            );
                            let cells = if open_menu_service == Some(service.source_index) {
                                // The open menu floats on its own row: anchored
                                // by the popover primitive and dismissed by an
                                // outside press without touching what's below.
                                let panel = super::service_menu::panel(
                                    table_theme,
                                    service.source_index,
                                    service.name.clone(),
                                );
                                crate::ui::components::Popover::new(
                                    cells,
                                    panel,
                                    Message::CloseServiceRowMenu,
                                )
                                .into()
                            } else {
                                cells
                            };
                            let row = row![
                                cells,
                                service_action_buttons(table_theme, service.source_index, compact,),
                            ]
                            .spacing(8)
                            .padding(row_padding)
                            .width(Length::Fill);
                            Some(virtual_table_row(row.into(), service_row_height(compact)))
                        })
                        .collect()
                })
            });
            let table = virtual_table(
                app.services_scroll_id(),
                header.into(),
                table_body.into(),
                Length::Fill,
                iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::default(),
                ),
                Message::ServicesScrolled,
            );
            if let Some(banner) = source_notice_banner(
                theme_snapshot,
                shell.projection().services_source.as_deref(),
                RefreshRequest::Services,
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
    let heading = service_heading(list_state, row_count);

    let control_bar: Element<'_, Message, iced::Theme, iced::Renderer> =
        if let Some(target) = shell.pending_service_control() {
            row![
                text(format!(
                    "{} {}?",
                    service_action_label(target.action),
                    target.service_id
                )),
                focus::button(
                    theme_snapshot,
                    FocusTarget::ConfirmServiceControl,
                    t("common.confirm"),
                    Message::ConfirmServiceControl,
                    false,
                ),
                focus::button(
                    theme_snapshot,
                    FocusTarget::CancelServiceControl,
                    t("common.cancel"),
                    Message::DismissOverlay,
                    false,
                ),
            ]
            .spacing(8)
            .padding(4)
            .into()
        } else {
            text(t("svc.actions_hint"))
                .size(f32::from(tokens::FONT_12))
                .into()
        };

    // The shared search-input component (icon prefix + clear affordance):
    // placeholder, value, and the `Message::ServicesSearchChanged` channel are
    // unchanged from the bare text_input it replaces; the field now registers
    // under its stable `FocusTarget::ServicesSearch` id so the focus shell can
    // reach it. The 280px bound the page always gave the field wraps the
    // component's Fill-shaped row.
    let search: Element<'_, Message, iced::Theme, iced::Renderer> = container(search_input(
        theme_snapshot,
        FocusTarget::ServicesSearch,
        t("search.services"),
        query,
        Message::ServicesSearchChanged,
    ))
    .width(Length::Fixed(280.0))
    .into();

    column![
        text(heading).size(f32::from(tokens::FONT_16)),
        search,
        body,
        control_bar,
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

pub(super) fn service_action_label(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => t("svc.start"),
        ServiceAction::Stop => t("svc.stop"),
        ServiceAction::Restart => t("svc.restart"),
        ServiceAction::Enable => t("svc.enable"),
        ServiceAction::Disable => t("svc.disable"),
    }
}

/// The per-row Start/Restart/Stop/Enable/Disable strip. `source_index` is the
/// row's PROVIDER-order index — never its visual position in the sorted and
/// filtered view: [`Message::RequestServiceAction`] resolves the target
/// against `shell.projection().services`, so a visual index would target the wrong
/// service whenever a sort or the page filter reorders the rows.
fn service_action_buttons(
    theme_snapshot: taskmanager_theme::Theme,
    source_index: usize,
    compact: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let actions = vec![
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceAction {
                index: source_index,
                action: ServiceAction::Start,
            },
            t("svc.start").to_string(),
            Message::RequestServiceAction {
                index: source_index,
                action: ServiceAction::Start,
            },
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceAction {
                index: source_index,
                action: ServiceAction::Restart,
            },
            t("svc.restart").to_string(),
            Message::RequestServiceAction {
                index: source_index,
                action: ServiceAction::Restart,
            },
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceAction {
                index: source_index,
                action: ServiceAction::Stop,
            },
            t("svc.stop").to_string(),
            Message::RequestServiceAction {
                index: source_index,
                action: ServiceAction::Stop,
            },
            true,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceAction {
                index: source_index,
                action: ServiceAction::Enable,
            },
            t("svc.enable").to_string(),
            Message::RequestServiceAction {
                index: source_index,
                action: ServiceAction::Enable,
            },
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceAction {
                index: source_index,
                action: ServiceAction::Disable,
            },
            t("svc.disable").to_string(),
            Message::RequestServiceAction {
                index: source_index,
                action: ServiceAction::Disable,
            },
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ServiceLogOpen {
                index: source_index,
            },
            t("svc.logs").to_string(),
            Message::OpenServiceLogFor {
                index: source_index,
            },
            false,
        ),
        super::service_details::open_button_owned(theme_snapshot, source_index),
    ];
    if compact {
        // Four buttons per line keep the service actions inside the row's
        // available width; the service row contract reserves two lines.
        super::chunked_rows(actions, 4)
    } else {
        // The action strip shares the typed Actions column width with the
        // header cell, so the strip ends where the table ends.
        let actions_width = services_columns(compact).actions.length();
        row(actions).spacing(4).width(actions_width).into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListState {
    Loading,
    Empty,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServiceRow {
    /// Provider-order index of the underlying item. The action messages carry
    /// THIS index (not the visual row position) because their effect arm
    /// resolves the target against `shell.projection().services` — sorting or
    /// filtering the view must never reinterpret who gets Stop/Restart.
    pub(super) source_index: usize,
    pub(super) name: String,
    pub(super) status: ServiceStatus,
    pub(super) description: String,
}

pub(crate) fn service_matches_lower(service: &ServiceRow, query: &str) -> bool {
    query.is_empty()
        || service.name.to_lowercase().contains(query)
        || service.description.to_lowercase().contains(query)
}

/// Shared fixed row contract for the three inventory tables. Service action
/// buttons need a little more vertical room than the read-only Startup/Users
/// cells, so the contract is intentionally conservative and density-aware.
pub(crate) fn inventory_row_height(compact: bool) -> f32 {
    if compact { 32.0 } else { 40.0 }
}

/// Services have seven action buttons in the compact projection. The wrapped
/// action cell needs two bounded rows; Startup and Users retain the smaller
/// shared inventory contract.
pub(crate) fn service_row_height(compact: bool) -> f32 {
    if compact {
        72.0
    } else {
        inventory_row_height(false)
    }
}

/// Invalidation identity shared by Services, Startup and Users lazy bodies.
/// The row data watermark, sort/query, selection and theme are the complete
/// inputs that can change the materialized widget tree; the virtual range is
/// added by [`virtual_table_key`].
pub(super) struct InventoryTableKey<'a> {
    pub(super) theme_snapshot: &'a taskmanager_theme::Theme,
    pub(super) generation: u64,
    pub(super) table: InfoTable,
    pub(super) sort: Option<(InfoSortCol, SortDir)>,
    pub(super) query: &'a str,
    pub(super) search_active: bool,
    pub(super) selected: usize,
    pub(super) row_count: usize,
    pub(super) compact: bool,
    /// Stable identity of the row whose context menu is open, if any. The
    /// open menu re-hosts one materialized row, so opening, moving, or
    /// closing it must rebuild the lazy body exactly like any other visual
    /// invalidation.
    pub(super) open_menu: Option<String>,
}

pub(super) fn inventory_table_key(input: InventoryTableKey<'_>) -> u64 {
    super::lazy_key::LazyKey::new("inventory-table")
        .revision(input.generation)
        .theme(input.theme_snapshot)
        .field(match input.table {
            InfoTable::Services => "services",
            InfoTable::Startup => "startup",
            InfoTable::Users => "users",
        })
        .field(
            input
                .sort
                .map(|(column, direction)| (column.label(), direction.label())),
        )
        .field(input.query)
        .field(input.search_active)
        .field(input.selected)
        .field(input.row_count)
        .field(input.compact)
        .field(input.open_menu)
        .finish()
}

pub(super) fn service_list_state(shell: &ShellApp) -> ListState {
    match shell.projection().services.as_deref() {
        None => ListState::Loading,
        Some([]) => ListState::Empty,
        Some(_) => ListState::Ready,
    }
}

pub(crate) fn service_rows(shell: &ShellApp) -> Vec<ServiceRow> {
    // Rows project through the shared indexed sort order (see
    // [`ShellApp::sorted_service_indices`]). The source index is carried all
    // the way into action messages; no pointer scan can turn this into an
    // O(N²) projection when a large service inventory is sorted.
    let provider = shell.projection().services.as_deref().unwrap_or(&[]);
    shell
        .sorted_service_indices()
        .into_iter()
        .filter_map(|source_index| {
            let service = provider.get(source_index)?;
            Some(ServiceRow {
                source_index,
                name: service.name.clone(),
                status: service.status,
                description: service.description.clone(),
            })
        })
        .collect()
}

pub(super) fn service_description(description: &str) -> &str {
    if description.is_empty() {
        MISSING_VALUE
    } else {
        description
    }
}

/// Confirm the pending process termination through the shared shell intent.
pub(super) fn confirm_bar<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: &FrozenProcessIdentity,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(format!("End {} ({})?", target.name, target.pid)),
        focus::button(
            theme_snapshot,
            FocusTarget::ConfirmEndTask,
            t("common.confirm"),
            Message::ConfirmEndTask,
            true,
        ),
        focus::button(
            theme_snapshot,
            FocusTarget::CancelEndTask,
            t("common.cancel"),
            Message::DismissOverlay,
            false,
        ),
    ]
    .spacing(8)
    .padding(4)
    .into()
}

/// The destructive batch (Kill) confirmation bar — mirrors [`confirm_bar`] but
/// confirms the pending batch intent (`ShellApp::pending_batch`). The target is
/// the intent's single frozen identity (single-select today); if the target
/// vanished before confirmation, only the cancel affordance renders.
pub(super) fn confirm_batch_bar<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    intent: &taskmanager_core::core::process::ProcessBatchIntent,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let confirm = focus::button(
        theme_snapshot,
        FocusTarget::ConfirmProcessBatch,
        t("common.confirm"),
        Message::ConfirmProcessBatch,
        true,
    );
    let cancel = focus::button(
        theme_snapshot,
        FocusTarget::CancelEndTask,
        t("common.cancel"),
        Message::DismissOverlay,
        false,
    );
    let action_label = match intent.action {
        taskmanager_core::core::process::ProcessBatchAction::End => t("proc.end_process_tree"),
        taskmanager_core::core::process::ProcessBatchAction::Kill => t("proc.kill"),
        taskmanager_core::core::process::ProcessBatchAction::Suspend => t("proc.suspend"),
        taskmanager_core::core::process::ProcessBatchAction::Resume => t("proc.resume"),
        taskmanager_core::core::process::ProcessBatchAction::SetPriority(_) => t("proc.priority"),
    };
    // Surface the full target scope so a multi-target destructive action reads
    // as a frozen identity set rather than the single first row (mirrors the
    // GPUI confirmation scope).
    let prompt: String = match intent.targets.len() {
        0 => action_label.to_string(),
        1 => {
            let target = &intent.targets[0];
            format!("{action_label} {} ({})?", target.name, target.pid)
        }
        count => format!(
            "{action_label} {} ({}) +{} more?",
            intent.targets[0].name,
            intent.targets[0].pid,
            count - 1
        ),
    };
    if intent.targets.is_empty() {
        row![cancel].spacing(8).padding(4).into()
    } else {
        row![text(prompt), confirm, cancel]
            .spacing(8)
            .padding(4)
            .into()
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/tables_tests.rs"]
mod tests;
