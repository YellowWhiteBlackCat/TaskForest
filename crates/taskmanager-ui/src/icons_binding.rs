//! The GPUI icon adapter (ADR-017 Phase 2, relocated by ADR-051).
//!
//! The toolkit-neutral semantic identity ([`taskmanager_ui_contract::IconId`])
//! stays in `taskmanager-ui-contract`; the embedded SVG assets stay in
//! `taskmanager-assets`; the neutral path/bytes table stays in
//! `taskmanager-icons`. This module owns the GPUI mapping between them:
//!
//! - [`icon`] — build a tintable GPUI SVG icon element. Color inherits from
//!   the surrounding text style at layout time, and callers can keep chaining
//!   GPUI style methods (`.size(..)`, `.text_color(..)`, …) on the result.
//! - [`application_image`] — build a GPUI image element from
//!   provider-resolved application-icon bytes.
//!
//! No `gpui_component` types are used or re-exported.

use std::sync::Arc;

use gpui::{
    App, Image, ImageFormat, Img, IntoElement, Refineable, RenderOnce, StyleRefinement, Styled,
    Window, img, svg,
};
use taskmanager_icons::path;
use taskmanager_ui_contract::IconId;

/// Toolkit-owned mirror of the shared icon wire format.
///
/// Keeping this mapping in the GPUI adapter means the core/application
/// layers do not depend on GPUI image types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationImageFormat {
    Svg,
    Png,
    Jpeg,
    Webp,
    Bmp,
}

/// Build a GPUI image element from already-resolved bytes.
///
/// The provider has already bounded and validated the payload. GPUI owns the
/// decode/cache lifecycle after this point; this function performs no file or
/// network I/O.
#[must_use]
pub fn application_image(format: ApplicationImageFormat, bytes: &[u8]) -> Img {
    let format = match format {
        ApplicationImageFormat::Svg => ImageFormat::Svg,
        ApplicationImageFormat::Png => ImageFormat::Png,
        ApplicationImageFormat::Jpeg => ImageFormat::Jpeg,
        ApplicationImageFormat::Webp => ImageFormat::Webp,
        ApplicationImageFormat::Bmp => ImageFormat::Bmp,
    };
    img(Arc::new(Image::from_bytes(format, bytes.to_vec())))
}

/// A GPUI icon element for a semantic [`IconId`].
///
/// Renders the embedded SVG asset through gpui's native `svg` element (no
/// `gpui_component::Icon`). The glyph color is resolved at layout time from the
/// composed ancestor text style, so an icon placed inside a `text_color(..)`
/// container inherits that color; callers may also chain `.text_color(..)`
/// directly. All GPUI style methods (`.size(..)`, `.w(..)`, …) are available on
/// this builder.
#[derive(IntoElement)]
pub struct Icon {
    id: IconId,
    style: StyleRefinement,
}

impl Icon {
    fn new(id: IconId) -> Self {
        Self {
            id,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut element = svg().path(path(self.id));
        element.style().refine(&self.style);
        // gpui's Svg element paints only when a text color is resolved (it
        // reads its own style.text.color, never a parent's). Inherit the
        // composed ancestor text style — the same fallback the gc Icon
        // rendering used (window.text_style().color at layout time).
        let has_own_color = element
            .style()
            .text
            .as_ref()
            .is_some_and(|text| text.color.is_some());
        if !has_own_color {
            element = element.text_color(window.text_style().color);
        }
        element
    }
}

/// Build a tintable GPUI icon. Color inherits from the surrounding text style.
#[must_use]
pub fn icon(icon: IconId) -> Icon {
    Icon::new(icon)
}
