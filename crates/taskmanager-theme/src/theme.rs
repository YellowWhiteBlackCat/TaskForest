//! Multi-platform skin engine. `Theme` is the active skin's RESOLVED runtime
//! token set; built from [`Skin`] (Gnome/Kde/Windows/Macos) × [`LightDark`] ×
//! [`HighContrast`]. Auto-detected from the host desktop, switchable at
//! runtime (toggle light/dark, choose EyeForest, or change skin → whole app
//! re-skins next frame).
//!
//! Token values are sourced verbatim from upstream design specs:
//!   GNOME  — libadwaita (Adwaita, GNOME 48 "colder" palette) _colors/_palette/_common
//!   KDE    — Breeze (Plasma 6) colors/BreezeDark.colors / BreezeLight.colors
//!   Win    — WinUI 3 (Fluent) SolidBackgroundFillColorBase + accent + card tokens
//!   macOS  — Sonoma system colors (opaque vibrancy fallbacks + system hues)
//! The current product-level skin rules live in `docs/PRODUCT_IDENTITY.md` and
//! `docs/UI_COMPONENT_ARCHITECTURE.md`; this module owns the typed token tables.
//!
//! The per-variant token tables live in [`crate::skins`]; the window/panel
//! color contract in [`crate::palette`]; font resolution in [`crate::fonts`];
//! native-appearance detection in [`crate::detection`].

use crate::color::{Color, mix};
use crate::detection::{NativeAppearance, detect_high_contrast, detect_mode, detect_skin};
use crate::fonts::ResolvedFonts;
use crate::skins::tokens_for;

/// The four native OS skins. (`Platform` kept as an alias for back-compat.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Skin {
    Gnome,
    Kde,
    Windows,
    Macos,
}

impl Skin {
    pub const ALL: [Skin; 4] = [Skin::Gnome, Skin::Kde, Skin::Windows, Skin::Macos];
    pub fn label(self) -> &'static str {
        match self {
            Skin::Gnome => "GNOME",
            Skin::Kde => "KDE",
            Skin::Windows => "Windows",
            Skin::Macos => "macOS",
        }
    }
    /// Index into `Skin::ALL` — maps the enum to `[ResolvedFonts; 4]` slots.
    pub(crate) fn idx(self) -> usize {
        match self {
            Skin::Gnome => 0,
            Skin::Kde => 1,
            Skin::Windows => 2,
            Skin::Macos => 3,
        }
    }
    /// The skin's preferred SYSTEM UI font family (Adwaita Sans / Noto Sans /
    /// Segoe UI Variable / AppleSystemUIFont). The resolved stack falls back
    /// to a bundled face when this family is not installed on the host.
    pub const fn ui_font(self) -> &'static str {
        match self {
            Skin::Gnome => "Adwaita Sans",
            Skin::Kde => "Noto Sans",
            Skin::Windows => "Segoe UI Variable",
            Skin::Macos => ".AppleSystemUIFont",
        }
    }
    /// The skin's preferred SYSTEM monospace family (the digit-aligned face
    /// used for metrics columns). Falls back to the bundled Roboto Mono.
    pub const fn mono_font(self) -> &'static str {
        match self {
            Skin::Gnome => "Adwaita Mono",
            Skin::Kde => "Noto Sans Mono",
            Skin::Windows => "Cascadia Code",
            Skin::Macos => "SF Mono",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightDark {
    Light,
    Dark,
    /// A low-glare, desaturated green palette designed for long sessions.
    /// This remains on the existing color-mode axis so persisted settings,
    /// native skin geometry, and per-window isolation keep one source of truth.
    EyeForest,
}

impl LightDark {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::EyeForest];

    pub fn other(self) -> Self {
        match self {
            LightDark::Light => LightDark::Dark,
            LightDark::Dark => LightDark::Light,
            // There is no second EyeForest variant; returning to Light keeps
            // the toggle deterministic and avoids silently switching to dark.
            LightDark::EyeForest => LightDark::Light,
        }
    }

    pub fn is_eye_forest(self) -> bool {
        matches!(self, LightDark::EyeForest)
    }

    pub fn is_dark(self) -> bool {
        matches!(self, LightDark::Dark)
    }

    pub fn label(self) -> &'static str {
        match self {
            LightDark::Light => "Light",
            LightDark::Dark => "Dark",
            LightDark::EyeForest => "EyeForest",
        }
    }
}

/// Window backdrop material. Only Windows (Mica) and macOS (Vibrancy) use
/// translucency; GNOME/KDE are fully Opaque. v1 paints the opaque fallbacks;
/// the field selects chrome behavior (e.g. whether to render a translucency
/// tint).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Opaque,
    Mica,
    Vibrancy,
}

/// Native window-control placement/style — drives the CSD titlebar chrome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowControls {
    /// macOS: three traffic-light circles, top-LEFT (close/min/zoom).
    TrafficLight,
    /// Windows 11: caption buttons, top-RIGHT (min/max/close), flat ~46×32.
    Caption,
    /// GNOME/libadwaita: single circular Close, top-RIGHT (red on hover).
    AdwaitaClose,
    /// KDE Breeze: min/max/close, top-RIGHT (+ optional appmenu left).
    Breeze,
}

/// High-contrast accessibility axis (GNOME a11y / macOS contrast / Windows
/// HighContrast).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HighContrast {
    Off,
    On,
}

/// The four outer corners of a window. Corner-specific radius policy lets a
/// tiled/maximized window suppress exactly the corners that touch a screen or
/// tiling edge, matching what native CSD apps do on KWin/Niri.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Per-edge tiling facts, mirrored from gpui's `Decorations::Client { tiling }`
/// (filled by the Wayland compositor via xdg_toplevel configure). Kept as our
/// own Copy type so `Theme` stays platform-neutral and unit-testable.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EdgeTiling {
    pub top: bool,
    pub left: bool,
    pub right: bool,
    pub bottom: bool,
}

/// Live window chrome state, snapshotted from the platform window at render
/// time. Drives the CSD corner policy: floating windows keep the per-skin
/// radius; fullscreen/maximized windows and tiled edges get square corners —
/// the same rules native CSD apps follow (Windows DWM also drops rounding when
/// maximized/snapped).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct WindowChromeState {
    pub maximized: bool,
    pub fullscreen: bool,
    pub tiling: EdgeTiling,
}

impl WindowChromeState {
    // Snapshot the current platform window's chrome facts. The gpui reading
    // (`window_chrome_state(&gpui::Window)`) lives behind the theme's
    // `gpui` feature (ADR-026); this type itself stays platform-neutral.

    /// Whether `corner` should keep its rounded radius under the current
    /// window state. Rule set (matching native CSD behavior):
    /// * fullscreen / maximized → every corner square;
    /// * a tiled edge → the two corners touching that edge square;
    /// * anything else (floating window) → rounded.
    pub fn corner_enabled(self, corner: WindowCorner) -> bool {
        if self.fullscreen || self.maximized {
            return false;
        }
        let t = self.tiling;
        match corner {
            WindowCorner::TopLeft => !t.top && !t.left,
            WindowCorner::TopRight => !t.top && !t.right,
            WindowCorner::BottomLeft => !t.bottom && !t.left,
            WindowCorner::BottomRight => !t.bottom && !t.right,
        }
    }
}

/// Unified corner-radius scale (Zed-style): every skin supplies ONE monotonic
/// gradient, and UI layers take their corner radius by semantic size tier
/// instead of per-skin ad-hoc values. Tiers are size-ranked, not component
/// classes — `XSmall` is the tightest corner, `XLarge` the roundest — so a
/// layer's intent (tight control vs. large surface) is skin-independent while
/// the concrete pixels stay platform-idiomatic.
///
/// Current semantic binding: controls → [`RadiusScale::Medium`], cards/panels →
/// [`RadiusScale::Large`], window chrome → [`RadiusScale::XLarge`] (the tier
/// that feeds [`Theme::window_corner_radius`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RadiusScale {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

impl RadiusScale {
    pub const ALL: [RadiusScale; 5] = [
        RadiusScale::XSmall,
        RadiusScale::Small,
        RadiusScale::Medium,
        RadiusScale::Large,
        RadiusScale::XLarge,
    ];
    /// Index into a skin's per-tier gradient array.
    pub(crate) const fn idx(self) -> usize {
        match self {
            RadiusScale::XSmall => 0,
            RadiusScale::Small => 1,
            RadiusScale::Medium => 2,
            RadiusScale::Large => 3,
            RadiusScale::XLarge => 4,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    // Identity
    pub skin: Skin,
    pub mode: LightDark,
    pub dark: bool,
    pub hc: bool,
    pub material: Material,
    pub window_controls: WindowControls,

    // Surfaces
    pub window_bg: Color,
    pub view_bg: Color,
    pub sidebar_bg: Color,
    pub sidebar_card_bg: Color,
    pub card_bg: Color,
    pub border: Color,
    pub shade: Color,
    pub scrim: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub accent: Color,
    pub accent_text: Color,

    // Per-category graph accents
    pub cpu: Color,
    pub memory: Color,
    pub disk: Color,
    pub network: Color,
    pub fan: Color,
    pub gpu: Color,
    pub battery: Color,

    // Semantic status accents (also the `Palette` danger/success/warning
    // sources): danger is the app-wide destructive red, success/warning are
    // the variant's own green/amber hues (see `skins`).
    pub danger: Color,
    pub success: Color,
    pub warning: Color,

    // Metrics
    // The radius system is a unified per-skin scale: each skin constructor
    // defines one gradient (`radius_scale`), and `radius(scale)` serves every
    // semantic. The three legacy fields below are DERIVED from that gradient
    // in `radius_fields` and kept only as compatibility for renderers that
    // still read the raw field (root chrome, elements); new call sites must
    // use `t.radius(RadiusScale::…)` by semantic.
    pub card_radius: f32,
    pub control_radius: f32,
    pub window_radius: f32,
    /// The skin's corner-radius gradient, XSmall→XLarge. The single source of
    /// truth for all corner radii in the UI; populated by each skin
    /// constructor. Read through [`Theme::radius`].
    pub(crate) radius_scale: [f32; 5],
    pub ui_font: &'static str,
    pub mono_font: &'static str,
    /// True when the OS window surface itself is transparent and the window
    /// chrome must paint the native corner rounding itself (Linux/Wayland CSD:
    /// KWin/Niri do not round CSD surfaces). macOS/Windows leave the system
    /// window server to round natively, so this stays false there. Consumed by
    /// the root/titlebar/sidebar/scrim renderers via [`Theme::window_corner_radius`].
    pub window_transparent: bool,
    /// Live chrome state (maximized/fullscreen/tiling), refreshed from the
    /// platform window at the start of every render. Corners are suppressed
    /// per [`WindowChromeState::corner_enabled`]; on macOS/Windows
    /// (non-transparent surfaces) it is inert.
    pub window_state: WindowChromeState,

    // Render-only accessibility policy derived from the owning RootView. This
    // is deliberately not an input state source: each frame copies the
    // resolved skin and injects the current per-window focus-visible decision.
    pub(crate) focus_visible: bool,
}

impl Theme {
    /// Produce the immutable theme snapshot used for one window render.
    pub const fn with_focus_visible(mut self, focus_visible: bool) -> Self {
        self.focus_visible = focus_visible;
        self
    }

    /// Whether shared controls should paint their focus indicator this frame.
    pub const fn focus_visible(self) -> bool {
        self.focus_visible
    }

    pub fn build(skin: Skin, mode: LightDark, hc: HighContrast, fonts: ResolvedFonts) -> Self {
        let mut t = tokens_for(skin, mode).into_theme(skin, mode);
        t.ui_font = fonts.ui;
        t.mono_font = fonts.mono;
        if hc == HighContrast::On {
            apply_high_contrast(&mut t);
        }
        t
    }

    /// Resolve skin from native-adapter appearance facts. The executable
    /// composition edge feeds [`NativeAppearance`] (see
    /// `taskmanager_gpui::gpui_app::theme::detect`)
    /// for the taskmanager-core `DesktopAppearance` adaptation + the
    /// `TM_SKIN`/`TM_SKIN_HC` testing override).
    pub fn detect(appearance: NativeAppearance) -> Self {
        let skin = detect_skin(appearance);
        Self::build(
            skin,
            detect_mode(appearance),
            detect_high_contrast(appearance),
            ResolvedFonts::system_for(skin),
        )
    }

    /// Toggle light/dark, keeping skin (and clearing HC).
    pub fn toggle_mode(&mut self) {
        self.mode = self.mode.other();
        self.recompose(self.skin, self.mode, self.hc);
    }

    /// Switch skin, keeping the current mode + HC and the resolved font
    /// families (the RootView re-resolves per-skin availability when the user
    /// changes skin from the Settings UI; this raw setter preserves fonts).
    pub fn set_skin(&mut self, skin: Skin) {
        self.recompose(skin, self.mode, self.hc);
    }

    /// Switch light/dark explicitly, keeping skin + HC.
    pub fn set_mode(&mut self, mode: LightDark) {
        self.recompose(self.skin, mode, self.hc);
    }

    pub fn set_high_contrast(&mut self, hc: bool) {
        self.recompose(self.skin, self.mode, hc);
    }

    /// Rebuild the token set for a new skin/mode/HC while preserving the
    /// resolved font families and the platform window-transparency decision.
    fn recompose(&mut self, skin: Skin, mode: LightDark, hc: bool) {
        let transparent = self.window_transparent;
        let state = self.window_state;
        *self = Self::build(
            skin,
            mode,
            if hc {
                HighContrast::On
            } else {
                HighContrast::Off
            },
            self.fonts(),
        );
        self.window_transparent = transparent;
        self.window_state = state;
    }

    /// The corner radius for `scale` in the active skin's gradient. All UI
    /// layers read their corner radius through this semantic: controls →
    /// [`RadiusScale::Medium`], cards/panels → [`RadiusScale::Large`], window
    /// chrome → [`RadiusScale::XLarge`].
    pub const fn radius(&self, scale: RadiusScale) -> f32 {
        self.radius_scale[scale.idx()]
    }

    /// The radius to paint for `corner` this frame. Non-zero only for a
    /// transparent (Linux CSD) surface whose window state keeps the corner
    /// rounded; maximized/fullscreen/tiled states and non-Linux platforms
    /// resolve to `0` (system or compositor handles the shape there).
    pub fn window_corner_radius(self, corner: WindowCorner) -> f32 {
        if self.window_transparent && self.window_state.corner_enabled(corner) {
            self.radius(RadiusScale::XLarge)
        } else {
            0.0
        }
    }

    /// The currently resolved font families (used to preserve them across
    /// skin/mode/HC rebuilds).
    fn fonts(self) -> ResolvedFonts {
        ResolvedFonts {
            ui: self.ui_font,
            mono: self.mono_font,
        }
    }

    // ── Derived surface tokens ─────────────────────────────────────────────
    // Semantic state surfaces derived from the skin tokens (accent/fg). Views
    // and components must read these, never re-derive ad-hoc alpha blends —
    // one source of truth, consistent across rows, buttons and overlays.

    /// Selected-row / active-selection surface: translucent accent tint
    /// (Win11 TM selected-row parity). Widened from 0.12 to 0.16 so a selected
    /// row is clearly distinct from a merely hovered one (the old 0.12-vs-0.10
    /// delta was below the just-noticeable-difference); the selection identity
    /// is still led by the accent rail (`SELECTION_RAIL`), not a full wash.
    pub const fn selection_bg(self) -> Color {
        self.accent.with_alpha(0.16)
    }

    /// Hovered-row / hovered-control surface: fainter accent tint. Lowered
    /// from 0.10 to 0.08 to open a perceptible gap below `selection_bg`.
    pub const fn hover_bg(self) -> Color {
        self.accent.with_alpha(0.08)
    }

    /// Odd-row zebra surface: a neutral fg tint that lightens odd rows on dark
    /// skins / darkens them on light skins — universally legible without a
    /// dedicated zebra color.
    pub const fn zebra_bg(self) -> Color {
        self.fg.with_alpha(0.03)
    }

    /// Search-match text color (the semantic alias of the accent for
    /// query-highlight runs — kept separate so a future accent change cannot
    /// silently alter search legibility).
    pub const fn highlight_fg(self) -> Color {
        self.accent
    }

    /// Card-surface fill for graph cards, panels, tooltips and the overlay
    /// family — the app's ELEVATED surface. Uses the skin's own `card_bg`
    /// when the table distinguishes cards from the view surface (GNOME,
    /// light KDE/macOS); when the table defines `card_bg == view_bg`
    /// (dark KDE/Windows/macOS, light Windows) the card fill is derived:
    /// dark modes lift ~4% toward the (near-white) foreground so cards read
    /// as raised layers instead of flat boxes on the backdrop; light modes
    /// blend toward white (Win11 card-white-on-grey). Never transparent —
    /// the same opaque contract as `Palette::surface`.
    pub fn card_surface(self) -> Color {
        let card = self.card_bg;
        if card != self.view_bg {
            return card;
        }
        if self.dark {
            mix(card, self.fg, 0.04)
        } else {
            mix(card, Color::WHITE, 0.5)
        }
    }

    /// Card/panel drop-shadow ink (Mission Center soft two-layer shadows).
    /// Mode-derived from the neutral `shade` token: dark skins cast a
    /// translucent BLACK shadow (raised cards read against dark chrome),
    /// light skins darken `shade` 55% toward black at 40% alpha (grey ink
    /// reads softer than pure black on light surfaces). Always translucent —
    /// the two-layer shadow helper scales this ink by layer.
    pub const fn card_shadow(self) -> Color {
        if self.dark {
            Color::BLACK.with_alpha(0.45)
        } else {
            let darken = 0.55;
            Color::new(
                self.shade.r * (1.0 - darken),
                self.shade.g * (1.0 - darken),
                self.shade.b * (1.0 - darken),
                0.4,
            )
        }
    }

    /// Accent-gradient START stop: the accent lifted 14% toward white — the
    /// brighter top edge of a Mission Center primary-button gradient.
    pub const fn gradient_from(self) -> Color {
        let lift = 0.14;
        Color::new(
            self.accent.r + (1.0 - self.accent.r) * lift,
            self.accent.g + (1.0 - self.accent.g) * lift,
            self.accent.b + (1.0 - self.accent.b) * lift,
            self.accent.a,
        )
    }

    /// Accent-gradient END stop: the accent sunk 16% toward black — the
    /// darker bottom edge that gives a 90° button gradient its depth.
    pub const fn gradient_to(self) -> Color {
        let sink = 0.16;
        Color::new(
            self.accent.r * (1.0 - sink),
            self.accent.g * (1.0 - sink),
            self.accent.b * (1.0 - sink),
            self.accent.a,
        )
    }

    /// Default theme (GNOME dark, no high-contrast) — the test harness + cold
    /// start use this until the skin detector runs.
    pub fn dark() -> Self {
        Self::build(
            Skin::Gnome,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        )
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    c.with_alpha(a)
}

/// Boost border strength + dim-text opacity for the high-contrast variant
/// (GNOME idiom #10: border-opacity 15→50%, dim-opacity 55→90%).
fn apply_high_contrast(t: &mut Theme) {
    t.hc = true;
    // Solid, high-contrast borders.
    t.border = if t.dark { Color::WHITE } else { Color::BLACK };
    // Dim text much closer to full foreground.
    t.fg_dim = with_alpha(t.fg, 0.92);
    // Card/sidebar surfaces gain a visible edge against the window.
    t.sidebar_card_bg = t.shade;
}

#[cfg(test)]
#[path = "../tests/headless/theme.rs"]
mod tests;
