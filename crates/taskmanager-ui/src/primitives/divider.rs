//! Hairline divider (stateless visual primitive).

use gpui::{App, IntoElement, RenderOnce, Styled, Window, div};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

/// Divider orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DividerOrientation {
    /// 1px vertical line (separates items in a row).
    Vertical,
    /// 1px horizontal line (separates stacked sections).
    Horizontal,
}

/// A 1px divider in the palette's border color.
#[derive(IntoElement)]
pub struct Divider {
    orientation: DividerOrientation,
    palette: Palette,
    /// When true, omit the built-in `mx`/`my` margin. Use this inside a
    /// `gap(...)` flex parent where the parent gap already supplies inter-item
    /// spacing — otherwise gap + margin stack and each divider breaches ~4× the
    /// button-to-button rhythm (the "fragmented" feeling).
    flush: bool,
}

impl Divider {
    /// Build a divider that carries its own `mx`/`my(SPACE_8)` breathing room
    /// (the default for standalone placement).
    pub fn new(orientation: DividerOrientation, palette: Palette) -> Self {
        Self {
            orientation,
            palette,
            flush: false,
        }
    }

    /// Margin-less variant for use INSIDE a `gap(...)` flex container, where the
    /// parent gap already supplies inter-item spacing (avoids gap+mx stacking).
    pub fn new_flush(orientation: DividerOrientation, palette: Palette) -> Self {
        Self {
            orientation,
            palette,
            flush: true,
        }
    }
}

impl RenderOnce for Divider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        match self.orientation {
            DividerOrientation::Vertical => {
                let mut d = div().w_px().h_full().bg(self.palette.border);
                if !self.flush {
                    d = d.mx(tokens::SPACE_8);
                }
                d
            }
            DividerOrientation::Horizontal => {
                let mut d = div().h_px().w_full().bg(self.palette.border);
                if !self.flush {
                    d = d.my(tokens::SPACE_8);
                }
                d
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_divider_tests.rs"]
mod tests;
