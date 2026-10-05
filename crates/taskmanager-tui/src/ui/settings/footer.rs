//! Mandatory action hints and whole-group admission for optional explanation.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use taskmanager_application::i18n::t;

use super::SettingsForm;
use crate::TuiTheme;
use crate::ui::process_details::wrapped_content_height;

pub(super) struct SettingsFooter {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) height: u16,
}

pub(super) fn settings_footer(
    form: &SettingsForm,
    focused_field: Option<usize>,
    theme: TuiTheme,
    inner: Rect,
    setup_available: bool,
) -> SettingsFooter {
    let action = if focused_field.is_some_and(|field| field >= 30) {
        t("settings.privileges_authorize")
    } else {
        t("common.apply")
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("↑↓/Tab · Enter {action} · Esc {}", t("common.cancel")),
        Style::new().fg(theme.accent),
    ))];
    lines.push(Line::from(Span::styled(
        format!("F3 {}", t("sidebar.edit_devices")),
        Style::new().fg(theme.accent),
    )));
    if setup_available {
        lines.push(Line::from(Span::styled(
            format!("F2 {}", t("settings.additional_setup_open")),
            Style::new().fg(theme.accent),
        )));
    }
    if let Some(error) = form.save_error.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("✗ {error}"),
            Style::new().fg(theme.danger),
        )));
    }
    let optional = vec![
        Line::from(Span::styled(
            t("settings.footer_hint"),
            Style::new().fg(theme.dim),
        )),
        Line::from(Span::styled(
            t("settings.font_terminal_hint"),
            Style::new().fg(theme.dim),
        )),
    ];
    // One blank row protects the bottom edge. Eight body rows keep the
    // permission group navigable at the terminal's smallest admitted size.
    let mandatory_height = wrapped_content_height(&lines, inner.width);
    let optional_height = wrapped_content_height(&optional, inner.width);
    if mandatory_height + optional_height + 1 + 8 <= usize::from(inner.height) {
        lines.extend(optional);
    }
    let height = u16::try_from(wrapped_content_height(&lines, inner.width).saturating_add(1))
        .unwrap_or(u16::MAX)
        .min(inner.height);
    SettingsFooter { lines, height }
}
