//! The single quarantined module where the neutral theme meets gpui (ADR-026).
//!
//! Compiled only under `feature = "gpui"` (off by default). Rust's orphan rule
//! forbids `impl From<Color> for gpui::Rgba` in any third crate — both types
//! are foreign there — so the conversions live here, in the crate that owns
//! [`Color`] (`palette`-crate precedent: per-toolkit conversions behind
//! optional features). The neutral modules above never name a toolkit type.
//!
//! A future iced frontend gets the same shape: an optional `iced` feature and
//! one `src/iced.rs` module with `From<Color> for iced::Color` and friends.

#![cfg(feature = "gpui")]

use std::time::Duration;

use gpui::{AbsoluteLength, Background, DefiniteLength, Fill, FontWeight, Hsla, Pixels, Rgba};
use gpui::{Animation, WindowBackgroundAppearance, ease_in_out, px, rems};

use crate::color::{Color, FontSize, Length, Ratio, Weight};
use crate::fonts::FontAvailability;
use crate::platform::{WeightCompensationAxis, effective_weight};
use crate::theme::{EdgeTiling, Material, Theme, WindowChromeState};
use crate::tokens::{DURATION_FAST, DURATION_MEDIUM, MotionPolicy};

impl From<Color> for Rgba {
    fn from(color: Color) -> Rgba {
        Rgba {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

impl From<Color> for Hsla {
    fn from(color: Color) -> Hsla {
        Hsla::from(Rgba::from(color))
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Fill {
        Fill::Color(Hsla::from(color).into())
    }
}

impl From<Color> for Background {
    fn from(color: Color) -> Background {
        Background::from(Hsla::from(color))
    }
}

impl From<Length> for Pixels {
    fn from(length: Length) -> Pixels {
        px(length.0)
    }
}

impl From<Length> for AbsoluteLength {
    fn from(length: Length) -> AbsoluteLength {
        AbsoluteLength::Pixels(Pixels::from(length))
    }
}

impl From<FontSize> for AbsoluteLength {
    fn from(size: FontSize) -> AbsoluteLength {
        // 14px is the Small profile and authored token baseline. The window
        // rem becomes 14/16/18px for Small/Standard/Large, respectively.
        AbsoluteLength::Rems(rems(size.0 / 14.0))
    }
}

impl From<Length> for DefiniteLength {
    fn from(length: Length) -> DefiniteLength {
        DefiniteLength::Absolute(AbsoluteLength::from(length))
    }
}

impl From<Length> for gpui::Length {
    fn from(length: Length) -> gpui::Length {
        gpui::Length::Definite(DefiniteLength::from(length))
    }
}

impl From<Ratio> for DefiniteLength {
    fn from(ratio: Ratio) -> DefiniteLength {
        DefiniteLength::Fraction(ratio.0)
    }
}

impl From<Weight> for FontWeight {
    fn from(weight: Weight) -> FontWeight {
        // Platform compensation is decided in `platform` and shared with the
        // iced binding (CORE-07); this impl only projects the decision onto
        // gpui's weight type.
        FontWeight(effective_weight(weight, WeightCompensationAxis::target()).0)
    }
}

/// The window-background appearance the OS surface should present for this
/// theme: `Blurred` for Mica/Vibrancy skins (Windows/macOS translucency),
/// `Transparent` on a Linux CSD surface (the chrome paints the corner
/// rounding itself), and `Opaque` everywhere else. Single source of truth
/// for both the startup `WindowOptions` and the runtime re-application on
/// skin/font switches.
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
pub fn detect_font_availability(cx: &gpui::App) -> FontAvailability {
    let names = cx.text_system().all_font_names();
    FontAvailability::from_installed_families(names.iter().map(|name| name.as_ref()))
}

/// Build a safe GPUI animation from a neutral motion policy and duration
/// token. `None` means the caller should render the final state directly;
/// this keeps reduced/no-motion behavior out of GPUI-specific call sites and
/// prevents zero-duration progress calculations.
pub fn motion_animation(policy: MotionPolicy, duration: Duration) -> Option<Animation> {
    policy
        .animation_duration(duration)
        .map(|duration| Animation::new(duration).with_easing(ease_in_out))
}

/// Policy-aware tooltip/micro-overlay fade animation.
pub fn fade_in_for(policy: MotionPolicy) -> Option<Animation> {
    motion_animation(policy, DURATION_FAST)
}

/// Policy-aware panel/dialog appear animation.
pub fn appear_for(policy: MotionPolicy) -> Option<Animation> {
    motion_animation(policy, DURATION_MEDIUM)
}

/// Quick fade-in for tooltips and micro-overlays (80ms, ease-in-out).
pub fn fade_in() -> Animation {
    Animation::new(DURATION_FAST).with_easing(ease_in_out)
}

/// Standard appear animation for panels/dialogs (180ms, ease-in-out).
pub fn appear() -> Animation {
    Animation::new(DURATION_MEDIUM).with_easing(ease_in_out)
}

#[cfg(test)]
#[path = "../tests/headless/theme_gpui.rs"]
mod tests;
