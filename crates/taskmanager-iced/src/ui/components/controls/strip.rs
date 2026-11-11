//! Bounded action strips: one horizontal scroll owner for a full action
//! vocabulary.
//!
//! A row of actions is wider than a narrow viewport. Letting a `Fill` row
//! shrink its children turns a whole control into a half-painted fragment at
//! the right edge; this component gives the strip its own bounded viewport
//! and keeps every child's intrinsic width, so the lower table never loses
//! vertical space and the right edge never shows a half group.

use iced::Length;
use iced::widget::row;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use crate::ui::components::IcedElement;
use crate::ui::components::{ScrollAxis, ScrollScale, scrollable};

/// One bounded horizontal viewport around complete action groups.
///
/// The caller declares the tallest control role in the strip. Its shared
/// extent bounds the viewport; role padding stays inside the controls.
#[must_use]
pub(crate) fn bounded_toolbar<'a>(
    theme_snapshot: &'a Theme,
    extent_role: ControlRole,
    children: Vec<IcedElement<'a>>,
) -> IcedElement<'a> {
    let extent = crate::theme::control_extent(theme_snapshot, extent_role);
    let band = row(children)
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center)
        .width(Length::Shrink);
    scrollable(band, ScrollAxis::Horizontal, ScrollScale::Standard)
        .height(Length::Fixed(extent))
        .width(Length::Fill)
        .into()
}
