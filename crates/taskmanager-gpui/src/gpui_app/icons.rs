//! GPUI adapter for frontend-neutral semantic icons.
//!
//! ADR-017 Phase 2: the implementation moved to the `taskmanager-icons` crate
//! (gpui-native SVG, no `gpui_component::Icon`); this module is a thin shim so
//! existing `icons::icon(..)` / `icons::path(..)` call sites keep working.

use crate::core::{ApplicationIconAsset, ApplicationIconFormat};
use gpui::Img;

pub use taskmanager_icons::{icon, path};

/// Adapt a provider-resolved, toolkit-neutral icon asset to a GPUI image.
///
/// The root composition edge is the only place that knows both core's wire
/// format and the GPUI-owned adapter. The bytes were already resolved by the
/// native provider, so rendering performs no Linux filesystem access.
#[must_use]
pub fn application_image(asset: &ApplicationIconAsset) -> Img {
    let format = match asset.format {
        ApplicationIconFormat::Svg => taskmanager_icons::ApplicationImageFormat::Svg,
        ApplicationIconFormat::Png => taskmanager_icons::ApplicationImageFormat::Png,
        ApplicationIconFormat::Jpeg => taskmanager_icons::ApplicationImageFormat::Jpeg,
        ApplicationIconFormat::Webp => taskmanager_icons::ApplicationImageFormat::Webp,
        ApplicationIconFormat::Bmp => taskmanager_icons::ApplicationImageFormat::Bmp,
    };
    taskmanager_icons::application_image(format, &asset.bytes)
}
