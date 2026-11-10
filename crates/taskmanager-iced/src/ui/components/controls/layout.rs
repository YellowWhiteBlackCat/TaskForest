//! Non-interactive layout primitives: labels, section headers, toolbars.
//!
//! These components spend theme-owned typography and spacing tokens only.
//! They do not create a second color or measurement authority.

use iced::Length;
use iced::advanced::text::IntoFragment;
use iced::widget::{row, text};
use taskmanager_theme::{Theme, tokens};

use crate::ui::components::IcedElement;
use crate::ui::components::divider;

/// A labeled section title followed by the shared glass hairline.
#[must_use]
pub(crate) fn section_header<'a>(
    theme_snapshot: &'a Theme,
    title: impl IntoFragment<'a>,
) -> IcedElement<'a> {
    let fg = crate::theme_binding::color(theme_snapshot.palette().fg);
    row![
        text(title).size(f32::from(tokens::FONT_14)).color(fg).font(
            crate::theme_binding::ui_font_weight(theme_snapshot, tokens::FONT_WEIGHT_SEMIBOLD,)
        ),
        divider(theme_snapshot),
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .align_y(iced::Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// A wrapping action strip with the shared spacing gutter.
#[must_use]
pub(crate) fn toolbar<'a>(children: Vec<IcedElement<'a>>) -> IcedElement<'a> {
    row(children)
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center)
        .width(Length::Fill)
        .wrap()
        .into()
}

/// A static label whose ink and type size are theme-owned.
#[must_use]
pub(crate) fn label<'a>(
    theme_snapshot: &'a Theme,
    body: impl IntoFragment<'a>,
    muted: bool,
) -> IcedElement<'a> {
    let palette = theme_snapshot.palette();
    text(body)
        .size(f32::from(tokens::FONT_13))
        .color(crate::theme_binding::color(if muted {
            palette.fg_muted
        } else {
            palette.fg
        }))
        .into()
}
