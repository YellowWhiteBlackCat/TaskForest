//! Renderer-local scroll state for vertical tables and horizontal device strips.

/// Viewport state shared by Iced's virtual table and Performance rail paths.
/// The renderer learns the actual extents after layout; until then callers use
/// a bounded window-size fallback so the first render never becomes eager.
#[derive(Clone)]
pub(crate) struct VirtualScrollState {
    offset_x: f32,
    offset_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    id: iced::widget::Id,
}

impl VirtualScrollState {
    pub(crate) fn new() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            id: iced::widget::Id::unique(),
        }
    }

    pub(crate) fn offset_x(&self) -> f32 {
        self.offset_x.max(0.0)
    }

    pub(crate) fn offset_y(&self) -> f32 {
        self.offset_y.max(0.0)
    }

    pub(crate) fn viewport_height(&self, fallback: f32) -> f32 {
        if self.viewport_height.is_finite() && self.viewport_height > 0.0 {
            self.viewport_height
        } else if fallback.is_finite() && fallback > 0.0 {
            fallback
        } else {
            240.0
        }
    }

    pub(crate) fn viewport_width(&self, fallback: f32) -> f32 {
        if self.viewport_width.is_finite() && self.viewport_width > 0.0 {
            self.viewport_width
        } else if fallback.is_finite() && fallback > 0.0 {
            fallback
        } else {
            240.0
        }
    }

    pub(crate) fn id(&self) -> iced::widget::Id {
        self.id.clone()
    }

    pub(crate) fn update_from_viewport(&mut self, viewport: iced::widget::scrollable::Viewport) {
        let offset = viewport.absolute_offset();
        if offset.x.is_finite() {
            self.offset_x = offset.x.max(0.0);
        }
        if offset.y.is_finite() {
            self.offset_y = offset.y.max(0.0);
        }
        let bounds = viewport.bounds();
        if bounds.width.is_finite() && bounds.width > 0.0 {
            self.viewport_width = bounds.width;
        }
        if bounds.height.is_finite() && bounds.height > 0.0 {
            self.viewport_height = bounds.height;
        }
    }

    pub(crate) fn set_offset(&mut self, offset_y: f32) {
        if offset_y.is_finite() {
            self.offset_y = offset_y.max(0.0);
        }
    }

    pub(crate) fn invalidate_viewport(&mut self) {
        self.viewport_width = 0.0;
        self.viewport_height = 0.0;
    }

    /// Calculate the target `offset_y` required to ensure the row at `row_index`
    /// (with height `row_height`) is fully visible within the current viewport
    /// (with `header_height` reserved at top). If the row is already fully
    /// visible, returns `None`.
    #[must_use]
    pub(crate) fn ensure_row_visible(
        &self,
        row_index: usize,
        row_height: f32,
        header_height: f32,
        viewport_fallback: f32,
    ) -> Option<f32> {
        if !row_height.is_finite() || row_height <= 0.0 {
            return None;
        }
        let vp_height = self.viewport_height(viewport_fallback);
        let header = if header_height.is_finite() {
            header_height.max(0.0)
        } else {
            0.0
        };
        let current_y = self.offset_y();
        let row_top = header + row_index as f32 * row_height;
        let row_bottom = row_top + row_height;

        if row_top < current_y {
            Some(row_top)
        } else if row_bottom > current_y + vp_height {
            Some((row_bottom - vp_height).max(0.0))
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/scroll_tests.rs"]
mod tests;
