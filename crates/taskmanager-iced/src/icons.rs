//! Iced's native adapter for the shared semantic SVG registry.
//!
//! The asset identity and bytes come from `taskmanager-icons`/`taskmanager-assets`;
//! this module owns only the Iced `svg` widget, sizing and theme tint. It is
//! intentionally not a GPUI element wrapper.

use iced::widget::{svg, text};
use iced::{Element, Length};
use taskmanager_theme::Theme;
use taskmanager_ui_contract::IconId;

use crate::app::Message;

/// Render one semantic icon using the embedded SVG asset and a theme-derived
/// tint. Missing optional assets degrade to a visible text marker rather than
/// panicking or silently claiming that an icon was rendered.
pub(crate) fn icon<'a>(
    theme_snapshot: &Theme,
    id: IconId,
    size: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let tint = crate::theme_binding::color(theme_snapshot.palette().fg);
    let Some(bytes) = taskmanager_icons::asset_bytes(id) else {
        return text("·")
            .size(size)
            .style(move |_theme| iced::widget::text::Style { color: Some(tint) })
            .into();
    };

    svg::Svg::new(svg::Handle::from_memory(bytes))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme, _status| svg::Style { color: Some(tint) })
        .into()
}

#[cfg(test)]
#[path = "../tests/gui/icons_tests.rs"]
mod tests;
