//! Toolkit-neutral contract for optical glass.
//!
//! A [`GlassLayer`](crate::GlassLayer) is a painted frosted fallback. Optical
//! glass is a stronger renderer capability: it samples the content behind an
//! element, blurs it, refracts it through a rounded SDF, disperses the color
//! channels, and adds a specular rim and grain. This module owns the shared
//! design intent and the fallback decision. It deliberately owns no renderer
//! state and no physical-pixel constants.
//!
//! Renderers report [`OpticalGlassCapability`] only after their production
//! pipeline can perform every required operation. A missing capability never
//! upgrades a painted surface into optical glass; it returns an explicit
//! [`OpticalGlassFallback`] with the reason.

use crate::color::Color;
use crate::theme::{GlassTier, RadiusScale, Theme};

/// Shader quality tier for an optical-glass surface.
///
/// The toolkit binding decides the concrete sample count. The neutral contract
/// only records the intended quality so one renderer cannot silently choose a
/// different tier.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum OpticalGlassQuality {
    /// Minimum viable optical pipeline for constrained devices.
    Low,
    /// Default desktop quality.
    #[default]
    Balanced,
    /// Strongest optical quality, reserved for the modal tier.
    High,
}

/// One optical-glass material in root-relative design units.
///
/// Every length is authored in REM. [`Theme::optical_glass_spec`] derives the
/// values from the active skin and tier; renderers resolve them through
/// [`OpticalGlassSpec::resolve`] using the current root size and device scale.
/// Colors carry their own alpha, except `grain_ink`, whose alpha is ignored in
/// favor of `grain_strength`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpticalGlassSpec {
    /// Backdrop blur radius in REM.
    pub blur_radius_rem: f32,
    /// Maximum edge displacement caused by refraction, in REM.
    pub refraction_strength_rem: f32,
    /// Width of the rounded-SDF edge band that refracts, in REM.
    pub refraction_width_rem: f32,
    /// Fractional R/B displacement difference, in `0.0..=1.0`.
    pub chromatic_dispersion: f32,
    /// Width of the specular rim, in REM.
    pub specular_width_rem: f32,
    /// Non-negative specular intensity multiplier.
    pub specular_intensity: f32,
    /// Final glass opacity, in `0.0..=1.0`.
    pub frost_opacity: f32,
    /// Tint mixed into the refracted backdrop. Its alpha is the mix amount.
    pub tint: Color,
    /// Grain opacity multiplier, in `0.0..=1.0`.
    pub grain_strength: f32,
    /// Ink used by the grain texture. Alpha is supplied by `grain_strength`.
    pub grain_ink: Color,
    /// Corner radius used by the rounded SDF, in REM.
    pub corner_radius_rem: f32,
    /// Intended shader quality tier.
    pub quality: OpticalGlassQuality,
}

impl OpticalGlassSpec {
    /// Resolve all root-relative geometry into physical pixels for a renderer.
    ///
    /// `root_rem_size` is the renderer-independent root size and
    /// `device_scale_factor` converts logical units to physical pixels. Invalid
    /// specs or contexts fail closed with `None`; callers must use a painted
    /// fallback rather than forwarding malformed uniforms to the GPU.
    #[must_use]
    pub fn resolve(self, context: OpticalGlassContext) -> Option<OpticalGlassGeometry> {
        if !context.is_valid() || !self.is_valid() {
            return None;
        }

        let physical_scale = context.root_rem_size * context.device_scale_factor;
        let to_px = |rem: f32| {
            let px = rem * physical_scale;
            px.is_finite().then_some(px)
        };

        Some(OpticalGlassGeometry {
            blur_radius_px: to_px(self.blur_radius_rem)?,
            refraction_strength_px: to_px(self.refraction_strength_rem)?,
            refraction_width_px: to_px(self.refraction_width_rem)?,
            specular_width_px: to_px(self.specular_width_rem)?,
            corner_radius_px: to_px(self.corner_radius_rem)?,
        })
    }

    fn is_valid(self) -> bool {
        let non_negative_finite = |value: f32| value.is_finite() && value >= 0.0;
        let unit_interval = |value: f32| value.is_finite() && (0.0..=1.0).contains(&value);

        non_negative_finite(self.blur_radius_rem)
            && self.blur_radius_rem > 0.0
            && non_negative_finite(self.refraction_strength_rem)
            && non_negative_finite(self.refraction_width_rem)
            && self.refraction_width_rem > 0.0
            && unit_interval(self.chromatic_dispersion)
            && non_negative_finite(self.specular_width_rem)
            && self.specular_width_rem > 0.0
            && non_negative_finite(self.specular_intensity)
            && unit_interval(self.frost_opacity)
            && valid_color(self.tint)
            && unit_interval(self.grain_strength)
            && valid_color(self.grain_ink)
            && non_negative_finite(self.corner_radius_rem)
    }
}

/// Renderer inputs needed to resolve an optical-glass material.
///
/// `nesting_depth` is zero-based: a top-level glass is depth `0`, a glass inside
/// that glass is depth `1`. High contrast is not stored here; the owning
/// [`Theme`] supplies it to [`Theme::optical_glass_decision`] so a caller cannot
/// accidentally bypass the accessibility fallback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpticalGlassContext {
    /// Root-relative unit size in logical pixels.
    pub root_rem_size: f32,
    /// Physical pixels per logical pixel.
    pub device_scale_factor: f32,
    /// Zero-based glass nesting depth.
    pub nesting_depth: u8,
    /// Whether this render has no GPU/backdrop-sampling surface.
    pub headless: bool,
}

impl OpticalGlassContext {
    /// A top-level, non-headless context at the supplied scale.
    #[must_use]
    pub const fn new(root_rem_size: f32, device_scale_factor: f32) -> Self {
        Self {
            root_rem_size,
            device_scale_factor,
            nesting_depth: 0,
            headless: false,
        }
    }

    /// Set the zero-based nesting depth.
    #[must_use]
    pub const fn with_nesting_depth(mut self, nesting_depth: u8) -> Self {
        self.nesting_depth = nesting_depth;
        self
    }

    /// Mark this context as a headless render.
    #[must_use]
    pub const fn headless(mut self) -> Self {
        self.headless = true;
        self
    }

    fn is_valid(self) -> bool {
        self.root_rem_size.is_finite()
            && self.root_rem_size > 0.0
            && self.device_scale_factor.is_finite()
            && self.device_scale_factor > 0.0
    }
}

/// Physical-pixel geometry consumed by a toolkit binding.
///
/// These are the only optical-glass lengths that cross into a GPU uniform. The
/// theme keeps REM authoring as the source of truth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpticalGlassGeometry {
    /// Backdrop blur radius in physical pixels.
    pub blur_radius_px: f32,
    /// Maximum refraction displacement in physical pixels.
    pub refraction_strength_px: f32,
    /// Rounded-SDF refraction band in physical pixels.
    pub refraction_width_px: f32,
    /// Specular rim width in physical pixels.
    pub specular_width_px: f32,
    /// Rounded-SDF corner radius in physical pixels.
    pub corner_radius_px: f32,
}

/// One operation required by the complete optical-glass pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpticalGlassFeature {
    /// Read the previously composited content behind the element.
    BackdropSampling,
    /// Blur the sampled backdrop.
    Blur,
    /// Displace backdrop samples through a rounded-SDF normal.
    Refraction,
    /// Offset R/G/B samples independently.
    Dispersion,
    /// Add a normal-dependent specular rim.
    Specular,
    /// Composite the shared grain texture.
    Grain,
}

impl OpticalGlassFeature {
    /// All operations required before a renderer may report optical glass.
    pub const REQUIRED: [Self; 6] = [
        Self::BackdropSampling,
        Self::Blur,
        Self::Refraction,
        Self::Dispersion,
        Self::Specular,
        Self::Grain,
    ];
}

/// Capability report supplied by a concrete renderer.
///
/// There is intentionally no `Default` implementation: a renderer must state
/// every required operation and both limits explicitly. `UNSUPPORTED` is the
/// only fail-closed shortcut. GPUI and Iced report this value until an upstream
/// release exposes a sampleable lower composite and the required painter order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpticalGlassCapability {
    /// The renderer can sample the content behind the element.
    pub backdrop_sampling: bool,
    /// The renderer can blur that sample.
    pub blur: bool,
    /// The renderer can apply rounded-SDF refraction.
    pub refraction: bool,
    /// The renderer can apply chromatic dispersion.
    pub dispersion: bool,
    /// The renderer can add a normal-dependent specular rim.
    pub specular: bool,
    /// The renderer can composite the shared grain.
    pub grain: bool,
    /// Maximum supported simultaneously nested glass layers. Zero means none.
    pub max_nested_layers: u8,
    /// Maximum physical blur radius in pixels. Zero means no optical blur.
    pub max_blur_px: u32,
}

/// Explicit renderer operations supplied when reporting optical capability.
///
/// Keeping the six booleans in a named value prevents accidental positional
/// swaps between sampling and downstream shader stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpticalGlassOperations {
    /// The renderer can sample the content behind the element.
    pub backdrop_sampling: bool,
    /// The renderer can blur that sample.
    pub blur: bool,
    /// The renderer can apply rounded-SDF refraction.
    pub refraction: bool,
    /// The renderer can apply chromatic dispersion.
    pub dispersion: bool,
    /// The renderer can add a normal-dependent specular rim.
    pub specular: bool,
    /// The renderer can composite the shared grain.
    pub grain: bool,
}

impl OpticalGlassOperations {
    /// No optical operation is available.
    pub const NONE: Self = Self {
        backdrop_sampling: false,
        blur: false,
        refraction: false,
        dispersion: false,
        specular: false,
        grain: false,
    };

    /// Every operation required by the complete optical pipeline is available.
    pub const COMPLETE: Self = Self {
        backdrop_sampling: true,
        blur: true,
        refraction: true,
        dispersion: true,
        specular: true,
        grain: true,
    };
}

impl OpticalGlassCapability {
    /// A renderer with no optical-glass pipeline.
    pub const UNSUPPORTED: Self = Self {
        backdrop_sampling: false,
        blur: false,
        refraction: false,
        dispersion: false,
        specular: false,
        grain: false,
        max_nested_layers: 0,
        max_blur_px: 0,
    };

    /// Build a capability report from explicit renderer facts.
    #[must_use]
    pub const fn reported_by_renderer(
        operations: OpticalGlassOperations,
        max_nested_layers: u8,
        max_blur_px: u32,
    ) -> Self {
        Self {
            backdrop_sampling: operations.backdrop_sampling,
            blur: operations.blur,
            refraction: operations.refraction,
            dispersion: operations.dispersion,
            specular: operations.specular,
            grain: operations.grain,
            max_nested_layers,
            max_blur_px,
        }
    }

    /// Whether this report supports one operation.
    #[must_use]
    pub const fn supports(self, feature: OpticalGlassFeature) -> bool {
        match feature {
            OpticalGlassFeature::BackdropSampling => self.backdrop_sampling,
            OpticalGlassFeature::Blur => self.blur,
            OpticalGlassFeature::Refraction => self.refraction,
            OpticalGlassFeature::Dispersion => self.dispersion,
            OpticalGlassFeature::Specular => self.specular,
            OpticalGlassFeature::Grain => self.grain,
        }
    }

    /// First missing operation in the fixed pipeline order.
    #[must_use]
    pub const fn first_missing_feature(self) -> Option<OpticalGlassFeature> {
        if !self.backdrop_sampling {
            Some(OpticalGlassFeature::BackdropSampling)
        } else if !self.blur {
            Some(OpticalGlassFeature::Blur)
        } else if !self.refraction {
            Some(OpticalGlassFeature::Refraction)
        } else if !self.dispersion {
            Some(OpticalGlassFeature::Dispersion)
        } else if !self.specular {
            Some(OpticalGlassFeature::Specular)
        } else if !self.grain {
            Some(OpticalGlassFeature::Grain)
        } else {
            None
        }
    }
}

/// Honest material used when optical glass is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpticalGlassFallback {
    /// Opaque themed surface, required by high contrast.
    Opaque,
    /// Backdrop blur exists, but the complete optical pipeline does not.
    Frosted,
    /// Painted theme fill, edge, shadow, and sheen with no backdrop sampling.
    Painted,
}

/// How a degraded glass surface treats the content behind it.
///
/// Optical glass samples the backdrop and refracts it. A fallback may keep
/// the sampling — blurred, without the refraction or the dispersion — or drop
/// it entirely and paint its own fill.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlassFallbackBackdrop {
    /// The surface paints its own fill; nothing behind it is sampled.
    None,
    /// The backdrop is sampled and blurred, without refraction.
    Blurred,
}

/// The paint contract a degraded glass surface wears.
///
/// Optical glass is a rendering technique, not a material: when the pipeline
/// is unavailable the surface still has to LOOK like the tier it belongs to.
/// The theme owns that answer here, so a renderer that cannot run the
/// pipeline never has to decide for itself what "an opaque Card" means, and
/// two frontends cannot degrade the same tier differently.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassFallbackSurface {
    /// The panel fill to paint.
    pub fill: Color,
    /// The hairline edge.
    pub edge: Color,
    /// The two-layer elevation shadow ink.
    pub shadow: Color,
    /// Whether the tier's specular sheen survives this fallback.
    pub sheen: bool,
    /// How the surface treats the content behind it.
    pub backdrop: GlassFallbackBackdrop,
}

/// Why the optical pipeline could not be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpticalGlassUnavailableReason {
    /// High contrast requires an opaque material.
    HighContrast,
    /// The render has no GPU/backdrop-sampling surface.
    Headless,
    /// A scrim is a depth wash, not a panel.
    ScrimNotOptical,
    /// The spec or renderer context contains invalid dimensions.
    InvalidSpec,
    /// The renderer lacks one required operation.
    MissingFeature(OpticalGlassFeature),
    /// The requested physical blur exceeds the renderer's limit.
    BlurLimitExceeded,
    /// The requested nesting depth exceeds the renderer's limit.
    NestedLayerLimitExceeded,
}

/// Result of resolving a theme tier against a renderer capability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpticalGlassDecision {
    /// The complete pipeline is available and may render optical glass.
    Optical {
        /// Theme-authored material intent.
        spec: OpticalGlassSpec,
        /// Physical-pixel values for the GPU binding.
        geometry: OpticalGlassGeometry,
    },
    /// Render the named painted/frosted/opaque fallback instead.
    Fallback {
        /// Material the renderer must paint.
        material: OpticalGlassFallback,
        /// Why optical glass was not used.
        reason: OpticalGlassUnavailableReason,
    },
}

impl OpticalGlassDecision {
    /// Whether this decision permits the Optical label.
    #[must_use]
    pub const fn is_optical(self) -> bool {
        matches!(self, Self::Optical { .. })
    }
}

impl Theme {
    /// The paint contract one glass tier wears under a degraded pipeline.
    ///
    /// The TIER still decides the material — fill, edge and shadow — because
    /// a fallback Card is still a Card. The fallback decides only how much of
    /// the optical treatment survives: a frosted fallback keeps the tier's
    /// painted material and blurs the backdrop, a painted fallback keeps the
    /// material and samples nothing, and an opaque fallback flattens the fill
    /// and drops the sheen while keeping the tier's edge, so the surface
    /// still reads as its tier.
    #[must_use]
    pub fn glass_fallback_surface(
        self,
        tier: GlassTier,
        fallback: OpticalGlassFallback,
    ) -> GlassFallbackSurface {
        let layer = self.glass_layer(tier);
        let sheen = !self.hc && tier != GlassTier::Scrim;
        match fallback {
            OpticalGlassFallback::Painted => GlassFallbackSurface {
                fill: layer.fill,
                edge: layer.edge,
                shadow: layer.shadow,
                sheen,
                backdrop: GlassFallbackBackdrop::None,
            },
            OpticalGlassFallback::Frosted => GlassFallbackSurface {
                fill: layer.fill,
                edge: layer.edge,
                shadow: layer.shadow,
                sheen,
                backdrop: GlassFallbackBackdrop::Blurred,
            },
            OpticalGlassFallback::Opaque => GlassFallbackSurface {
                fill: layer.fill.with_alpha(1.0),
                edge: layer.edge,
                shadow: layer.shadow,
                sheen: false,
                backdrop: GlassFallbackBackdrop::None,
            },
        }
    }

    /// Resolve one panel tier into optical-glass design intent.
    ///
    /// `Scrim` is not a glass panel and returns `None`. All lengths remain in
    /// REM; the returned spec must pass through [`OpticalGlassSpec::resolve`]
    /// before a renderer uploads physical-pixel uniforms.
    #[must_use]
    pub fn optical_glass_spec(self, tier: GlassTier) -> Option<OpticalGlassSpec> {
        if tier == GlassTier::Scrim || !self.root_rem_size.is_finite() || self.root_rem_size <= 0.0
        {
            return None;
        }

        let (refraction_strength_rem, refraction_width_rem, chromatic_dispersion) = match tier {
            GlassTier::Sidebar => (1.0, 1.5, 0.06),
            GlassTier::Card => (1.5, 2.25, 0.10),
            GlassTier::Modal => (2.0, 2.75, 0.12),
            GlassTier::Scrim => return None,
        };
        // The tier's material opacity has ONE authority: the glass layer the
        // painted fallback already uses. A GPU panel is the same material, so
        // it must not re-derive a lighter one — a tint that thin turns the
        // panel into a window onto whatever flat canvas sits behind it, and
        // the card loses its elevation the moment real glass is enabled.
        //
        // `frost` is therefore full coverage and `tint` is the tier's own
        // fill alpha: the panel keeps exactly the elevation the other
        // frontends paint, and the remaining fraction of the panel is the
        // sampled backdrop — the blur, refraction, dispersion and specular
        // that only a real lower composite can show.
        let glass = self.glass_layer(tier);
        let (specular_width_rem, specular_intensity, quality) = match tier {
            GlassTier::Sidebar => (
                GlassTier::Sidebar.edge_width_rem() * 1.5,
                0.95,
                OpticalGlassQuality::Balanced,
            ),
            GlassTier::Card => (
                GlassTier::Card.edge_width_rem() * 2.0,
                1.10,
                OpticalGlassQuality::Balanced,
            ),
            GlassTier::Modal => (
                GlassTier::Modal.edge_width_rem() * 2.5,
                1.25,
                OpticalGlassQuality::High,
            ),
            GlassTier::Scrim => return None,
        };

        Some(OpticalGlassSpec {
            blur_radius_rem: tier.blur_radius_rem(),
            refraction_strength_rem,
            refraction_width_rem,
            chromatic_dispersion,
            specular_width_rem,
            specular_intensity,
            frost_opacity: 1.0,
            tint: glass.fill,
            grain_strength: tier.noise_strength(),
            grain_ink: self.card_shadow(),
            corner_radius_rem: self.radius(RadiusScale::Large) / self.root_rem_size,
            quality,
        })
    }

    /// Decide whether one tier may use optical glass in this render.
    ///
    /// The owning theme supplies high contrast, so callers cannot bypass the
    /// accessibility fallback. Headless renders, missing renderer operations,
    /// over-limit nesting/blur, scrims, and invalid specs all fail closed.
    #[must_use]
    pub fn optical_glass_decision(
        self,
        tier: GlassTier,
        capability: OpticalGlassCapability,
        context: OpticalGlassContext,
    ) -> OpticalGlassDecision {
        if tier == GlassTier::Scrim {
            return fallback(
                OpticalGlassFallback::Painted,
                OpticalGlassUnavailableReason::ScrimNotOptical,
            );
        }
        let Some(spec) = self.optical_glass_spec(tier) else {
            return fallback(
                OpticalGlassFallback::Painted,
                OpticalGlassUnavailableReason::InvalidSpec,
            );
        };
        if self.hc {
            return fallback(
                OpticalGlassFallback::Opaque,
                OpticalGlassUnavailableReason::HighContrast,
            );
        }
        if context.headless {
            return fallback(
                OpticalGlassFallback::Painted,
                OpticalGlassUnavailableReason::Headless,
            );
        }
        let Some(geometry) = spec.resolve(context) else {
            return fallback(
                OpticalGlassFallback::Painted,
                OpticalGlassUnavailableReason::InvalidSpec,
            );
        };
        if let Some(feature) = capability.first_missing_feature() {
            return fallback(
                degraded_material(capability),
                OpticalGlassUnavailableReason::MissingFeature(feature),
            );
        }
        if context.nesting_depth >= capability.max_nested_layers {
            return fallback(
                degraded_material(capability),
                OpticalGlassUnavailableReason::NestedLayerLimitExceeded,
            );
        }
        if geometry.blur_radius_px > capability.max_blur_px as f32 {
            return fallback(
                degraded_material(capability),
                OpticalGlassUnavailableReason::BlurLimitExceeded,
            );
        }

        OpticalGlassDecision::Optical { spec, geometry }
    }
}

fn fallback(
    material: OpticalGlassFallback,
    reason: OpticalGlassUnavailableReason,
) -> OpticalGlassDecision {
    OpticalGlassDecision::Fallback { material, reason }
}

fn degraded_material(capability: OpticalGlassCapability) -> OpticalGlassFallback {
    if capability.backdrop_sampling && capability.blur {
        OpticalGlassFallback::Frosted
    } else {
        OpticalGlassFallback::Painted
    }
}

fn valid_color(color: Color) -> bool {
    [color.r, color.g, color.b, color.a]
        .into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
}

#[cfg(test)]
#[path = "../tests/headless/optical_glass.rs"]
mod tests;
