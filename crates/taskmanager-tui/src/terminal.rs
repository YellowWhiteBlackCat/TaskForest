//! Terminal capability profile and presentation fallbacks.
//!
//! A terminal is not a pixel surface.  The TUI therefore resolves the small
//! set of capabilities that affect its rendering contract once at the
//! composition edge and passes the value down as data.  Renderers do not read
//! the environment, guess from a widget, or silently emit a glyph/color that
//! the selected terminal profile cannot carry.

use std::env;

use ratatui::style::Color;
use taskmanager_ui_contract::IconId;

/// Color precision available to the terminal renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiColorMode {
    /// 24-bit RGB escape sequences are safe to emit.
    TrueColor,
    /// The xterm 256-color palette is safe to emit.
    Ansi256,
    /// The base/bright ANSI palette is safe to emit.
    Ansi16,
    /// Color should not be emitted; semantic state must survive without it.
    Monochrome,
}

/// Glyph repertoire available to the terminal renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiGlyphMode {
    /// The terminal locale/profile is expected to carry the shared Unicode
    /// icon vocabulary and half/block graph markers.
    Unicode,
    /// Use portable ASCII semantic substitutes.
    Ascii,
}

/// Immutable capabilities that affect terminal-cell presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TuiTerminalProfile {
    pub color: TuiColorMode,
    pub glyphs: TuiGlyphMode,
}

impl Default for TuiTerminalProfile {
    fn default() -> Self {
        Self {
            color: TuiColorMode::TrueColor,
            glyphs: TuiGlyphMode::Unicode,
        }
    }
}

impl TuiTerminalProfile {
    /// Resolve the profile once from process environment at the native
    /// composition edge.  Tests and remote hosts should use
    /// [`Self::from_signals`] instead of mutating global environment state.
    #[must_use]
    pub fn detect() -> Self {
        Self::from_signals(
            env::var("TERM").ok().as_deref(),
            env::var("COLORTERM").ok().as_deref(),
            env::var("NO_COLOR").is_ok(),
            env::var("TM_TUI_GLYPHS").ok().as_deref(),
            env::var("LC_ALL")
                .ok()
                .or_else(|| env::var("LC_CTYPE").ok())
                .or_else(|| env::var("LANG").ok())
                .as_deref(),
        )
    }

    /// Build a profile from already-read terminal signals.  The arguments are
    /// intentionally boring strings: this keeps capability policy pure and
    /// makes each fallback testable without unsafe environment mutation.
    #[must_use]
    pub fn from_signals(
        term: Option<&str>,
        color_term: Option<&str>,
        no_color: bool,
        glyph_override: Option<&str>,
        locale: Option<&str>,
    ) -> Self {
        let dumb = term.is_some_and(|value| value.eq_ignore_ascii_case("dumb"));
        let color = if no_color || dumb {
            TuiColorMode::Monochrome
        } else if color_term.is_some_and(|value| {
            value.eq_ignore_ascii_case("truecolor")
                || value.eq_ignore_ascii_case("24bit")
                || value.eq_ignore_ascii_case("direct")
        }) {
            TuiColorMode::TrueColor
        } else if term.is_some_and(|value| value.to_ascii_lowercase().contains("256color")) {
            TuiColorMode::Ansi256
        } else {
            TuiColorMode::Ansi16
        };

        let glyphs = match glyph_override.map(str::trim).map(str::to_ascii_lowercase) {
            Some(value) if value == "ascii" => TuiGlyphMode::Ascii,
            Some(value) if value == "unicode" => TuiGlyphMode::Unicode,
            _ if dumb => TuiGlyphMode::Ascii,
            _ if locale.is_some_and(is_utf8_locale) => TuiGlyphMode::Unicode,
            // A missing locale is not evidence that Unicode is safe.  The
            // explicit override remains available for modern terminals whose
            // locale is not exported by the host launcher.
            _ => TuiGlyphMode::Ascii,
        };

        Self { color, glyphs }
    }

    /// Map a semantic icon to the selected terminal repertoire.
    #[must_use]
    pub const fn glyph(self, icon: IconId) -> &'static str {
        match self.glyphs {
            TuiGlyphMode::Unicode => taskmanager_shell::presentation::icon_glyph(icon),
            TuiGlyphMode::Ascii => ascii_glyph(icon),
        }
    }

    /// Replace one already-painted non-ASCII cell with a single ASCII cell.
    /// This is deliberately cell-local: returning `"..."` or another
    /// multi-cell string here would make a terminal buffer overlap after the
    /// renderer has already resolved its geometry.
    #[must_use]
    pub(crate) fn ascii_cell_symbol(symbol: &str) -> &'static str {
        match symbol {
            "—" | "–" | "─" | "━" | "╼" | "╾" => "-",
            "│" | "┃" | "╽" | "╿" => "|",
            "┌" | "┐" | "└" | "┘" | "├" | "┤" | "┬" | "┴" | "┼" | "╭" | "╮" | "╰" | "╯" | "╴"
            | "╵" | "╶" | "╷" => "+",
            "·" | "•" | "∙" | "⋅" | "░" | "▒" | "▁" | "▂" | "▃" | "▄" | "▅" | "▆" | "▇" => {
                "."
            }
            "█" | "▓" | "▉" | "▊" | "▋" | "▌" | "▍" | "▎" | "▏" => "#",
            "…" => ".",
            "×" | "✗" | "✕" | "✖" => "x",
            "✓" | "✔" => "v",
            "⚠" | "△" | "⚡" => "!",
            "↑" | "⇧" | "▴" | "▲" => "^",
            "↓" | "⇩" | "▾" | "▼" => "v",
            "←" | "⇦" | "◀" | "◁" => "<",
            "→" | "⇨" | "▶" | "▷" | "▸" | "›" => ">",
            "↔" | "⇄" => "=",
            "◇" | "○" | "◎" | "◌" => "o",
            "Ⅱ" => "|",
            "⌘" | "☷" | "▥" => "*",
            "⌛" => "H",
            "⌕" => "?",
            _ => "?",
        }
    }

    /// Map a Ratatui color through the selected terminal palette.
    #[must_use]
    pub fn color(self, color: Color) -> Color {
        match self.color {
            TuiColorMode::TrueColor => color,
            TuiColorMode::Ansi256 => {
                if matches!(color, Color::Reset) {
                    Color::Reset
                } else {
                    let rgb = color_rgb(color);
                    Color::Indexed(rgb_to_ansi256(rgb))
                }
            }
            TuiColorMode::Ansi16 => {
                if matches!(color, Color::Reset) {
                    Color::Reset
                } else {
                    nearest_ansi16(color_rgb(color))
                }
            }
            TuiColorMode::Monochrome => Color::Reset,
        }
    }
}

fn is_utf8_locale(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("utf-8") || value.contains("utf8")
}

const fn ascii_glyph(icon: IconId) -> &'static str {
    match icon {
        IconId::Cpu => "C",
        IconId::Memory => "M",
        IconId::Disk => "D",
        IconId::Network => "<>",
        IconId::Gpu => "G",
        IconId::Process => "P",
        IconId::Service => "S",
        IconId::Startup => "^",
        IconId::User => "U",
        IconId::Health => "H",
        IconId::Alert => "!",
        IconId::Export => ">",
        IconId::Settings => "*",
        IconId::Search => "?",
        IconId::More => "...",
        IconId::NavigateUp => "^",
        IconId::NavigateDown => "v",
        IconId::Focus => "*",
        IconId::Performance => "~",
        IconId::Applications => "A",
        IconId::Services => "S",
        IconId::System => "#",
        IconId::Users => "U",
        IconId::Refresh => "R",
        IconId::EndTask | IconId::Close | IconId::CircleX => "x",
        IconId::Properties => "=",
        IconId::Pause => "||",
        IconId::Sidebar => "|",
        IconId::CircleCheck => "+",
        IconId::TriangleAlert => "!",
        IconId::History => "H",
    }
}

fn color_rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Reset => [255, 255, 255],
        Color::Black => [0, 0, 0],
        Color::Red => [128, 0, 0],
        Color::Green => [0, 128, 0],
        Color::Yellow => [128, 128, 0],
        Color::Blue => [0, 0, 128],
        Color::Magenta => [128, 0, 128],
        Color::Cyan => [0, 128, 128],
        Color::Gray => [192, 192, 192],
        Color::DarkGray => [128, 128, 128],
        Color::LightRed => [255, 0, 0],
        Color::LightGreen => [0, 255, 0],
        Color::LightYellow => [255, 255, 0],
        Color::LightBlue => [0, 0, 255],
        Color::LightMagenta => [255, 0, 255],
        Color::LightCyan => [0, 255, 255],
        Color::White => [255, 255, 255],
        Color::Rgb(red, green, blue) => [red, green, blue],
        Color::Indexed(index) => ansi256_rgb(index),
    }
}

fn ansi256_rgb(index: u8) -> [u8; 3] {
    match index {
        0..=15 => [
            [0, 0, 0],
            [128, 0, 0],
            [0, 128, 0],
            [128, 128, 0],
            [0, 0, 128],
            [128, 0, 128],
            [0, 128, 128],
            [192, 192, 192],
            [128, 128, 128],
            [255, 0, 0],
            [0, 255, 0],
            [255, 255, 0],
            [0, 0, 255],
            [255, 0, 255],
            [0, 255, 255],
            [255, 255, 255],
        ][usize::from(index)],
        16..=231 => {
            let index = index - 16;
            let red = index / 36;
            let green = (index % 36) / 6;
            let blue = index % 6;
            let cube = [0, 95, 135, 175, 215, 255];
            [
                cube[usize::from(red)],
                cube[usize::from(green)],
                cube[usize::from(blue)],
            ]
        }
        232..=255 => {
            let value = 8 + (index - 232) * 10;
            [value, value, value]
        }
    }
}

fn rgb_to_ansi256(rgb: [u8; 3]) -> u8 {
    let cube = [0, 95, 135, 175, 215, 255];
    let mut best = (u32::MAX, 16u8);
    for red in 0..6u8 {
        for green in 0..6u8 {
            for blue in 0..6u8 {
                let candidate = [
                    cube[usize::from(red)],
                    cube[usize::from(green)],
                    cube[usize::from(blue)],
                ];
                let distance = color_distance(rgb, candidate);
                if distance < best.0 {
                    best = (distance, 16 + 36 * red + 6 * green + blue);
                }
            }
        }
    }
    for gray in 0..24u8 {
        let value = 8 + gray * 10;
        let distance = color_distance(rgb, [value, value, value]);
        if distance < best.0 {
            best = (distance, 232 + gray);
        }
    }
    best.1
}

fn nearest_ansi16(rgb: [u8; 3]) -> Color {
    const PALETTE: [(Color, [u8; 3]); 16] = [
        (Color::Black, [0, 0, 0]),
        (Color::Red, [128, 0, 0]),
        (Color::Green, [0, 128, 0]),
        (Color::Yellow, [128, 128, 0]),
        (Color::Blue, [0, 0, 128]),
        (Color::Magenta, [128, 0, 128]),
        (Color::Cyan, [0, 128, 128]),
        (Color::Gray, [192, 192, 192]),
        (Color::DarkGray, [128, 128, 128]),
        (Color::LightRed, [255, 0, 0]),
        (Color::LightGreen, [0, 255, 0]),
        (Color::LightYellow, [255, 255, 0]),
        (Color::LightBlue, [0, 0, 255]),
        (Color::LightMagenta, [255, 0, 255]),
        (Color::LightCyan, [0, 255, 255]),
        (Color::White, [255, 255, 255]),
    ];
    PALETTE
        .into_iter()
        .min_by_key(|(_, candidate)| color_distance(rgb, *candidate))
        .map_or(Color::Reset, |(color, _)| color)
}

fn color_distance(left: [u8; 3], right: [u8; 3]) -> u32 {
    let red = i32::from(left[0]) - i32::from(right[0]);
    let green = i32::from(left[1]) - i32::from(right[1]);
    let blue = i32::from(left[2]) - i32::from(right[2]);
    u32::try_from(red * red + green * green + blue * blue).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[path = "../tests/gui/terminal_tests.rs"]
mod tests;
