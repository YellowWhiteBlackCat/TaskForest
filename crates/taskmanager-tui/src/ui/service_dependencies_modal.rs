//! Service dependencies browsing modal (Services page).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{ServiceDeps, ServiceRelationKind};
use taskmanager_ui_contract::IconId;

use super::containers::{KeyHint, Modal};
use crate::{ServiceDependenciesTarget, TuiApp, TuiTheme};

/// Render the service dependencies modal overlay.
pub(super) fn render_service_dependencies_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    target: &ServiceDependenciesTarget,
    theme: TuiTheme,
    popup: Rect,
) {
    let title = format!("{} · {}", t("svc.dependencies"), target.service_name);
    let inner = Modal::new(theme, IconId::Service, &title).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);

    let lifecycle = &app.shell.service_dependencies;
    let aimed_here = lifecycle.target() == Some(&target.service_id);
    let dependencies = if aimed_here {
        lifecycle.projected()
    } else {
        None
    };

    let mut lines = Vec::new();

    if aimed_here && lifecycle.is_loading() {
        lines.push(Line::from(Span::styled(
            t("svc.details_loading"),
            Style::new().fg(theme.dim),
        )));
    } else if let Some(deps) = dependencies {
        lines.extend(build_relation_section(
            theme,
            t("svc.requires"),
            deps,
            &ServiceRelationKind::Requires,
        ));
        lines.push(Line::from(""));
        lines.extend(build_relation_section(
            theme,
            t("svc.wants"),
            deps,
            &ServiceRelationKind::Wants,
        ));
        lines.push(Line::from(""));
        lines.extend(build_relation_section(
            theme,
            t("svc.wanted_by"),
            deps,
            &ServiceRelationKind::WantedBy,
        ));
        lines.push(Line::from(""));
        lines.extend(build_relation_section(
            theme,
            t("svc.after"),
            deps,
            &ServiceRelationKind::After,
        ));
    } else {
        lines.push(Line::from(Span::styled(
            t("svc.details_loading"),
            Style::new().fg(theme.dim),
        )));
    }

    let total_lines = lines.len();
    let visible_height = usize::from(body.height);
    let scroll = target
        .scroll
        .min(total_lines.saturating_sub(visible_height));
    let visible_lines = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(visible_lines), body);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            KeyHint::line(
                theme,
                vec![
                    (" ↑/↓ ", "Scroll".to_string()),
                    (" Esc / d / q ", "Close".to_string()),
                ],
            ),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn build_relation_section<'a>(
    theme: TuiTheme,
    header: &'a str,
    deps: &ServiceDeps,
    kind: &ServiceRelationKind,
) -> Vec<Line<'a>> {
    let targets: Vec<&str> = deps
        .relation_targets(kind)
        .map(taskmanager_core::core::target::ServiceId::as_str)
        .collect();
    if targets.is_empty() {
        return vec![Line::from(Span::styled(
            format!("{header}: —"),
            Style::new().fg(theme.dim),
        ))];
    }
    let mut out = vec![Line::from(Span::styled(
        format!("{header} ({}):", targets.len()),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    for chunk in targets.chunks(3) {
        let joined = chunk.join("  ");
        out.push(Line::from(Span::styled(
            format!("  {joined}"),
            Style::new().fg(theme.color(Color::White)),
        )));
    }
    out
}
