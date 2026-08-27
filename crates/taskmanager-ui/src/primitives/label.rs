//! Text label with palette-derived colors (stateless visual primitive).

use gpui::{
    App, FontWeight, IntoElement, ParentElement, RenderOnce, Rgba, SharedString, Styled, Window,
    div,
};
use taskmanager_theme::Palette;

/// Typed text sizes (semantic, not pixel constants).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelSize {
    /// `text_xs`
    Small,
    /// `text_sm`
    Regular,
    /// `text_base`
    Large,
}

/// A static text label. Colors come from the palette snapshot; `muted` maps
/// to `Palette::fg_muted`.
#[derive(IntoElement)]
pub struct Label {
    text: SharedString,
    size: LabelSize,
    muted: bool,
    color: Option<Rgba>,
    weight: Option<FontWeight>,
}

impl Label {
    /// Build a label from any text-like value.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            size: LabelSize::Regular,
            muted: false,
            color: None,
            weight: None,
        }
    }

    /// Semantic size tier.
    #[must_use]
    pub fn size(mut self, size: LabelSize) -> Self {
        self.size = size;
        self
    }

    /// Use `Palette::fg_muted`.
    #[must_use]
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    /// Explicit palette color override.
    #[must_use]
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// Font weight override.
    #[must_use]
    pub fn weight(mut self, weight: impl Into<FontWeight>) -> Self {
        self.weight = Some(weight.into());
        self
    }
}

impl RenderOnce for Label {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut element = div().child(self.text);
        match self.size {
            LabelSize::Small => element = element.text_xs(),
            LabelSize::Regular => element = element.text_sm(),
            LabelSize::Large => element = element.text_base(),
        }
        if let Some(weight) = self.weight {
            element = element.font_weight(weight);
        }
        // Color is resolved by the consumer through `Palette`; the element
        // itself never invents a hue.
        element
    }
}

/// A label with a resolved palette color applied at build time.
#[derive(IntoElement)]
pub struct PaletteLabel {
    text: SharedString,
    palette: Palette,
    size: LabelSize,
    muted: bool,
    weight: Option<FontWeight>,
}

impl PaletteLabel {
    /// Build a label that applies `palette.fg` (or `fg_muted` when `muted`).
    pub fn new(text: impl Into<SharedString>, palette: Palette) -> Self {
        Self {
            text: text.into(),
            palette,
            size: LabelSize::Regular,
            muted: false,
            weight: None,
        }
    }

    #[must_use]
    pub fn size(mut self, size: LabelSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    #[must_use]
    pub fn weight(mut self, weight: impl Into<FontWeight>) -> Self {
        self.weight = Some(weight.into());
        self
    }
}

impl RenderOnce for PaletteLabel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let color = if self.muted {
            self.palette.fg_muted
        } else {
            self.palette.fg
        };
        let mut element = div().text_color(color).child(self.text);
        match self.size {
            LabelSize::Small => element = element.text_xs(),
            LabelSize::Regular => element = element.text_sm(),
            LabelSize::Large => element = element.text_base(),
        }
        if let Some(weight) = self.weight {
            element = element.font_weight(weight);
        }
        element
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_label_tests.rs"]
mod tests;
