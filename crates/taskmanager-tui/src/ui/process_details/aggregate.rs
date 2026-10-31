//! Compact table cells and full selected-row details for process aggregates.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Wrap};
use taskforest_core::core::metrics::ScalarAvailability;
use taskforest_core::core::process::aggregate::AggregateMetric;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{bytes, control_error_detail};

use crate::TuiTheme;
use crate::ui::{kv, panel};

/// One-line CPU readout for a group header. Partial and stale rows keep the
/// value's availability visible with `P` / `S` plus member coverage; the
/// selected group's details panel expands the marker into the localized
/// reason. Stale values come only from `last_known_value`.
pub(super) fn cpu_cell(metric: &AggregateMetric<f32>) -> Cell<'static> {
    Cell::from(metric_cell_text(metric, 8, compact_cpu_value))
}

/// One-line memory readout for a group header. The cell uses compact byte
/// units only when state and coverage need the room; selected details retain
/// the full shared byte spelling and reason.
pub(super) fn memory_cell(metric: &AggregateMetric<u64>) -> Cell<'static> {
    Cell::from(metric_cell_text(metric, 10, compact_memory_value))
}

fn metric_cell_text<T>(
    metric: &AggregateMetric<T>,
    width: usize,
    format_value: impl Fn(&T, usize) -> String,
) -> String {
    match metric.availability() {
        ScalarAvailability::Available => metric.current_value().map_or_else(
            || compact_localized_label(t("proc_insights.unknown"), width),
            |value| format_value(value, width),
        ),
        ScalarAvailability::Partial(_) => {
            let suffix = coverage_suffix(
                'P',
                metric.current_member_count(),
                metric.member_count(),
                width,
            );
            let value_width = width.saturating_sub(display_width(&suffix));
            let value = metric
                .current_value()
                .map_or_else(String::new, |value| format_value(value, value_width));
            format!("{value}{suffix}")
        }
        ScalarAvailability::Stale(_) => {
            let suffix = coverage_suffix(
                'S',
                metric.known_member_count(),
                metric.member_count(),
                width,
            );
            let value_width = width.saturating_sub(display_width(&suffix));
            let value = metric
                .last_known_value()
                .map_or_else(String::new, |value| format_value(value, value_width));
            format!("{value}{suffix}")
        }
        ScalarAvailability::Unknown => compact_localized_label(t("proc_insights.unknown"), width),
        ScalarAvailability::Unavailable(_) => {
            compact_localized_label(t("proc_insights.unavailable"), width)
        }
    }
}

fn coverage_suffix(marker: char, covered: usize, total: usize, width: usize) -> String {
    let exact = format!("{marker}{covered}/{total}");
    if display_width(&exact) <= width {
        exact
    } else if width > 1 {
        format!("{marker}…")
    } else if width == 1 {
        marker.to_string()
    } else {
        String::new()
    }
}

fn compact_localized_label(label: &str, width: usize) -> String {
    if display_width(label) <= width {
        return label.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let ellipsis = "…";
    let budget = width.saturating_sub(display_width(ellipsis));
    let mut clipped = String::new();
    for character in label.chars() {
        let mut candidate = clipped.clone();
        candidate.push(character);
        if display_width(&candidate) > budget {
            break;
        }
        clipped.push(character);
    }
    clipped.push_str(ellipsis);
    clipped
}

fn compact_cpu_value(value: &f32, width: usize) -> String {
    let candidates = [
        format!("{value:.1}%"),
        format!("{value:.0}%"),
        format!("{value:.1}"),
        format!("{value:.0}"),
        format!("{value:.1e}"),
    ];
    candidates
        .into_iter()
        .find(|candidate| display_width(candidate) <= width)
        .unwrap_or_else(|| compact_localized_label(&format!("{value:.0}"), width))
}

fn compact_memory_value(value: &u64, width: usize) -> String {
    let standard = bytes(*value);
    if display_width(&standard) <= width {
        return standard;
    }
    let compacted = standard.replace(' ', "");
    if display_width(&compacted) <= width {
        return compacted;
    }
    compact_scaled_bytes(*value, width).unwrap_or_else(|| compact_localized_label(&standard, width))
}

/// Render a compact byte amount with exact integer scaling. This path is used
/// only when a status suffix leaves less space than the shared `bytes` text.
fn compact_scaled_bytes(value: u64, width: usize) -> Option<String> {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let value = u128::from(value);
    let mut divisor = 1_u128;
    let mut unit = 0;
    while value >= divisor * 1024 && unit + 1 < UNITS.len() {
        divisor *= 1024;
        unit += 1;
    }

    while unit < UNITS.len() {
        let whole = value / divisor;
        let rounded_tenths = ((value % divisor) * 10 + divisor / 2) / divisor;
        let (whole, tenths) = if rounded_tenths == 10 {
            (whole + 1, 0)
        } else {
            (whole, rounded_tenths)
        };
        for candidate in [
            format!("{whole}.{tenths}{}", UNITS[unit]),
            format!("{whole}{}", UNITS[unit]),
        ] {
            if display_width(&candidate) <= width {
                return Some(candidate);
            }
        }
        if unit + 1 == UNITS.len() {
            break;
        }
        divisor *= 1024;
        unit += 1;
    }
    None
}

fn display_width(text: &str) -> usize {
    Line::from(text).width()
}

/// Full selected-group readout. The table cell has a fixed narrow width, so
/// its compact marker is expanded here with the canonical aggregate's status,
/// exact coverage, and shared localized failure detail.
fn metric_detail<T>(metric: &AggregateMetric<T>, format_value: impl Fn(&T) -> String) -> String {
    match metric.availability() {
        ScalarAvailability::Available => metric
            .current_value()
            .map_or_else(|| t("proc_insights.unknown").to_owned(), format_value),
        ScalarAvailability::Partial(failure) => metric.current_value().map_or_else(
            || t("proc_insights.unknown").to_owned(),
            |value| {
                format!(
                    "{} · {}/{} · {}: {}",
                    format_value(value),
                    metric.current_member_count(),
                    metric.member_count(),
                    t("disk.usage_partial"),
                    control_error_detail(failure),
                )
            },
        ),
        ScalarAvailability::Stale(failure) => metric.last_known_value().map_or_else(
            || t("proc_insights.unknown").to_owned(),
            |value| {
                format!(
                    "{} · {} · {}/{} · {}",
                    format_value(value),
                    t("device.stale"),
                    metric.known_member_count(),
                    metric.member_count(),
                    control_error_detail(failure),
                )
            },
        ),
        ScalarAvailability::Unknown => t("proc_insights.unknown").to_owned(),
        ScalarAvailability::Unavailable(failure) => format!(
            "{}: {}",
            t("proc_insights.unavailable"),
            control_error_detail(failure)
        ),
    }
}

pub(super) fn render_group_details(
    frame: &mut Frame<'_>,
    theme: TuiTheme,
    area: Rect,
    label: &str,
    count: usize,
    cpu: &AggregateMetric<f32>,
    memory: &AggregateMetric<u64>,
) {
    let lines = vec![
        Line::from(Span::styled(
            format!("{label} ×{count}"),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        kv(
            t("common.cpu"),
            metric_detail(cpu, |value| format!("{value:.1}%")),
            theme,
        ),
        kv(
            t("common.memory"),
            metric_detail(memory, |value| bytes(*value)),
            theme,
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(t("prop.process_details"), theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}
