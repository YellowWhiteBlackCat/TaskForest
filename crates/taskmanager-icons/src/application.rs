//! GPUI adapter for provider-resolved application icon bytes.

use std::sync::Arc;

use gpui::{Image, ImageFormat, Img, img};

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
