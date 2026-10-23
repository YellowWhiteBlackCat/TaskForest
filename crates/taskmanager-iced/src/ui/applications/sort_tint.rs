//! The sorted-column tint: the faint accent wash the active sort column wears
//! across its header cell and every body cell of the Applications table.
//!
//! The wash is translucent, so the row's own surface (selection tint, zebra
//! stripe, plain backdrop) still reads through it: the column tint marks the
//! sort axis without hiding the row state. The color comes from the theme's
//! own faint accent wash token — no color literal enters this module.

use iced::widget::container;
use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;

/// The surface wash of one column of the Applications table.
///
/// `Some` for the active sort column (the whole column wears the faint accent
/// wash), `None` for every other column (no surface at all, so the row's own
/// background shows unchanged).
#[must_use]
pub(crate) fn sort_column_tint(
    theme: &Theme,
    active: SortCol,
    column: SortCol,
) -> Option<iced::Color> {
    (active == column).then(|| crate::theme_binding::color(theme.palette().hover))
}

/// The container style of one tinted cell: the translucent wash over the
/// row's own surface.
#[must_use]
pub(crate) fn sort_tint_style(
    theme: &Theme,
    active: SortCol,
    column: SortCol,
) -> Option<container::Style> {
    sort_column_tint(theme, active, column).map(|tint| container::Style {
        background: Some(tint.into()),
        ..container::Style::default()
    })
}
