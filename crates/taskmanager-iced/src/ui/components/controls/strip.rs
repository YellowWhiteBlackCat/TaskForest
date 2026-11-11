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
/// The height is the shared control extent, not an action-local number: the
/// strip spends the same `ControlRole::Button` measurement the buttons spend.
#[must_use]
pub(crate) fn bounded_toolbar<'a>(
    theme_snapshot: &'a Theme,
    children: Vec<IcedElement<'a>>,
) -> IcedElement<'a> {
    // The strip may carry Button and Input roles. Spend the shared Button
    // extent plus the Input role's vertical padding so a select's focus
    // shell is never clipped by a button-height viewport.
    let extent = crate::theme::control_extent(theme_snapshot, ControlRole::Button)
        + crate::theme::control_padding_y(theme_snapshot, ControlRole::Input);
    let band = row(children)
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center)
        .width(Length::Shrink);
    scrollable(band, ScrollAxis::Horizontal, ScrollScale::Standard)
        .height(Length::Fixed(extent))
        .width(Length::Fill)
        .into()
}
