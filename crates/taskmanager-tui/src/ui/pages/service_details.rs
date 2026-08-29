//! Selected-service details column for the Services page.
//!
//! GPUI parity (`services_view/details.rs`): the selected service renders a
//! state triplet (load / active / sub) and its Requires / Wants /
//! Wanted-by / After relations beside the table. The terminal surface says
//! the same facts in row/column language: a read-only column that follows
//! the table's `selected` cursor and yields wholly to frames that cannot
//! afford it. Relations come only from the shell's canonical
//! `ServiceDependenciesLifecycle`; the terminal never opens the channel and
//! never fabricates a fact the channel did not deliver.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{ServiceDeps, ServiceRelationKind};

use super::super::{panel, text};
use crate::{TuiApp, TuiTheme};

/// The details column's fixed width: the widest localized label plus a
/// relation value that wraps inside the panel instead of eating the table.
const COLUMN_WIDTH: u16 = 36;
/// Minimum total content width before the panel may exist: the column plus
/// a table region that still renders its three columns honestly.
const MIN_CONTENT_WIDTH: u16 = 100;
/// Minimum content height before the panel may exist: the seven fact rows
/// plus their panel borders.
const MIN_CONTENT_HEIGHT: u16 = 10;
/// Label cell width for the fact rows (the longest English label is 12
/// cells; `pad_cells` keeps CJK labels aligned by display width too).
const LABEL_CELLS: usize = 13;
/// GPUI parity bound (`details.rs` `format_dependencies`): a relation row's
/// joined targets truncate after this many display characters.
const MAX_RELATION_CHARS: usize = 80;

/// Split the Services page's content into the table region and the optional
/// details column. The panel is a pure addition with one honest rule: a
/// narrow or short terminal, or an empty inventory (nothing to detail),
/// yields the WHOLE column back to the table — the panel never overlaps the
/// table nor squeezes it below its usable width.
#[must_use]
pub(super) fn column_split(app: &TuiApp, content: Rect) -> (Rect, Option<Rect>) {
    let affords = !app.sorted_services().is_empty()
        && content.width >= MIN_CONTENT_WIDTH
        && content.height >= MIN_CONTENT_HEIGHT;
    if !affords {
        return (content, None);
    }
    let [table, details] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(COLUMN_WIDTH)]).areas(content);
    (table, Some(details))
}

/// Render the selected service's details column: the state triplet from the
/// inventory row and the read-only relation rows from the shell's canonical
/// dependency lifecycle. Every unavailable fact is a dash — never a zero,
/// an empty success, or another service's data.
pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let Some(service) = app.sorted_service_at(app.selected) else {
        return;
    };
    // The dependency channel carries at most one service's facts. A capture
    // aimed at another service (or a channel that never opened) must never
    // leak its relations into this panel; the rows degrade to dashes.
    let lifecycle = &app.shell.service_dependencies;
    let aimed_here = lifecycle.target() == Some(&service.id);
    let dependencies = if aimed_here {
        lifecycle.projected()
    } else {
        None
    };
    let mut lines = vec![
        fact_line(
            theme,
            t("svc.load_state"),
            honest_value(&service.load_state),
        ),
        fact_line(
            theme,
            t("svc.active_state"),
            honest_value(&service.active_state),
        ),
        fact_line(theme, t("svc.sub_state"), honest_value(&service.sub_state)),
    ];
    for (kind, key) in [
        (ServiceRelationKind::Requires, "svc.requires"),
        (ServiceRelationKind::Wants, "svc.wants"),
        (ServiceRelationKind::WantedBy, "svc.wanted_by"),
        (ServiceRelationKind::After, "svc.after"),
    ] {
        let value = dependencies
            .map_or_else(taskmanager_shell::presentation::missing_value, |deps| {
                relation_value(deps, &kind)
            });
        lines.push(fact_line(theme, t(key), value));
    }
    if aimed_here && lifecycle.is_loading() {
        // GPUI appends the same in-flight note while the shared lifecycle
        // resolves, including over a last-good snapshot (details.rs:269-276).
        lines.push(Line::from(Span::styled(
            t("svc.details_loading").to_owned(),
            Style::new().fg(theme.dim),
        )));
    }
    // The title carries the selected service's name — the same identity the
    // highlighted table row shows — so the panel can never be read against
    // the wrong row.
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(&service.name, theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// A triplet value that never reads as an empty success: an inventory row
/// whose provider left the field blank renders the shared missing-value dash.
fn honest_value(value: &str) -> String {
    if value.is_empty() {
        taskmanager_shell::presentation::missing_value()
    } else {
        value.to_owned()
    }
}

/// One relation row's value: space-joined targets from the canonical typed
/// graph, an honest dash when the service declares none of that kind, and
/// the GPUI parity truncation for long assemblies.
fn relation_value(dependencies: &ServiceDeps, kind: &ServiceRelationKind) -> String {
    let joined = dependencies
        .relation_targets(kind)
        .map(taskmanager_core::core::target::ServiceId::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        return taskmanager_shell::presentation::missing_value();
    }
    if joined.chars().count() > MAX_RELATION_CHARS {
        format!(
            "{}…",
            joined.chars().take(MAX_RELATION_CHARS).collect::<String>()
        )
    } else {
        joined
    }
}

fn fact_line<'a>(theme: TuiTheme, label: &'a str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{} ", text::pad_cells(label, LABEL_CELLS)),
            Style::new().fg(theme.dim),
        ),
        Span::styled(value, Style::new().fg(theme.color(Color::White))),
    ])
}
