//! Complete preset groups and fixed native action hints in the terminal review.
use crate::saved_views::SavedViewInput;
use crate::{TuiApp, TuiSurface, TuiTheme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};
use taskmanager_application::i18n::t;
use taskmanager_shell::saved_views::{feedback_text, review_rows};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, popup: Rect) {
    frame.render_widget(Clear, popup);
    let block = super::panel(t("saved_views.title"), theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let Some(TuiSurface::SavedViews { selected, input }) = app.local_surface() else {
        return;
    };
    let body_height = inner.height.saturating_sub(4);
    let status = match input {
        SavedViewInput::AwaitingImport => Some(t("saved_views.paste_json").to_owned()),
        SavedViewInput::Browsing => app.saved_views.feedback.map(feedback_text),
    };
    let capacity = usize::from(body_height.saturating_sub(u16::from(status.is_some())) / 3);
    let rows: Vec<_> = review_rows(&app.saved_views.rows).collect();
    let index = selected
        .and_then(|id| rows.iter().position(|entry| entry.id == id))
        .unwrap_or(0);
    let start = index.saturating_sub(capacity.saturating_sub(1));
    let mut lines = Vec::new();
    if let Some(status) = status {
        lines.push(Line::from(status));
    }
    for preset in rows.iter().skip(start).take(capacity) {
        lines.push(Line::from(format!(
            "{} {}",
            if *selected == Some(preset.id) {
                ">"
            } else {
                " "
            },
            preset.display_name()
        )));
        lines.push(Line::from(format!(
            "  {} · {} {}",
            preset.filter.label(),
            preset.sort_col.label(),
            if preset.sort_asc { "↑" } else { "↓" }
        )));
        lines.push(Line::from(format!(
            "  {}",
            t("saved_views.hidden_columns")
                .replace("{count}", &preset.hidden_cols.len().to_string())
        )));
    }
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(inner.x, inner.y, inner.width, body_height),
    );
    let footer = vec![
        Line::from(format!(
            "↑↓ · Enter {} · Esc {}",
            t("common.apply"),
            t("common.close")
        )),
        Line::from(format!(
            "F2 {} · Del {}",
            t("saved_views.save_current"),
            t("common.remove")
        )),
        Line::from(format!(
            "F5 {} · F6 {}",
            t("common.export"),
            t("common.import")
        )),
    ];
    frame.render_widget(
        Paragraph::new(footer).style(Style::new().fg(theme.accent)),
        Rect::new(
            inner.x,
            inner.y.saturating_add(body_height),
            inner.width,
            inner.height.saturating_sub(body_height).min(3),
        ),
    );
}
