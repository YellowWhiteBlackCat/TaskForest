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

/// The standard UI-size root used to translate physical skin tokens into REM.
pub const STANDARD_ROOT_REM_SIZE: f32 = 14.0;

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
    pub const fn label(self) -> &'static str {
        match self {
            Skin::Gnome => "GNOME",
            Skin::Kde => "KDE",
            Skin::Windows => "Windows",
            Skin::Macos => "macOS",
        }
    }
    /// The persisted [`taskforest-core` config] token for this skin. It is the
    /// label by design (ADR-063): a config file stays human-readable and one
    /// public vocabulary serves all four products, so no frontend may re-spell
    /// these strings in a private codec.
    pub const fn config_token(self) -> &'static str {
        self.label()
    }
    /// Resolve a persisted skin token (or the skin half of a
    /// [`crate::settings::AppearanceOverride`]) back into the axis. Case and
    /// surrounding whitespace are irrelevant, and the short platform aliases the
    /// appearance override has always accepted (`win`, `mac`) stay legal so a
    /// token written by any product or harness resolves through this one codec.
    /// Empty and unknown tokens yield `None`: the caller keeps the
    /// host-detected skin instead of forcing a wrong palette.
    #[must_use]
    pub fn from_config_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "gnome" => Some(Self::Gnome),
            "kde" => Some(Self::Kde),
            "windows" | "win" => Some(Self::Windows),
            "macos" | "mac" => Some(Self::Macos),
            _ => None,
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

    pub const fn label(self) -> &'static str {
        match self {
            LightDark::Light => "Light",
            LightDark::Dark => "Dark",
            LightDark::EyeForest => "EyeForest",
        }
    }
    /// The persisted color-mode token for an explicit choice. The fourth
    /// segment a product offers — follow the native desktop — is not a mode, so
    /// it lives in [`crate::settings::ModePreference`], never as a
    /// frontend-local variant of this axis.
    pub const fn config_token(self) -> &'static str {
        self.label()
    }
    /// Resolve a persisted or override mode token. Case-insensitive; accepts
    /// the historical `eye-forest` spelling. Empty, `System`, and unknown
    /// tokens yield `None` because they request native resolution rather than a
    /// concrete mode.
    #[must_use]
    pub fn from_config_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "eyeforest" | "eye-forest" => Some(Self::EyeForest),
            _ => None,
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

/// One glass layer's resolved surface tokens. Renderers use the theme's
/// gradient stops with this fill, edge and shadow; backdrop blur is available
/// only where the window system actually supplies it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassLayer {
    /// The translucent panel fill composited over the available backdrop.
    pub fill: Color,
    /// Hairline edge with enough contrast against the surrounding surface.
    pub edge: Color,
    /// The soft two-layer shadow ink (the card shadow's glass variant).
    pub shadow: Color,
}

impl GlassLayer {
    const fn new(fill: Color, edge: Color, shadow: Color) -> Self {
        Self { fill, edge, shadow }
    }
}

/// One toolkit-neutral shadow layer with resolved color and REM geometry.
///
/// Lengths stay on the root-relative axis; each frontend binding projects
/// them into its own shadow vocabulary without authoring a second ladder.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassShadowLayer {
    /// The resolved shadow ink for this layer.
    pub color: Color,
    /// Vertical offset in root-relative units.
    pub offset_y: f32,
    /// Blur radius in root-relative units.
    pub blur_radius: f32,
}

/// The complete two-layer glass elevation shadow for one tier.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlassShadow {
    /// Wide, soft ambient elevation.
    pub ambient: GlassShadowLayer,
    /// Tight contact shadow that keeps the edge attached to the backdrop.
    pub contact: GlassShadowLayer,
}

/// The renderer's declared projection of the shared glass shadow contract.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlassShadowProjection {
    /// Paint both shared layers within the theme's total shadow-ink opacity.
    Layered,
    /// Fold both layers into one alpha-weighted quad with the tier's
    /// elevation opacity, for a renderer that paints only one shadow.
    SingleQuad,
}

/// The single-quad elevation role for one glass tier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlassShadowElevation {
    Sidebar,
    Card,
    Modal,
    Scrim,
}

impl GlassShadowElevation {
    /// The elevation opacity for this surface role.
    #[must_use]
    pub const fn opacity(self) -> f32 {
        match self {
            Self::Sidebar => 0.45,
            Self::Card => 0.7,
            Self::Modal => 1.0,
            Self::Scrim => 0.0,
        }
    }
}

/// The four glass tiers the product paints, with their blur radii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlassTier {
    /// Side navigation and other persistent chrome.
    Sidebar,
    /// Floating cards, panels, menus, popovers.
    Card,
    /// Modal dialogs and the strongest elevation.
    Modal,
    /// The dimmed backdrop beneath a modal. A scrim carries no edge,
    /// shadow, blur, or noise: it is the material's depth wash, not a panel.
    Scrim,
}

impl GlassTier {
    pub const ALL: [Self; 4] = [Self::Sidebar, Self::Card, Self::Modal, Self::Scrim];

    /// The backdrop-blur radius in root-relative units for this tier.
    pub const fn blur_radius_rem(self) -> f32 {
        match self {
            Self::Sidebar => 24.0 / STANDARD_ROOT_REM_SIZE,
            Self::Card => 20.0 / STANDARD_ROOT_REM_SIZE,
            Self::Modal => 30.0 / STANDARD_ROOT_REM_SIZE,
            Self::Scrim => 0.0,
        }
    }

    /// Specular edge thickness. At the standard 14-unit root this resolves
    /// to a one-unit hairline and follows the user's UI-size choice.
    pub const fn edge_width_rem(self) -> f32 {
        match self {
            Self::Sidebar | Self::Card | Self::Modal => 1.0 / STANDARD_ROOT_REM_SIZE,
            Self::Scrim => 0.0,
        }
    }

    /// Ambient shadow vertical offset and blur radius, both in REM.
    pub const fn ambient_shadow_rem(self) -> (f32, f32) {
        match self {
            Self::Sidebar => (2.0 / STANDARD_ROOT_REM_SIZE, 12.0 / STANDARD_ROOT_REM_SIZE),
            Self::Card => (3.0 / STANDARD_ROOT_REM_SIZE, 12.0 / STANDARD_ROOT_REM_SIZE),
            Self::Modal => (8.0 / STANDARD_ROOT_REM_SIZE, 30.0 / STANDARD_ROOT_REM_SIZE),
            Self::Scrim => (0.0, 0.0),
        }
    }

    /// Contact shadow vertical offset and blur radius, both in REM.
    pub const fn contact_shadow_rem(self) -> (f32, f32) {
        match self {
            Self::Sidebar => (0.5 / STANDARD_ROOT_REM_SIZE, 3.0 / STANDARD_ROOT_REM_SIZE),
            Self::Card => (1.0 / STANDARD_ROOT_REM_SIZE, 4.0 / STANDARD_ROOT_REM_SIZE),
            Self::Modal => (2.0 / STANDARD_ROOT_REM_SIZE, 6.0 / STANDARD_ROOT_REM_SIZE),
            Self::Scrim => (0.0, 0.0),
        }
    }

    /// The two source layers with the theme's resolved ink and alpha weights.
    ///
    /// Use [`Self::projected_shadow`] when sending these layers to a renderer.
    #[must_use]
    pub fn shadow(self, theme: &Theme) -> GlassShadow {
        let ink = theme.glass_layer(self).shadow;
        let (ambient_y, ambient_blur) = self.ambient_shadow_rem();
        let (contact_y, contact_blur) = self.contact_shadow_rem();
        GlassShadow {
            ambient: GlassShadowLayer {
                color: ink.with_alpha(ink.a * AMBIENT_SHADOW_ALPHA_WEIGHT),
                offset_y: ambient_y,
                blur_radius: ambient_blur,
            },
            contact: GlassShadowLayer {
                color: ink,
                offset_y: contact_y,
                blur_radius: contact_blur,
            },
        }
    }

    /// The shared shadow layers after a renderer's declared projection.
    ///
    /// The layered projection apportions the ink's alpha in optical depth so
    /// overlapping same-ink shadows stay within the theme's opacity budget.
    /// Single-quad renderers keep their tier-specific elevation reduction.
    #[must_use]
    pub fn projected_shadow(self, theme: &Theme, projection: GlassShadowProjection) -> GlassShadow {
        let mut shadow = self.shadow(theme);
        match projection {
            GlassShadowProjection::Layered => {
                shadow = layered_shadow_alpha_budget(shadow, theme.glass_layer(self).shadow.a);
            }
            GlassShadowProjection::SingleQuad => {
                let elevation = self.single_quad_elevation().opacity();
                shadow.ambient.color = shadow
                    .ambient
                    .color
                    .with_alpha(shadow.ambient.color.a * elevation);
                shadow.contact.color = shadow
                    .contact
                    .color
                    .with_alpha(shadow.contact.color.a * elevation);
            }
        }
        shadow
    }

    /// The single-quad elevation role for this tier.
    #[must_use]
    pub const fn single_quad_elevation(self) -> GlassShadowElevation {
        match self {
            Self::Sidebar => GlassShadowElevation::Sidebar,
            Self::Card => GlassShadowElevation::Card,
            Self::Modal => GlassShadowElevation::Modal,
            Self::Scrim => GlassShadowElevation::Scrim,
        }
    }

    /// The noise-texture opacity factor for this tier (0..=1). Small on
    /// persistent chrome, strongest on the largest glass panel.
    pub const fn noise_strength(self) -> f32 {
        match self {
            Self::Sidebar => 0.04,
            Self::Card => 0.06,
            Self::Modal => 0.08,
            Self::Scrim => 0.0,
        }
    }

    /// The glass layer the active skin × mode resolves for this tier.
    pub fn layer(self, theme: &Theme) -> GlassLayer {
        theme.glass_layer(self)
    }
}

const AMBIENT_SHADOW_ALPHA_WEIGHT: f32 = 0.6;

fn layered_shadow_alpha_budget(mut shadow: GlassShadow, target_alpha: f32) -> GlassShadow {
    let ambient_depth = -(1.0 - shadow.ambient.color.a).ln();
    let contact_depth = -(1.0 - shadow.contact.color.a).ln();
    let source_depth = ambient_depth + contact_depth;
    if source_depth <= f32::EPSILON {
        return shadow;
    }

    let depth_scale = -(1.0 - target_alpha).ln() / source_depth;
    shadow.ambient.color = shadow
        .ambient
        .color
        .with_alpha(1.0 - (-ambient_depth * depth_scale).exp());
    shadow.contact.color = shadow
        .contact
        .color
        .with_alpha(1.0 - (-contact_depth * depth_scale).exp());
    shadow
}

impl Theme {
    /// Stable-role aliases over [`Theme::glass_layer`]. Persistent chrome is
    /// the most open surface, cards remain the product's default panel, and
    /// modals are the most opaque: the roles make call sites self-documenting
    /// without letting each frontend re-derive its own alpha.
    pub fn glass_chrome(self) -> GlassLayer {
        self.glass_layer(GlassTier::Sidebar)
    }

    pub fn glass_card(self) -> GlassLayer {
        self.glass_layer(GlassTier::Card)
    }

    pub fn glass_modal(self) -> GlassLayer {
        self.glass_layer(GlassTier::Modal)
    }

    pub fn glass_scrim(self) -> GlassLayer {
        self.glass_layer(GlassTier::Scrim)
    }
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

#[derive(Clone, Copy, Debug)]
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

    /// Root-relative unit size in renderer-independent logical units.
    /// Frontends with a user-controlled reading scale set this per window.
    pub root_rem_size: f32,
}

impl Theme {
    /// Carry a frontend's live root size into theme-owned surface metrics.
    #[must_use]
    pub fn with_root_rem_size(mut self, root_rem_size: f32) -> Self {
        if root_rem_size.is_finite() && root_rem_size > 0.0 {
            self.root_rem_size = root_rem_size;
        }
        self
    }

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
    /// for the taskforest-core `DesktopAppearance` adaptation + the
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
    /// resolved font families, root size, and platform window state.
    fn recompose(&mut self, skin: Skin, mode: LightDark, hc: bool) {
        let transparent = self.window_transparent;
        let state = self.window_state;
        let root_rem_size = self.root_rem_size;
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
        self.root_rem_size = root_rem_size;
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
    /// light skins darken `shade` 55% toward black at 16% alpha (grey ink
    /// reads softer than pure black on light surfaces). Always translucent —
    /// the layered shadow projection apportions this alpha across both layers
    /// so source-over compositing stays within the token's opacity budget.
    pub const fn card_shadow(self) -> Color {
        if self.dark {
            Color::BLACK.with_alpha(0.18)
        } else {
            let darken = 0.55;
            Color::new(
                self.shade.r * (1.0 - darken),
                self.shade.g * (1.0 - darken),
                self.shade.b * (1.0 - darken),
                0.16,
            )
        }
    }

    /// Accent-gradient START stop: the accent lifted 14% toward white — the
    /// brighter top edge of a Mission Center primary-button gradient.
    ///
    /// This is [`mix`] toward white with the accent's own alpha as the target
    /// alpha, so the blend formula stays the token owner's and the stop still
    /// keeps `accent.a` rather than fading toward opaque. The arithmetic is the
    /// same term order the hand-spelled channels used, so every channel is
    /// bit-for-bit unchanged.
    pub const fn gradient_from(self) -> Color {
        mix(self.accent, Color::WHITE.with_alpha(self.accent.a), 0.14)
    }

    /// The resolved glass layer for `tier`. The light-mode outline uses the
    /// palette border rather than white on white; the gradient supplies the
    /// highlight. High contrast forces an opaque fill and solid outline.
    /// A scrim is the modal depth wash rather than a panel: it keeps the
    /// theme's own translucent scrim token in every contrast mode.
    pub fn glass_layer(self, tier: GlassTier) -> GlassLayer {
        if tier == GlassTier::Scrim {
            return GlassLayer::new(self.scrim, Color::TRANSPARENT, Color::TRANSPARENT);
        }
        let base = match tier {
            GlassTier::Sidebar => self.sidebar_bg,
            GlassTier::Card | GlassTier::Modal => self.card_surface(),
            GlassTier::Scrim => self.sidebar_bg,
        };
        let (fill_alpha, edge_alpha) = if self.hc {
            (1.0, 1.0)
        } else if self.dark {
            match tier {
                GlassTier::Sidebar => (0.74, 0.23),
                GlassTier::Card => (0.78, 0.25),
                GlassTier::Modal => (0.82, 0.28),
                GlassTier::Scrim => (1.0, 0.0),
            }
        } else {
            match tier {
                GlassTier::Sidebar => (0.80, 0.48),
                GlassTier::Card => (0.82, 0.55),
                GlassTier::Modal => (0.86, 0.62),
                GlassTier::Scrim => (1.0, 0.0),
            }
        };
        let edge = if self.hc {
            self.border
        } else if self.dark {
            self.fg.with_alpha(edge_alpha)
        } else {
            self.border.with_alpha(edge_alpha)
        };
        GlassLayer::new(base.with_alpha(fill_alpha), edge, self.card_shadow())
    }

    /// Top and bottom stops for the glass surface. This is a painted sheen,
    /// not a promise that the toolkit blurs content behind the panel.
    pub fn glass_gradient(self, tier: GlassTier) -> [Color; 2] {
        let fill = self.glass_layer(tier).fill;
        if self.hc || tier == GlassTier::Scrim {
            return [fill, fill];
        }
        let top = mix(
            fill,
            Color::WHITE.with_alpha(fill.a),
            if self.dark { 0.16 } else { 0.65 },
        );
        let bottom = if self.dark {
            fill
        } else {
            mix(fill, self.sidebar_bg.with_alpha(fill.a), 0.38)
        };
        [top, bottom]
    }

    /// Translucent highlight for renderers that place a sheen over an
    /// existing panel instead of replacing its fill with a gradient.
    pub fn glass_sheen(self, tier: GlassTier) -> Color {
        if self.hc || tier == GlassTier::Scrim {
            return Color::TRANSPARENT;
        }
        let [top, _] = self.glass_gradient(tier);
        top.with_alpha(if self.dark { 0.14 } else { 0.24 })
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
