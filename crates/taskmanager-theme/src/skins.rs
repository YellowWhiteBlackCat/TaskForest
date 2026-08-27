//! The theme engine has two explicit axes: product-owned color modes
//! (Light/Dark/EyeForest) and native secondary chrome (GNOME/KDE/Windows/macOS).
//! The legacy native tables below remain the source for radius/material/window
//! control adaptation; `tokens_for` overlays them onto our product palette so
//! system colors never replace the app's visual identity. High-contrast is a
//! post-transform applied by [`Theme::build`] (see
//! `crate::theme::apply_high_contrast`), not a separate table.
//!
//! [`SkinTokens`] also carries the three semantic status accents that feed
//! [`crate::Palette`] (`danger` / `success` / `warning`): danger is the
//! app-wide destructive red, success/warning reuse the variant's own
//! green/amber hues so the palette never invents new colors.

use crate::color::Color;
use crate::color::on_accent;
use crate::theme::{LightDark, Material, RadiusScale, Skin, Theme, WindowControls};

/// One skin × mode variant's full resolved token table (high-contrast applied
/// later). `into_theme` materializes it into a [`Theme`] snapshot.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SkinTokens {
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
    pub cpu: Color,
    pub memory: Color,
    pub disk: Color,
    pub network: Color,
    pub fan: Color,
    pub gpu: Color,
    pub battery: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    /// The skin's corner-radius gradient, XSmall→XLarge.
    pub radii: [f32; 5],
    pub material: Material,
    pub window_controls: WindowControls,
}

impl SkinTokens {
    /// Materialize the variant into a [`Theme`] snapshot with the standard
    /// defaults (no high contrast, no transparency, no per-frame state).
    pub(crate) fn into_theme(self, skin: Skin, mode: LightDark) -> Theme {
        let (card_radius, control_radius, window_radius) = radius_fields(self.radii);
        Theme {
            skin,
            mode,
            dark: mode.is_dark(),
            hc: false,
            material: self.material,
            window_controls: self.window_controls,
            window_bg: self.window_bg,
            view_bg: self.view_bg,
            sidebar_bg: self.sidebar_bg,
            sidebar_card_bg: self.sidebar_card_bg,
            card_bg: self.card_bg,
            border: self.border,
            shade: self.shade,
            scrim: self.scrim,
            fg: self.fg,
            fg_dim: self.fg_dim,
            accent: self.accent,
            accent_text: self.accent_text,
            cpu: self.cpu,
            memory: self.memory,
            disk: self.disk,
            network: self.network,
            fan: self.fan,
            gpu: self.gpu,
            battery: self.battery,
            danger: self.danger,
            success: self.success,
            warning: self.warning,
            card_radius,
            control_radius,
            window_radius,
            radius_scale: self.radii,
            ui_font: skin.ui_font(),
            mono_font: skin.mono_font(),
            window_transparent: false,
            window_state: Default::default(),
            focus_visible: false,
        }
    }
}

/// Resolve one product color mode first, then apply only the selected native
/// skin's secondary chrome contract (radius, material, and window controls).
/// The product palette therefore remains the visual authority across hosts;
/// system integration adapts structure without replacing our colors.
pub fn tokens_for(skin: Skin, mode: LightDark) -> SkinTokens {
    let native_mode = if mode.is_dark() {
        LightDark::Dark
    } else {
        LightDark::Light
    };
    let native = match skin {
        Skin::Gnome => gnome(native_mode),
        Skin::Kde => kde(native_mode),
        Skin::Windows => windows(native_mode),
        Skin::Macos => macos(native_mode),
    };
    let mut product = product_tokens(mode);
    product.radii = native.radii;
    product.material = native.material;
    product.window_controls = native.window_controls;
    product
}

fn ra(hex: u32, a: f32) -> Color {
    Color::from_hex(hex).with_alpha(a)
}

fn scrim() -> Color {
    // Modal/dialog backdrop — a neutral dark scrim (~black 55%).
    Color::new(0.0, 0.0, 0.0, 0.55)
}

fn product_tokens(mode: LightDark) -> SkinTokens {
    match mode {
        LightDark::Light => product_light(),
        LightDark::Dark => product_dark(),
        LightDark::EyeForest => product_eyeforest(),
    }
}

fn product_light() -> SkinTokens {
    product_palette(ProductPaletteSpec {
        window_bg: 0xf3f6f4,
        view_bg: 0xedf2ee,
        sidebar_bg: 0xe3ebe5,
        sidebar_card_bg: 0xd9e4dc,
        card_bg: 0xfbfdfb,
        border: 0xc2cec5,
        shade: 0xdfe8e2,
        fg: 0x1d2a21,
        fg_dim: 0x5b6b61,
        accent: 0x2f6f52,
        graph: [
            0x2f6f52, 0x7561a8, 0x9b7124, 0x3e8a76, 0x5073a4, 0x996552, 0x3e8a56,
        ],
        danger: 0xb83d4e,
        success: 0x3e8a56,
        warning: 0x9b7124,
    })
}

fn product_dark() -> SkinTokens {
    product_palette(ProductPaletteSpec {
        window_bg: 0x18221d,
        view_bg: 0x141c17,
        sidebar_bg: 0x202d24,
        sidebar_card_bg: 0x2a392f,
        card_bg: 0x25332a,
        border: 0x3b4d40,
        shade: 0x111812,
        fg: 0xe7f1e9,
        fg_dim: 0xa5b5a9,
        accent: 0x75c69a,
        graph: [
            0x75c69a, 0xc0a4e6, 0xe6bd66, 0x72cbb4, 0x94b6e7, 0xdf9f86, 0x83d59e,
        ],
        danger: 0xff7d8b,
        success: 0x83d59e,
        warning: 0xe6bd66,
    })
}

fn product_eyeforest() -> SkinTokens {
    let mut palette = product_palette(ProductPaletteSpec {
        window_bg: 0xeff5ec,
        view_bg: 0xe6efe3,
        sidebar_bg: 0xd9e8d7,
        sidebar_card_bg: 0xcfe0cc,
        card_bg: 0xf8fbf5,
        border: 0xb3c8ae,
        shade: 0xd6e4d3,
        fg: 0x1f3525,
        fg_dim: 0x57705a,
        accent: 0x306f4e,
        graph: [
            0x34734f, 0x725b8e, 0x9a6b20, 0x3b7f69, 0x4b6d99, 0x8b5e4c, 0x3e7d50,
        ],
        danger: 0xb33b46,
        success: 0x3e7d50,
        warning: 0xa97028,
    });
    palette.scrim = Color::new(0.10, 0.18, 0.12, 0.52);
    palette
}

/// Construct the product-owned palette shared by every native skin. The
/// seven graph accents stay semantically distinct while using a deliberately
/// restrained saturation range for long monitoring sessions.
struct ProductPaletteSpec {
    window_bg: u32,
    view_bg: u32,
    sidebar_bg: u32,
    sidebar_card_bg: u32,
    card_bg: u32,
    border: u32,
    shade: u32,
    fg: u32,
    fg_dim: u32,
    accent: u32,
    graph: [u32; 7],
    danger: u32,
    success: u32,
    warning: u32,
}

fn product_palette(spec: ProductPaletteSpec) -> SkinTokens {
    let ProductPaletteSpec {
        window_bg,
        view_bg,
        sidebar_bg,
        sidebar_card_bg,
        card_bg,
        border,
        shade,
        fg,
        fg_dim,
        accent,
        graph,
        danger,
        success,
        warning,
    } = spec;
    let [cpu, memory, disk, network, gpu, fan, battery] = graph;
    let accent = Color::from_hex(accent);
    SkinTokens {
        window_bg: Color::from_hex(window_bg),
        view_bg: Color::from_hex(view_bg),
        sidebar_bg: Color::from_hex(sidebar_bg),
        sidebar_card_bg: Color::from_hex(sidebar_card_bg),
        card_bg: Color::from_hex(card_bg),
        border: Color::from_hex(border),
        shade: Color::from_hex(shade),
        scrim: Color::new(0.0, 0.0, 0.0, 0.52),
        fg: Color::from_hex(fg),
        fg_dim: Color::from_hex(fg_dim),
        accent,
        accent_text: on_accent(accent),
        cpu: Color::from_hex(cpu),
        memory: Color::from_hex(memory),
        disk: Color::from_hex(disk),
        network: Color::from_hex(network),
        fan: Color::from_hex(fan),
        gpu: Color::from_hex(gpu),
        battery: Color::from_hex(battery),
        danger: Color::from_hex(danger),
        success: Color::from_hex(success),
        warning: Color::from_hex(warning),
        // Native skin geometry is applied by `tokens_for` after this product
        // palette is built.
        radii: [4.0, 6.0, 8.0, 10.0, 12.0],
        material: Material::Opaque,
        window_controls: WindowControls::AdwaitaClose,
    }
}

/// Map the three legacy compat radius fields onto a skin's gradient. Each
/// constructor defines its gradient once (the XSmall→XLarge array stored in
/// `radii`) and derives the historical `card_radius` / `control_radius` /
/// `window_radius` values from the tiers they replaced — cards/panels took the
/// [`RadiusScale::Large`] tier, controls [`RadiusScale::Medium`], window
/// chrome [`RadiusScale::XLarge`].
fn radius_fields(scale: [f32; 5]) -> (f32, f32, f32) {
    let tier = |s: RadiusScale| scale[s.idx()];
    (
        tier(RadiusScale::Large),
        tier(RadiusScale::Medium),
        tier(RadiusScale::XLarge),
    )
}

// ── GNOME / libadwaita (Adwaita, GNOME 48 "colder") ──────────────────────
// Verbatim from libadwaita _colors/_palette (GNOME 48). Borders & fg are the
// alpha-based tokens composited over window_bg. Accent fill is identical in
// light+dark; the standalone text/line variant is oklab-derived (darker in
// light ~#1c71d8, lighter in dark ~#62a0ea). Dark surfaces are translucent-over-
// window_bg flattened to opaque hexes. Graph accents step one index lighter in
// dark for legibility.
fn gnome(mode: LightDark) -> SkinTokens {
    let dark = mode.is_dark();
    let (window_bg, view_bg, sidebar_bg, card_bg, border, shade, fg, fg_dim) = if dark {
        (
            Color::from_hex(0x222226), // window_bg (colder gray)
            Color::from_hex(0x1d1d20), // view_bg
            Color::from_hex(0x2e2e32), // sidebar_bg
            Color::from_hex(0x343437), // card_bg = white@8% over window_bg
            Color::from_hex(0x434347), // border (white@12% composited)
            Color::from_hex(0x19191e), // shade
            Color::from_hex(0xffffff), // fg
            Color::from_hex(0x9c9c9d), // fg_dim = fg@55%
        )
    } else {
        (
            Color::from_hex(0xfafafb),
            Color::from_hex(0xffffff),
            Color::from_hex(0xebebed),
            Color::from_hex(0xffffff),
            Color::from_hex(0xd4d4d6), // border (currentColor@15% composited)
            Color::from_hex(0xe8e8ea), // shade
            Color::from_hex(0x323237), // fg = black@80%
            Color::from_hex(0x8c8c8f), // fg_dim = fg@55%
        )
    };
    let (cpu, memory, disk, network, gpu, fan, battery) = if dark {
        // cpu=blue_3, memory=purple_2, disk=yellow_4, network=green_3, gpu=teal, fan=slate, battery=green_4
        (
            Color::from_hex(0x3584e4),
            Color::from_hex(0xc061cb),
            Color::from_hex(0xf5c211),
            Color::from_hex(0x33d17a),
            Color::from_hex(0x2190a4),
            Color::from_hex(0x6f8396),
            Color::from_hex(0x2ec27e),
        )
    } else {
        // blue_3, purple_3, yellow_5, green_4, teal, slate, green_5
        (
            Color::from_hex(0x3584e4),
            Color::from_hex(0x9141ac),
            Color::from_hex(0xe5a50a),
            Color::from_hex(0x2ec27e),
            Color::from_hex(0x2190a4),
            Color::from_hex(0x6f8396),
            Color::from_hex(0x26a269),
        )
    };
    let accent = Color::from_hex(0x3584e4);
    // Radius gradient: control 8 (Medium), card 10 (Large), window 12 (XLarge) —
    // Adwaita-aligned (GNOME 46+ windows round at 12px; the strictly-increasing
    // scale resolves cards/controls just below it).
    let radii = [4.0, 6.0, 8.0, 10.0, 12.0];
    // Palette status accents: danger = app-wide destructive red; success =
    // the green_4/green_5 battery hue; warning = the yellow_4/yellow_5 disk hue.
    SkinTokens {
        window_bg,
        view_bg,
        sidebar_bg,
        sidebar_card_bg: shade,
        card_bg,
        border,
        shade,
        scrim: scrim(),
        fg,
        fg_dim,
        accent,
        accent_text: on_accent(accent),
        cpu,
        memory,
        disk,
        network,
        fan,
        gpu,
        battery,
        danger: Color::from_hex(0xe0245e),
        success: battery,
        warning: disk,
        radii,
        material: Material::Opaque,
        window_controls: WindowControls::AdwaitaClose,
    }
}

// ── KDE Plasma / Breeze (Plasma 6) ───────────────────────────────────────
// Verbatim from breeze colors/Breeze{Dark,Light}.colors. Three-tier bg hierarchy
// (View darkest-in-dark / pure-white-in-light, Window chrome, Button/card). Accent
// #3DAEE9 = DecorationFocus; #1D99F3 = ForegroundLink (secondary blue). Semantic
// graph colors identical across light+dark. FLAT: 1px borders, no translucency.
fn kde(mode: LightDark) -> SkinTokens {
    let dark = mode.is_dark();
    let (window_bg, view_bg, sidebar_bg, card_bg, border, shade, fg, fg_dim) = if dark {
        (
            Color::from_hex(0x2a2e32), // ViewBackgroundNormal dark
            Color::from_hex(0x31363b), // container dark
            Color::from_hex(0x232629), // sidebar (darker chrome)
            Color::from_hex(0x31363b), // card dark
            Color::from_hex(0x474b4f), // border dark
            Color::from_hex(0x35393e), // shade dark
            Color::from_hex(0xeff0f1), // fg dark
            Color::from_hex(0x9da1a5), // fg_dim dark
        )
    } else {
        (
            Color::from_hex(0xeff0f1), // ViewBackgroundNormal
            Color::from_hex(0xf3f4f5), // container
            Color::from_hex(0xe2e3e4), // sidebar (darker chrome)
            Color::from_hex(0xffffff), // card
            Color::from_hex(0xbcbec0), // border
            Color::from_hex(0xdfe0e1), // shade
            Color::from_hex(0x232627), // fg
            Color::from_hex(0x626568), // fg_dim
        )
    };
    let (cpu, memory, disk, network, gpu, fan, battery) = if dark {
        (
            Color::from_hex(0x3daee9), // Breeze Blue
            Color::from_hex(0xda4453), // Breeze Red
            Color::from_hex(0xf47750), // Breeze Orange
            Color::from_hex(0x27ae60), // Breeze Green
            Color::from_hex(0xbd93f9), // Breeze Purple dark
            Color::from_hex(0xfdbc4b), // amber
            Color::from_hex(0x1abc9c), // teal
        )
    } else {
        (
            Color::from_hex(0x3daee9), // Breeze Blue
            Color::from_hex(0xda4453), // Breeze Red
            Color::from_hex(0xf47750), // Breeze Orange
            Color::from_hex(0x27ae60), // Breeze Green
            Color::from_hex(0x9b59b6), // Breeze Purple light
            Color::from_hex(0xfdbc4b), // amber
            Color::from_hex(0x1abc9c), // teal
        )
    };
    // Radius gradient: control 4 (Medium), card 5 (Large), window 6 (XLarge) —
    // Breeze is the flattest skin (1px borders, no translucency).
    let radii = [2.0, 3.0, 4.0, 5.0, 6.0];
    // Palette status accents: danger = app-wide destructive red; success =
    // Breeze Green (the network hue); warning = the amber fan hue.
    SkinTokens {
        window_bg,
        view_bg,
        sidebar_bg,
        sidebar_card_bg: if dark {
            Color::from_hex(0x35393e)
        } else {
            Color::from_hex(0xdfe0e1)
        },
        card_bg,
        border,
        shade,
        scrim: scrim(),
        fg,
        fg_dim,
        accent: Color::from_hex(0x3daee9),
        accent_text: on_accent(Color::from_hex(0x3daee9)),
        cpu,
        memory,
        disk,
        network,
        fan,
        gpu,
        battery,
        danger: Color::from_hex(0xe0245e),
        success: network,
        warning: fan,
        radii,
        material: Material::Opaque,
        window_controls: WindowControls::Breeze,
    }
}

// ── Windows 11 / WinUI 3 (Fluent) ────────────────────────────────────────
// WinUI 3 SolidBackgroundFillColorBase + accent tokens, Mica transparency
// on dark surfaces. Card fills use SemiTransparentLayerOnMica flattened to
// opaque hexes. Accent #0067c0 (light) / #60cdff (dark). Control radius 4px.
fn windows(mode: LightDark) -> SkinTokens {
    let dark = mode.is_dark();
    let (window_bg, view_bg, sidebar_bg, card_bg, border, shade, fg, fg_dim) = if dark {
        (
            ra(0x202020, 0.95),        // SolidBackgroundFillColorBase with Mica hint
            Color::from_hex(0x2d2d2d), // Card/Container background
            ra(0x181818, 0.95),        // sidebar (darker chrome)
            Color::from_hex(0x2d2d2d), // CardBackground
            Color::from_hex(0x404040), // elevation border
            Color::from_hex(0x252525), // shade
            Color::from_hex(0xffffff), // TextFillColor
            Color::from_hex(0x999999), // TextFillColorSecondary
        )
    } else {
        (
            Color::from_hex(0xf9f9f9), // Window/Surface background
            Color::from_hex(0xf3f3f3), // SolidBackgroundFillColorBase
            Color::from_hex(0xe8e8e8), // sidebar
            Color::from_hex(0xf3f3f3), // CardBackground
            Color::from_hex(0xd0d0d0), // CardStrokeColorDefault
            Color::from_hex(0xe0e0e0), // shade
            Color::from_hex(0x1a1a1a), // TextFillColor
            Color::from_hex(0x5c5c5c), // TextFillColorSecondary
        )
    };
    let accent = if dark {
        Color::from_hex(0x60cdff) // SystemAccentColorLight2
    } else {
        Color::from_hex(0x0067c0) // SystemAccentColor
    };
    let (cpu, memory, disk, network, gpu, fan, battery) = if dark {
        (
            Color::from_hex(0x60cdff), // cpu: system blue
            Color::from_hex(0xe48bd5), // memory: magenta
            Color::from_hex(0xffb04d), // disk: orange
            Color::from_hex(0x3de0e9), // network: teal
            Color::from_hex(0xb49ef0), // gpu: purple
            Color::from_hex(0xff9d5c), // fan: warm apricot
            Color::from_hex(0x6ccb5f), // battery: green
        )
    } else {
        (
            Color::from_hex(0x0078d4), // cpu: system blue
            Color::from_hex(0xc239b3), // memory: magenta
            Color::from_hex(0xff8c00), // disk: orange
            Color::from_hex(0x00b7c3), // network: teal
            Color::from_hex(0x886ce4), // gpu: purple
            Color::from_hex(0xca5010), // fan: burnt orange
            Color::from_hex(0x107c10), // battery: green
        )
    };
    // Radius gradient: control 4 (Medium = Fluent ControlCornerRadius),
    // card 6 (Large), window 8 (XLarge = WindowCornerRadius).
    let radii = [2.0, 3.0, 4.0, 6.0, 8.0];
    // Palette status accents: danger = app-wide destructive red; success =
    // the battery green; warning = the orange disk hue.
    SkinTokens {
        window_bg,
        view_bg,
        sidebar_bg,
        sidebar_card_bg: if dark {
            Color::from_hex(0x252525)
        } else {
            Color::from_hex(0xe0e0e0)
        },
        card_bg,
        border,
        shade,
        scrim: ra(0x000000, 0.30), // SmokeFillColorDefault #4D000000
        fg,
        fg_dim,
        accent,
        accent_text: on_accent(accent),
        cpu,
        memory,
        disk,
        network,
        fan,
        gpu,
        battery,
        danger: Color::from_hex(0xe0245e),
        success: battery,
        warning: disk,
        radii,
        material: Material::Mica,
        window_controls: WindowControls::Caption,
    }
}

// ── macOS Sonoma ─────────────────────────────────────────────────────────
// Opaque vibrancy fallbacks (vibrancy disabled/screenshot mode). labelColor
// etc. are alpha-based dynamic colors resolved here to opaque hexes. Accent =
// systemBlue (#007AFF/#0A84FF); user-accent hook ready. System hues stable
// Big Sur→Sonoma.
fn macos(mode: LightDark) -> SkinTokens {
    let dark = mode.is_dark();
    let (window_bg, view_bg, sidebar_bg, card_bg, border, shade, fg, fg_dim, accent) = if dark {
        (
            Color::from_hex(0x242426), // windowBackgroundColor (vibrancy dark fallback)
            Color::from_hex(0x2c2c2e), // controlBackgroundColor dark
            ra(0x1e1e20, 0.97),        // sidebar (quaternarySystemFill dark)
            Color::from_hex(0x2c2c2e), // textBackgroundColor dark / card
            Color::from_hex(0x3a3a3c), // separatorColor dark
            Color::from_hex(0x323234), // shade
            Color::from_hex(0xf5f5f7), // labelColor dark
            Color::from_hex(0x98989d), // secondaryLabelColor dark
            Color::from_hex(0x0a84ff), // systemBlue dark
        )
    } else {
        (
            Color::from_hex(0xececec), // windowBackgroundColor
            Color::from_hex(0xf0f0f0), // controlBackgroundColor
            Color::from_hex(0xe6e8ea), // quaternarySystemFill
            Color::from_hex(0xffffff), // textBackgroundColor
            Color::from_hex(0xd2d2d2), // separatorColor
            Color::from_hex(0xe0e0e0), // shade
            Color::from_hex(0x1d1d1f), // labelColor
            Color::from_hex(0x6e6e73), // secondaryLabelColor
            Color::from_hex(0x007aff), // systemBlue light
        )
    };
    let (cpu, memory, disk, network, gpu, fan, battery) = if dark {
        (
            Color::from_hex(0x0a84ff), // systemBlue dark
            Color::from_hex(0xff453a), // systemRed dark
            Color::from_hex(0xffd60a), // systemOrange dark
            Color::from_hex(0x30d158), // systemGreen dark
            Color::from_hex(0xbf5af2), // systemPurple dark
            Color::from_hex(0x6ac4dc),
            Color::from_hex(0x30d158),
        )
    } else {
        (
            Color::from_hex(0x007aff), // systemBlue
            Color::from_hex(0xff375f), // systemRed
            Color::from_hex(0xff9f0a), // systemOrange
            Color::from_hex(0x30d158), // systemGreen
            Color::from_hex(0xaf52de), // systemPurple
            Color::from_hex(0x59adc4),
            Color::from_hex(0x34c759),
        )
    };
    // Radius gradient: control 6 (Medium), card 8 (Large), window 10 (XLarge).
    let radii = [2.0, 4.0, 6.0, 8.0, 10.0];
    // Palette status accents: danger = app-wide destructive red; success =
    // the systemGreen battery hue; warning = the orange disk hue.
    SkinTokens {
        window_bg,
        view_bg,
        sidebar_bg,
        sidebar_card_bg: shade,
        card_bg,
        border,
        shade,
        scrim: scrim(),
        fg,
        fg_dim,
        accent,
        accent_text: on_accent(accent),
        cpu,
        memory,
        disk,
        network,
        fan,
        gpu,
        battery,
        danger: Color::from_hex(0xe0245e),
        success: battery,
        warning: disk,
        radii,
        material: Material::Vibrancy,
        window_controls: WindowControls::TrafficLight,
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_skins.rs"]
mod tests;
