//! GPUI adapter for frontend-neutral semantic icons.
//!
//! ADR-017 Phase 2: the `icon`/`path` element builders live in the
//! `taskmanager-icons` crate (gpui-native SVG, no `gpui_component::Icon`) and
//! are imported from their owner path directly; this module keeps only the
//! provider-asset image adapter below.

use gpui::Img;
use taskmanager_core::core::{ApplicationIconAsset, ApplicationIconFormat};
use taskmanager_ui::icons_binding::ApplicationImageFormat;

/// Adapt a provider-resolved, toolkit-neutral icon asset to a GPUI image.
///
/// The root composition edge is the only place that knows both core's wire
/// format and the GPUI-owned adapter. The bytes were already resolved by the
/// native provider, so rendering performs no Linux filesystem access.
#[must_use]
pub fn application_image(asset: &ApplicationIconAsset) -> Img {
    use taskmanager_ui::icons_binding::application_image;

    let format = match asset.format {
        ApplicationIconFormat::Svg => ApplicationImageFormat::Svg,
        ApplicationIconFormat::Png => ApplicationImageFormat::Png,
        ApplicationIconFormat::Jpeg => ApplicationImageFormat::Jpeg,
        ApplicationIconFormat::Webp => ApplicationImageFormat::Webp,
        ApplicationIconFormat::Bmp => ApplicationImageFormat::Bmp,
    };
    application_image(format, &asset.bytes)
}
