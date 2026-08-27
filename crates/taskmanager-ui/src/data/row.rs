//! Shared table/data-row geometry.

use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px};
use taskmanager_theme::{Color, Length, Palette, tokens};

/// A horizontal data row whose cells remain caller-owned slots.
#[derive(IntoElement)]
pub struct DataRow {
    cells: Vec<AnyElement>,
    background: Color,
    padding_x: Length,
    padding_y: Length,
    radius: Length,
    bottom_border: Option<Color>,
}

impl DataRow {
    /// Build a row with the standard compact table rhythm.
    pub fn new(palette: Palette) -> Self {
        Self {
            cells: Vec::new(),
            background: palette.surface,
            padding_x: tokens::SPACE_10,
            padding_y: tokens::SPACE_7,
            radius: palette.control_radius,
            bottom_border: None,
        }
    }

    /// Add one already-sized cell.
    #[must_use]
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.cells.push(child.into_any_element());
        self
    }

    /// Set the row fill.
    #[must_use]
    pub fn background(mut self, background: Color) -> Self {
        self.background = background;
        self
    }

    /// Set horizontal padding.
    #[must_use]
    pub fn padding_x(mut self, padding: Length) -> Self {
        self.padding_x = padding;
        self
    }

    /// Set vertical padding.
    #[must_use]
    pub fn padding_y(mut self, padding: Length) -> Self {
        self.padding_y = padding;
        self
    }

    /// Set the corner radius.
    #[must_use]
    pub fn radius(mut self, radius: Length) -> Self {
        self.radius = radius;
        self
    }

    /// Add a bottom separator, useful for flat history rows.
    #[must_use]
    pub fn bottom_border(mut self, color: Color) -> Self {
        self.bottom_border = Some(color);
        self
    }

    /// Render the row.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let mut row = div()
            .flex()
            .flex_row()
            .w_full()
            .min_w(px(0.0))
            .items_center()
            .gap(tokens::SPACE_8)
            .px(self.padding_x)
            .py(self.padding_y)
            .rounded(self.radius)
            .bg(self.background)
            .children(self.cells);
        if let Some(color) = self.bottom_border {
            row = row.border_b_1().border_color(color);
        }
        row
    }
}

impl RenderOnce for DataRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
