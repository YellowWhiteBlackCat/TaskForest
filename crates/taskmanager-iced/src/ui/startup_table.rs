//! The Startup-page table projection (rows, heading, control bars) and the
//! BN-05 boot-timeline waterfall, extracted from [`super::tables`] so the
//! tables module stays under the repository's source-size budget. All row
//! data comes from the shared shell inventory; the enable/disable actions
//! route through the shared startup-control gate.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};
use std::rc::Rc;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppPage, RefreshRequest};
use taskmanager_core::core::startup::{
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupCriticalChainNode, StartupImpact,
    StartupImpactEvidence, StartupScope, StartupSource,
};

use taskmanager_shell::presentation::missing_value;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp};
use taskmanager_theme::tokens;

use super::tables::{
    InventoryTableKey, ListState, info_header_cell, inventory_row_height, inventory_table_key,
    message_panel, plain_header_cell, source_notice_banner, source_state_panel,
};
use super::virtual_list::{ColumnWidth, TableColumn};
use super::{
    VIRTUAL_TABLE_HEADER_HEIGHT, VirtualWindow, virtual_table, virtual_table_body,
    virtual_table_key, virtual_table_row,
};
use crate::app::{FocusTarget, Message};
use crate::ui::components::highlight;
use crate::{IcedApp, focus, theme};

mod timeline;
pub(super) use timeline::*;

/// Typed column specs for the Startup table — the same vocabulary as the
/// process-column contract (id/label/width/alignment), page-owned because
/// startup-entry semantics have no cross-frontend contract. The header row
/// and every body row read the SAME specs, which keeps the sticky header
/// pixel-aligned over its columns; the command column is the flexible
/// remainder.
#[derive(Clone, Copy)]
pub(super) struct StartupColumns {
    pub(super) status: TableColumn,
    pub(super) name: TableColumn,
    pub(super) impact: TableColumn,
    pub(super) source: TableColumn,
    pub(super) control: TableColumn,
    pub(super) command: TableColumn,
}

pub(super) fn startup_columns() -> StartupColumns {
    StartupColumns {
        status: TableColumn::text(
            "Status",
            InfoSortCol::Status.label(),
            ColumnWidth::Fixed(90.0),
        ),
        name: TableColumn::text("Name", InfoSortCol::Name.label(), ColumnWidth::Fixed(190.0)),
        impact: TableColumn::text("Impact", "startup.impact", ColumnWidth::Fixed(130.0)),
        source: TableColumn::text("Source", "startup.source", ColumnWidth::Fixed(170.0)),
        control: TableColumn::text("Control", "startup.control", ColumnWidth::Fixed(120.0)),
        command: TableColumn::text("Command", "startup.command", ColumnWidth::Fill),
    }
}

pub(super) fn startup_page(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let (rows, _visible_indices, projection_generation) = app.startup_projection();
    let row_count = rows.len();
    let list_state = startup_list_state(shell);
    let compact = app.compact_density();
    let row_padding = theme::row_padding(compact);

    let body: Element<'_, Message, iced::Theme, iced::Renderer> = match list_state {
        ListState::Loading => message_panel(theme_snapshot, t("common.waiting_inventory")),
        ListState::Empty => source_state_panel(
            theme_snapshot,
            shell.projection().startup_source.as_deref(),
            RefreshRequest::Startup,
        )
        .unwrap_or_else(|| message_panel(theme_snapshot, t("empty.no_startup_reported"))),
        ListState::Ready => {
            // Clickable sort headers: the active column wears ▲/▼, clicks
            // route to the shell's shared per-table sort slot. Widths come
            // from the typed column specs the body cells also read.
            let columns = startup_columns();
            let header = container(
                row![
                    info_header_cell(
                        theme_snapshot,
                        InfoTable::Startup,
                        InfoSortCol::Status,
                        shell.startup_sort,
                        columns.status.length(),
                    ),
                    info_header_cell(
                        theme_snapshot,
                        InfoTable::Startup,
                        InfoSortCol::Name,
                        shell.startup_sort,
                        columns.name.length(),
                    ),
                    plain_header_cell(&columns.impact),
                    plain_header_cell(&columns.source),
                    plain_header_cell(&columns.control),
                    plain_header_cell(&columns.command),
                ]
                .spacing(8)
                .padding(4)
                .width(Length::Fill),
            )
            .height(Length::Fixed(VIRTUAL_TABLE_HEADER_HEIGHT))
            .width(Length::Fill);

            let (scroll_y, viewport_height) = app.startup_virtual_scroll();
            // Sticky header: the header row sits outside the body scrollable,
            // so the body window carries no header prefix.
            let window = VirtualWindow::for_sticky_rows(
                rows.len(),
                scroll_y,
                viewport_height,
                inventory_row_height(compact),
            );
            let table_theme = *theme_snapshot;
            let query = shell.query.clone();
            let search_active = shell.search_active();
            let selected = shell.selected;
            // The open Startup-row menu re-hosts its row. The stored menu
            // identity is the provider source index; the visual→source mapping
            // is the same shared sorted order the rows project through.
            let open_menu_source = app.startup_menu_index();
            let sorted_indices = Rc::new(shell.sorted_startup_indices());
            let base_key = inventory_table_key(InventoryTableKey {
                theme_snapshot,
                generation: projection_generation,
                table: InfoTable::Startup,
                sort: shell.startup_sort,
                query: &query,
                search_active,
                selected,
                row_count: rows.len(),
                compact,
                open_menu: open_menu_source.map(|index| index.to_string()),
            });
            let body_rows = Rc::clone(&rows);
            let body_open_menu = open_menu_source;
            let body_sorted = Rc::clone(&sorted_indices);
            let body_query = query.clone();
            let columns = startup_columns();
            let table_body = iced::widget::lazy(virtual_table_key(base_key, window), move |_| {
                let rows = Rc::clone(&body_rows);
                let open_menu_source = body_open_menu;
                let sorted_indices = Rc::clone(&body_sorted);
                let query = body_query.clone();
                virtual_table_body(window, Length::Fill, move |start, end| {
                    rows.get(start..end)
                        .unwrap_or(&[])
                        .iter()
                        .enumerate()
                        .map(|(offset, startup)| {
                            let index = start + offset;
                            let is_selected = index == selected;
                            let zebra = theme::zebra_index(index);
                            let row = row![
                                text(startup_status_text(startup)).width(columns.status.length()),
                                highlight::cell(
                                    &table_theme,
                                    startup.name.as_str(),
                                    query.as_str(),
                                    search_active,
                                    columns.name.length(),
                                ),
                                text(startup_impact_text(startup)).width(columns.impact.length()),
                                text(startup_source_text(startup)).width(columns.source.length()),
                                text(startup_control_text(startup)).width(columns.control.length()),
                                text(startup.exec.clone()).width(columns.command.length()),
                            ]
                            .spacing(8)
                            .padding(row_padding)
                            .width(Length::Fill);
                            let row = focus::selectable_row_with_menu(
                                &table_theme,
                                AppPage::Startup,
                                index,
                                container(row)
                                    .style(move |_| {
                                        theme::row_style(&table_theme, is_selected, zebra)
                                    })
                                    .width(Length::Fill)
                                    .into(),
                                Message::OpenStartupRowMenu {
                                    visual_index: index,
                                },
                            );
                            let row = match sorted_indices.get(index).copied() {
                                Some(source_index) if open_menu_source == Some(source_index) => {
                                    // The open menu floats on its own row:
                                    // anchored by the popover primitive and
                                    // dismissed by an outside press without
                                    // touching what's below.
                                    let panel = super::startup_menu::panel(
                                        table_theme,
                                        source_index,
                                        startup.name.clone(),
                                    );
                                    crate::ui::components::Popover::new(
                                        row,
                                        panel,
                                        Message::CloseStartupRowMenu,
                                    )
                                    .into()
                                }
                                _ => row,
                            };
                            virtual_table_row(row, inventory_row_height(compact))
                        })
                        .collect()
                })
            });
            let table = virtual_table(
                app.startup_scroll_id(),
                header.into(),
                table_body.into(),
                Length::Fill,
                iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::default(),
                ),
                Message::StartupScrolled,
            );
            if let Some(banner) = source_notice_banner(
                theme_snapshot,
                shell.projection().startup_source.as_deref(),
                RefreshRequest::Startup,
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

    // Boot evidence (failed-units/critical-chain pills) + the BN-05 waterfall:
    // display-only blocks between the heading and the table (GPUI/TUI parity).
    // They never join the focus/selection domain; each stays silent until
    // typed evidence arrives and silent on typed failure.
    let mut page =
        column![text(startup_heading(list_state, row_count)).size(f32::from(tokens::FONT_16))]
            .spacing(8);
    if let Some(strip) = boot_evidence_strip(
        theme_snapshot,
        app.shell.projection().startup_boot_evidence.as_ref(),
    ) {
        page = page.push(strip);
    }
    if let Some(block) = boot_timeline_block(
        theme_snapshot,
        app.shell.projection().startup_boot_evidence.as_ref(),
    ) {
        page = page.push(block);
    }
    let page = page.push(body).height(Length::Fill);
    // The enable/disable action bar renders only when there are entries to act
    // on (gating on selection happens inside the bar). Mirrors the Users
    // session action bar.
    // A pending startup request gates behind a confirmation bar (mirrors the
    // F12 Kill confirm + GPUI's startup confirm dialog); otherwise the normal
    // enable/disable action bar.
    let page = if rows.is_empty() {
        page
    } else if let Some(request) = shell.pending_startup() {
        page.push(startup_confirm_bar(theme_snapshot, request))
    } else {
        page.push(startup_action_bar(theme_snapshot, shell, rows.as_ref()))
    };
    page.into()
}

/// The gated startup-control confirmation bar: the enable/disable prompt
/// carrying the target entry name, plus Confirm / Cancel. Mirrors GPUI's
/// `request_startup_control_confirmation` dialog and the F12 Kill confirm bar.
fn startup_confirm_bar<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    request: &taskmanager_application::StartupControlRequest,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let confirm = focus::button(
        theme_snapshot,
        FocusTarget::ConfirmStartupControl,
        t("common.confirm"),
        Message::ConfirmStartupControl,
        true,
    );
    let cancel = focus::button(
        theme_snapshot,
        FocusTarget::CancelEndTask,
        t("common.cancel"),
        Message::DismissOverlay,
        false,
    );
    let verb = if request.enabled {
        t("startup.enable")
    } else {
        t("startup.disable")
    };
    row![
        text(format!("{} {}?", verb, request.entry.name)),
        confirm,
        cancel
    ]
    .spacing(8)
    .padding(4)
    .into()
}

/// The Startup-page enable/disable action bar. A contextual toggle — Disable
/// when the selected entry is enabled, Enable when not — plus the selected
/// entry's name and control policy as a quiet status line. Disable is
/// destructive, so it wears the danger styling; no-selection or an
/// `Unsupported` control policy renders a quiet disabled affordance. The
/// toggle submits through the shell's shared startup-control request
/// (latest-wins). Mirrors the Users session action bar.
fn startup_action_bar<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    shell: &ShellApp,
    rows: &[StartupRow],
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let selected = rows.get(shell.selected);
    // GPUI parity: an Unsupported-policy entry never carries a live toggle —
    // the underlying control request can never succeed for it, so the button
    // renders as the same inert affordance the no-selection branch uses.
    let actionable = selected.is_some_and(startup_control_actionable);
    // Contextual toggle: Disable when currently enabled, Enable when not.
    let (label, next_enabled) = match selected.map(|row| row.enabled) {
        Some(true) => (t("startup.disable"), false),
        _ => (t("startup.enable"), true),
    };
    let status = selected.map_or_else(String::new, |row| {
        format!("{} · {}", row.name, startup_control_text(row))
    });
    let mut elements: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        if actionable {
            focus::button(
                theme_snapshot,
                FocusTarget::StartupControl,
                label,
                Message::RequestStartupControl(next_enabled),
                !next_enabled,
            )
        } else {
            button(text(label)).into()
        },
        text(status).size(f32::from(tokens::FONT_12)).into(),
    ];
    if let Some(row) = selected.filter(|r| !r.exec.is_empty()) {
        elements.push(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::StartupOpenLocation,
            format!("{}: {}", t("common.copy"), row.exec),
            Message::CopyTextToClipboard {
                label: "Startup Exec".to_string(),
                text: row.exec.clone(),
            },
            false,
        ));
    }
    row(elements).spacing(8).padding(4).into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StartupRow {
    pub(super) id: String,
    pub(crate) name: String,
    enabled: bool,
    source: StartupSource,
    scope: StartupScope,
    control_policy: StartupControlPolicy,
    impact: StartupImpact,
    impact_evidence: StartupImpactEvidence,
    pub(crate) exec: String,
}

pub(super) fn startup_list_state(shell: &ShellApp) -> ListState {
    match shell.projection().startup_entries.as_deref() {
        None => ListState::Loading,
        Some([]) => ListState::Empty,
        Some(_) => ListState::Ready,
    }
}

pub(crate) fn startup_rows(shell: &ShellApp) -> Vec<StartupRow> {
    // Rows project through the shared indexed sort order (provider order until
    // a header click picks a column), so selection maps to the same visible
    // order without first allocating a Vec of borrowed entries.
    let entries = shell.projection().startup_entries.as_deref().unwrap_or(&[]);
    shell
        .sorted_startup_indices()
        .into_iter()
        .filter_map(|index| {
            let entry = entries.get(index)?;
            Some(StartupRow {
                id: entry.id.as_str().to_owned(),
                name: entry.name.clone(),
                enabled: entry.enabled,
                source: entry.source,
                scope: entry.scope,
                control_policy: entry.control_policy,
                impact: entry.impact,
                impact_evidence: entry.impact_evidence,
                exec: entry.exec.clone(),
            })
        })
        .collect()
}

pub(super) fn startup_heading(state: ListState, row_count: usize) -> String {
    match state {
        ListState::Loading => format!("{} · {}", t("tab.startup"), t("common.waiting_inventory")),
        ListState::Empty => format!("{} · 0 {}", t("tab.startup"), t("common.reported")),
        ListState::Ready => format!(
            "{} · {row_count} {}",
            t("tab.startup"),
            t("common.reported")
        ),
    }
}

pub(super) fn startup_status_text(row: &StartupRow) -> &'static str {
    if row.enabled {
        t("common.enabled")
    } else {
        t("common.disabled")
    }
}

pub(super) fn startup_impact_text(row: &StartupRow) -> String {
    match row.impact_evidence {
        StartupImpactEvidence::Measured { duration_ms } => {
            format!("{} · {duration_ms} ms", t(row.impact.i18n_key()))
        }
        StartupImpactEvidence::Unknown { .. } => format!(
            "{} · {}",
            t(row.impact.i18n_key()),
            t("startup.impact_unmeasured")
        ),
    }
}

pub(super) fn startup_source_text(row: &StartupRow) -> String {
    format!(
        "{} · {}",
        row.source.as_str(),
        startup_scope_text(row.scope)
    )
}

fn startup_scope_text(scope: StartupScope) -> &'static str {
    match scope {
        StartupScope::User => t("startup.scope_user"),
        StartupScope::System => t("startup.scope_system"),
        StartupScope::Session => t("startup.scope_session"),
        StartupScope::Unknown => t("startup.scope_unknown"),
    }
}

pub(super) fn startup_control_text(row: &StartupRow) -> &'static str {
    match row.control_policy {
        StartupControlPolicy::Direct => t("startup.control_direct"),
        StartupControlPolicy::UserOverride => t("startup.control_user_override"),
        StartupControlPolicy::Unsupported => t("startup.control_unsupported"),
    }
}

/// Whether the selected row may carry a live enable/disable toggle. An
/// `Unsupported` control policy means the native provider cannot safely
/// mutate the entry — the request would never succeed, so the toggle renders
/// inert for those rows (GPUI's `can_enable`/`can_disable` exclude them).
fn startup_control_actionable(row: &StartupRow) -> bool {
    row.control_policy != StartupControlPolicy::Unsupported
}

// ── boot evidence strip (failed units + critical chain) ──────────────────────
//
// The same typed snapshot the waterfall reads also carries the `systemctl
// --failed` set and the `systemd-analyze` critical chain; those surface as
// two quiet pills above the waterfall (GPUI parity). Failures stay typed
// ("boot evidence unavailable"), a true empty set reads "0" — never a
// fabricated zero or a silent absence.

/// Up to this many failed-unit names appear in the summary pill.
const MAX_FAILED_UNIT_NAMES: usize = 2;

/// The projected strip contents; `failed_units_danger` marks a populated
/// failed-units set so the renderer can wear the theme's danger token.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootEvidenceStrip {
    failed_units: String,
    failed_units_danger: bool,
    critical_chain: String,
}

/// Pure strip projection over one typed evidence snapshot; `None` keeps the
/// strip silent until a snapshot arrives (matching the waterfall's silence).
fn boot_evidence_strip_data(
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<BootEvidenceStrip> {
    let evidence = evidence?;
    let failed_units = if evidence.failed_units_failure.is_some() {
        t("startup.evidence_unavailable").to_string()
    } else if evidence.failed_units.is_empty() {
        "0".to_string()
    } else {
        let names = evidence
            .failed_units
            .iter()
            .take(MAX_FAILED_UNIT_NAMES)
            .map(|unit| unit.unit.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        format!("{} · {names}", evidence.failed_units.len())
    };
    let critical_chain = if evidence.critical_chain_failure.is_some() {
        t("startup.evidence_unavailable").to_string()
    } else {
        // Total measured boot time across the chain's nodes plus the head
        // unit; untimed nodes contribute nothing, an all-untimed chain is an
        // honest dash (a measured zero stays a real value).
        let measured: Vec<&StartupCriticalChainNode> = evidence
            .critical_chain
            .iter()
            .filter(|node| node.duration_ms.is_some())
            .collect();
        if measured.is_empty() {
            missing_value()
        } else {
            let total_ms = measured.iter().fold(0_u64, |sum, node| {
                sum.saturating_add(node.duration_ms.unwrap_or(0))
            });
            let head = measured
                .first()
                .map(|node| node.unit.as_str())
                .unwrap_or("");
            format!(
                "{total_ms} ms{}",
                if head.is_empty() {
                    String::new()
                } else {
                    format!(" · {head}")
                }
            )
        }
    };
    Some(BootEvidenceStrip {
        failed_units,
        failed_units_danger: !evidence.failed_units.is_empty(),
        critical_chain,
    })
}

/// The non-interactive boot-evidence pill strip above the waterfall; `None`
/// when silent. Failed units wear the danger color only when populated.
fn boot_evidence_strip<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let data = boot_evidence_strip_data(evidence)?;
    let muted = theme::muted_text_color(theme_snapshot);
    let failed_color = if data.failed_units_danger {
        taskmanager_theme::iced::color(theme_snapshot.danger)
    } else {
        muted
    };
    let fg = taskmanager_theme::iced::color(theme_snapshot.fg);
    Some(
        row![
            row![
                text(t("startup.failed_units"))
                    .size(f32::from(tokens::FONT_11))
                    .color(muted),
                text(data.failed_units)
                    .size(f32::from(tokens::FONT_11))
                    .color(failed_color),
            ]
            .spacing(8),
            row![
                text(t("startup.critical_chain"))
                    .size(f32::from(tokens::FONT_11))
                    .color(muted),
                text(data.critical_chain)
                    .size(f32::from(tokens::FONT_11))
                    .color(fg),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .padding(4)
        .into(),
    )
}

// ── BN-05 boot timeline waterfall ────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/gui/ui/startup_table_tests.rs"]
mod tests;
