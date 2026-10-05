//! Complete selected health groups in a bounded terminal modal.
use super::alerts::{metric_label, metric_unit};
use crate::health_review::HealthReviewMode;
use crate::{TuiApp, TuiTheme};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::Paragraph,
};
use taskmanager_application::i18n::t;
use taskmanager_application::truncate_text;
use taskmanager_core::core::alerts::Alert;
use taskmanager_shell::presentation::health_review::{
    HealthReviewGroup, HealthReviewRow, sensor_groups, storage_groups,
};
pub(crate) fn groups(app: &TuiApp) -> Vec<HealthReviewGroup> {
    let projection = app.projection();
    match app.health_review.mode {
        HealthReviewMode::Storage => {
            let disks = projection
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.disks.as_slice())
                .unwrap_or_default();
            let (reports, _) = projection.smart_projection();
            storage_groups(
                projection
                    .storage_health_projection()
                    .map(|(snapshot, _)| snapshot),
                disks,
                reports.observations(),
            )
        }
        HealthReviewMode::Sensors => sensor_groups(projection.sensors.as_ref())
            .into_iter()
            .flat_map(|group| {
                group.rows.into_iter().map(move |row| HealthReviewGroup {
                    title: format!("{} · {}", group.title, row.label),
                    rows: vec![row],
                })
            })
            .collect(),
        HealthReviewMode::ActiveAlerts => projection
            .alert_active
            .iter()
            .map(|alert| alert_group(alert, "Active alert"))
            .collect(),
        HealthReviewMode::Events => projection
            .alert_center
            .event_history()
            .iter()
            .filter(|event| {
                app.health_review
                    .event_filter
                    .is_none_or(|kind| kind == event.kind)
            })
            .map(|event| {
                let mut group = alert_group(
                    &event.alert,
                    &format!("Event {} · {:?}", event.id, event.kind),
                );
                group.rows.push(HealthReviewRow {
                    label: "Observed at".into(),
                    value: format!("{} ms", event.observed_at_ms),
                });
                group
            })
            .collect(),
        HealthReviewMode::Rules => Vec::new(),
    }
}
fn alert_group(alert: &Alert, context: &str) -> HealthReviewGroup {
    let unit = metric_unit(alert.metric);
    HealthReviewGroup {
        title: format!("{context} · {}", metric_label(alert.metric)),
        rows: vec![
            HealthReviewRow {
                label: "Severity".into(),
                value: format!("{:?}", alert.severity),
            },
            HealthReviewRow {
                label: "Observed / threshold".into(),
                value: format!("{:.1}{unit} / {:.1}{unit}", alert.value, alert.threshold),
            },
            HealthReviewRow {
                label: "Target".into(),
                value: alert.target.clone(),
            },
            HealthReviewRow {
                label: "Rule".into(),
                value: alert.rule_id.clone(),
            },
        ],
    }
}
pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, inner: Rect) {
    let [heading, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(inner);
    let groups = groups(app);
    let selected = app
        .health_review
        .selected
        .min(groups.len().saturating_sub(1));
    if let Some(group) = groups.get(selected) {
        frame.render_widget(
            Paragraph::new(truncate_text(
                &format!("{} · {}/{}", group.title, selected + 1, groups.len()),
                usize::from(heading.width),
            )),
            heading,
        );
        let lines = group
            .rows
            .iter()
            .map(|row| {
                Line::from(truncate_text(
                    &format!("{}: {}", row.label, row.value),
                    usize::from(body.width),
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), body);
    } else {
        frame.render_widget(Paragraph::new("No observations in this view"), body);
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Up/Down groups · p Active · e Events"),
            Line::from(if app.health_review.mode == HealthReviewMode::Events {
                format!(
                    "F2 Filter {:?} · F5 Export · F8 Clear",
                    app.health_review.event_filter
                )
            } else {
                "w Storage · s Sensors · q Rules".into()
            }),
            Line::from(format!(
                "z Short test · x Extended test · h / Esc {}",
                t("chrome.close")
            )),
        ])
        .style(ratatui::style::Style::new().fg(theme.dim)),
        footer,
    );
}
