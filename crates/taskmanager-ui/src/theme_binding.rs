//! The single quarantined module where the neutral theme meets gpui
//! (ADR-026 conversions, relocated by ADR-051).
//!
//! This is the GPUI-owned component layer: it is the one crate in the gpui
//! product's dependency closure that may name both `taskmanager_theme` token
//! types and gpui value types in the same signature. Rust's orphan rule
//! forbids `impl From<Color> for gpui::Rgba` anywhere else — both types are
//! foreign in `taskmanager-gpui` and in every other crate — so the token →
//! gpui conversions live here as free functions, one per target type, and
//! the frontend consumes THIS module as its only conversion source. The
//! neutral `taskmanager-theme` modules never name a toolkit type.
//!
//! Platform compensation is decided in `taskmanager_theme::platform` and
//! shared with the iced binding; this module only projects those decisions
//! onto gpui's value types.

use std::time::Duration;

use gpui::{
    AbsoluteLength, Animation, Background, BoxShadow, DefiniteLength, Fill, FontWeight, Hsla,
    Pixels, Point, Rgba, WindowBackgroundAppearance, ease_in_out, linear_color_stop,
    linear_gradient, px, rems,
};

use taskmanager_theme::color::{Color, FontSize, Length, Ratio, Weight};
use taskmanager_theme::fonts::FontAvailability;
use taskmanager_theme::optical_glass::OpticalGlassCapability;
use taskmanager_theme::platform::{WeightCompensationAxis, effective_weight};
use taskmanager_theme::theme::{
    EdgeTiling, GlassTier, Material, STANDARD_ROOT_REM_SIZE, Theme, WindowChromeState,
};
use taskmanager_theme::tokens::{DURATION_FAST, DURATION_MEDIUM, MotionPolicy};

// ── token → gpui value conversions (one function per target type) ──────────

/// The optical-glass capability of the locked GPUI renderer.
///
/// `gpui-pre 0.3.10` exposes no sampleable lower composite or backdrop
/// primitive, so the production renderer reports no optical capability. This
/// remains [`OpticalGlassCapability::UNSUPPORTED`] until an upstream release
/// passes the documented P0 gate without a vendor/fork/patch.
#[must_use]
pub fn optical_glass_capability() -> OpticalGlassCapability {
    OpticalGlassCapability::UNSUPPORTED
}

/// Map a neutral color onto gpui's straight-alpha RGBA.
#[must_use]
pub fn rgba(color: Color) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

/// Map a neutral color onto gpui's working HSL space.
#[must_use]
pub fn hsla(color: Color) -> Hsla {
    Hsla::from(rgba(color))
}

/// Map a neutral color onto gpui's fill vocabulary.
#[must_use]
pub fn fill(color: Color) -> Fill {
    Fill::Color(hsla(color).into())
}

/// Project the shared glass sheen into GPUI's native rounded-surface fill.
#[must_use]
pub fn glass_fill(theme: &Theme, tier: GlassTier) -> Fill {
    let [top, bottom] = theme.glass_gradient(tier);
    Fill::from(linear_gradient(
        180.0,
        linear_color_stop(hsla(top), 0.0),
        linear_color_stop(hsla(bottom), 1.0),
    ))
}

/// Map a neutral color onto gpui's background vocabulary.
#[must_use]
pub fn background(color: Color) -> Background {
    Background::from(hsla(color))
}

/// Map a neutral absolute length onto gpui pixels.
#[must_use]
pub fn pixels(length: Length) -> Pixels {
    px(length.0)
}

/// Map a neutral absolute length onto gpui absolute lengths.
#[must_use]
pub fn absolute(length: Length) -> AbsoluteLength {
    AbsoluteLength::Pixels(pixels(length))
}

/// Express a skin radius on the live root-relative scale.
#[must_use]
pub fn rem_length(length: Length) -> gpui::Rems {
    rems(length.0 / STANDARD_ROOT_REM_SIZE)
}

/// Express a raw pixel value on the live root-relative scale. Callers that
/// hold a neutral [`Length`] should prefer [`rem_length`]; this exists for the
/// authored literal dimensions (dialog bounds, icon sizes, entrance offsets)
/// the product states directly in the standard root.
#[must_use]
pub fn rem_value(value: f32) -> gpui::Rems {
    rems(value / STANDARD_ROOT_REM_SIZE)
}

/// Convert one root-relative value at the renderer boundary. GPUI's shadow
/// primitive only accepts pixels; the source token and live theme remain REM.
fn rem_pixels(theme: &Theme, value: f32) -> Pixels {
    AbsoluteLength::Rems(rems(value)).to_pixels(Pixels::from(theme.root_rem_size))
}

/// Resolve the theme-owned two-layer glass shadow onto GPUI's pixel-only
/// shadow type. All dimensions originate in [`GlassTier`] REM values.
#[must_use]
pub fn glass_shadow(theme: &Theme, tier: GlassTier) -> Vec<BoxShadow> {
    let layers = tier.shadow(theme);
    vec![
        BoxShadow {
            color: hsla(layers.ambient.color),
            offset: Point::new(
                Pixels::default(),
                rem_pixels(theme, layers.ambient.offset_y),
            ),
            blur_radius: rem_pixels(theme, layers.ambient.blur_radius),
            spread_radius: Pixels::default(),
            inset: false,
        },
        BoxShadow {
            color: hsla(layers.contact.color),
            offset: Point::new(
                Pixels::default(),
                rem_pixels(theme, layers.contact.offset_y),
            ),
            blur_radius: rem_pixels(theme, layers.contact.blur_radius),
            spread_radius: Pixels::default(),
            inset: false,
        },
    ]
}

/// Map a neutral authored font size onto gpui rem lengths. 14px is the Small
/// profile and authored token baseline. The window rem becomes 14/16/18px for
/// Small/Standard/Large, respectively.
#[must_use]
pub fn font_size(size: FontSize) -> AbsoluteLength {
    AbsoluteLength::Rems(rems(size.0 / 14.0))
}

/// Map a neutral absolute length onto gpui definite lengths.
#[must_use]
pub fn definite_length(length: Length) -> DefiniteLength {
    DefiniteLength::Absolute(absolute(length))
}

/// Map a neutral length onto gpui's general length vocabulary.
#[must_use]
pub fn length(length: Length) -> gpui::Length {
    gpui::Length::Definite(definite_length(length))
}

/// Map a neutral ratio onto gpui fraction lengths.
#[must_use]
pub fn fraction(ratio: Ratio) -> DefiniteLength {
    DefiniteLength::Fraction(ratio.0)
}

/// Map a neutral weight onto gpui's font weight. Platform compensation is
/// decided in `taskmanager_theme::platform` and shared with the iced binding
/// (CORE-07); this function only projects the decision onto gpui's weight
/// type.
#[must_use]
pub fn font_weight(weight: Weight) -> FontWeight {
    FontWeight(effective_weight(weight, WeightCompensationAxis::target()).0)
}

// ── window / appearance / font / animation helpers ─────────────────────────

/// The window-background appearance the OS surface should present for this
/// theme: `Blurred` for Mica/Vibrancy skins (Windows/macOS translucency),
/// `Transparent` on a Linux CSD surface (the chrome paints the corner
/// rounding itself), and `Opaque` everywhere else. Single source of truth
/// for both the startup `WindowOptions` and the runtime re-application on
/// skin/font switches.
#[must_use]
pub fn background_appearance(theme: &Theme) -> WindowBackgroundAppearance {
    if theme.material != Material::Opaque {
        WindowBackgroundAppearance::Blurred
    } else if theme.window_transparent {
        WindowBackgroundAppearance::Transparent
    } else {
        WindowBackgroundAppearance::Opaque
    }
}

/// Snapshot the current platform window's chrome facts into the neutral
/// [`WindowChromeState`]. Read-only queries, cheap enough for every frame
/// (the compositor state changes only on maximize/fullscreen/tile events,
/// which already trigger a re-render).
#[must_use]
pub fn window_chrome_state(window: &gpui::Window) -> WindowChromeState {
    let tiling = match window.window_decorations() {
        gpui::Decorations::Client { tiling } => EdgeTiling {
            top: tiling.top,
            left: tiling.left,
            right: tiling.right,
            bottom: tiling.bottom,
        },
        gpui::Decorations::Server => EdgeTiling::default(),
    };
    WindowChromeState {
        maximized: window.is_maximized(),
        fullscreen: window.is_fullscreen(),
        tiling,
    }
}

/// Query gpui's font database for each skin's system families and build the
/// neutral per-skin availability snapshot. `cx` must have a text system
/// (true in the app and in GPUI tests). The returned catalog contains only
/// names actually reported by that text system; the neutral resolver chooses
/// a bounded typed fallback when a preferred family is absent.
#[must_use]
pub fn detect_font_availability(cx: &gpui::App) -> FontAvailability {
    let names = cx.text_system().all_font_names();
    FontAvailability::from_installed_families(names.iter().map(|name| name.as_ref()))
}

/// Build a safe GPUI animation from a neutral motion policy and duration
/// token. `None` means the caller should render the final state directly;
/// this keeps reduced/no-motion behavior out of GPUI-specific call sites and
/// prevents zero-duration progress calculations.
#[must_use]
pub fn motion_animation(policy: MotionPolicy, duration: Duration) -> Option<Animation> {
    policy
        .animation_duration(duration)
        .map(|duration| Animation::new(duration).with_easing(ease_in_out))
}

/// Policy-aware tooltip/micro-overlay fade animation.
#[must_use]
pub fn fade_in_for(policy: MotionPolicy) -> Option<Animation> {
    motion_animation(policy, DURATION_FAST)
}

/// Policy-aware panel/dialog appear animation.
#[must_use]
pub fn appear_for(policy: MotionPolicy) -> Option<Animation> {
    motion_animation(policy, DURATION_MEDIUM)
}

/// Quick fade-in for tooltips and micro-overlays (80ms, ease-in-out).
#[must_use]
pub fn fade_in() -> Animation {
    Animation::new(DURATION_FAST).with_easing(ease_in_out)
}

/// Standard appear animation for panels/dialogs (180ms, ease-in-out).
#[must_use]
pub fn appear() -> Animation {
    Animation::new(DURATION_MEDIUM).with_easing(ease_in_out)
}

#[cfg(test)]
#[path = "../tests/headless/theme_binding.rs"]
mod tests;
