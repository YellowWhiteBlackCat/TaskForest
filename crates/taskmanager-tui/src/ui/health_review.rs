//! Complete selected health groups in a bounded terminal modal.
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
use taskmanager_shell::presentation::health_review::{
    HealthReviewGroup, sensor_groups, storage_groups,
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
        HealthReviewMode::Rules => Vec::new(),
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
    let Some(group) = groups.get(selected) else {
        return;
    };
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
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Up/Down groups · w Storage · s Sensors · q Rules"),
            Line::from("z Short test · x Extended test"),
            Line::from(format!("h / Esc {}", t("chrome.close"))),
        ])
        .style(ratatui::style::Style::new().fg(theme.dim)),
        footer,
    );
}
