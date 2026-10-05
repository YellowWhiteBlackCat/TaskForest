//! Whole device rows in a bounded body, with three fixed action lines and bottom inset.

use crate::{TuiApp, TuiSurface, TuiTheme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};
use taskmanager_application::i18n::t;

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, popup: Rect) {
    frame.render_widget(Clear, popup);
    let block = super::panel(t("sidebar.edit_devices"), theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let Some(TuiSurface::SidebarEditor { selected }) = app.local_surface() else {
        return;
    };
    let entries = app.sidebar_entries();
    let index = selected
        .as_ref()
        .and_then(|key| entries.iter().position(|entry| &entry.key == key))
        .unwrap_or(0);
    let body_height = inner.height.saturating_sub(4);
    let start = index.saturating_sub(usize::from(body_height).saturating_sub(1));
    for (offset, entry) in entries
        .iter()
        .skip(start)
        .take(usize::from(body_height))
        .enumerate()
    {
        let focused = selected.as_ref() == Some(&entry.key);
        let line = format!(
            "{} [{}] {}",
            if focused { ">" } else { " " },
            if entry.visible { "x" } else { " " },
            entry.label
        );
        frame.render_widget(
            Paragraph::new(line).style(if focused {
                Style::new().fg(theme.accent)
            } else {
                Style::new()
            }),
            Rect::new(
                inner.x,
                inner
                    .y
                    .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
                inner.width,
                1,
            ),
        );
    }
    let footer = vec![
        Line::from(format!(
            "↑↓ · Space {}/{}",
            t("sidebar.show_device"),
            t("sidebar.hide_device")
        )),
        Line::from(format!(
            "←→ {}/{} · Enter",
            t("sidebar.move_up"),
            t("sidebar.move_down")
        )),
        Line::from(format!("Esc {}", t("common.close"))),
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
