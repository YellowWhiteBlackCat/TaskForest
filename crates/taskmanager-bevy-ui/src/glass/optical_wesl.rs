//! Typed refractive parameters for the production WESL optical shader.
//!
//! The shared theme contract owns the authored intent (`OpticalGlassSpec`)
//! and the fallback decision; this module owns the physical reading of that
//! intent at the GPU boundary: the index of refraction the shader bends
//! samples through, the dispersive fraction spread across the color
//! channels, and the blur radius the proven capability ceiling admits.
//!
//! The boundary is honest in both directions. IOR and dispersion are
//! continuous optical parameters with no theme gate, so out-of-range values
//! clamp into the valid glass range instead of reaching the GPU malformed.
//! The blur radius is capability-governed — the theme already fails closed
//! with `BlurLimitExceeded` — so an over-ceiling radius is rejected here
//! too, never silently clamped into a smaller claim. A NaN parameter cannot
//! clamp anywhere meaningful and fails closed; an infinite one clamps like
//! any other out-of-range value.
//!
//! This module adds no capability claim: the proven operation set in
//! `capability.rs` is unchanged, and the standard optical GPU gate re-proves
//! the deepened shader on hardware.

use std::ops::RangeInclusive;

use taskmanager_theme::optical_glass::{OpticalGlassGeometry, OpticalGlassSpec};

/// The refractive parameters one glass surface bends backdrop samples with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OpticalGlassParams {
    /// Index of refraction: the ratio the shader Snell-refracts with. Air
    /// (1.0) means no bending at all; 2.42 is diamond.
    pub(crate) ior: f32,
    /// Fractional IOR spread between the red and blue samples.
    pub(crate) dispersion: f32,
    /// Physical blur radius in pixels.
    pub(crate) blur_radius_px: f32,
}

impl OpticalGlassParams {
    /// Crown glass: the projection's own optical constant until the shared
    /// theme contract grows an authored IOR field. This module is the fact's
    /// only authority; it lives here, not in a palette or a shader literal.
    pub(crate) const DEFAULT_IOR: f32 = 1.5;

    /// The valid glass IOR range: air (no bending) ..= diamond.
    pub(crate) const IOR_RANGE: RangeInclusive<f32> = 1.0..=2.42;

    /// The valid dispersive fraction, matching the shared spec contract.
    pub(crate) const DISPERSION_RANGE: RangeInclusive<f32> = 0.0..=1.0;

    /// Raw parameters as a caller might state them, before the boundary
    /// clamps and admits them.
    #[must_use]
    pub(crate) fn from_raw(ior: f32, dispersion: f32, blur_radius_px: f32) -> Self {
        Self {
            ior,
            dispersion,
            blur_radius_px,
        }
    }

    /// Read the theme-owned spec and resolved geometry into raw parameters.
    #[must_use]
    pub(crate) fn from_spec(spec: OpticalGlassSpec, geometry: OpticalGlassGeometry) -> Self {
        Self::from_raw(
            Self::DEFAULT_IOR,
            spec.chromatic_dispersion,
            geometry.blur_radius_px,
        )
    }

    /// Clamp the continuous optical parameters into their valid ranges and
    /// admit the blur radius only inside the proven capability ceiling.
    ///
    /// `None` means the blur radius was never proven at this size: the caller
    /// must fall back instead of rendering a smaller, unproven claim.
    #[must_use]
    pub(crate) fn admitted(self, max_blur_px: u32) -> Option<Self> {
        let ior = self
            .ior
            .clamp(*Self::IOR_RANGE.start(), *Self::IOR_RANGE.end());
        let dispersion = self.dispersion.clamp(
            *Self::DISPERSION_RANGE.start(),
            *Self::DISPERSION_RANGE.end(),
        );
        let blur_admitted = (0.0..=max_blur_px as f32).contains(&self.blur_radius_px);
        if !ior.is_finite() || !dispersion.is_finite() || !blur_admitted {
            return None;
        }
        Some(Self {
            ior,
            dispersion,
            blur_radius_px: self.blur_radius_px,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/headless/optical_wesl.rs"]
mod tests;
