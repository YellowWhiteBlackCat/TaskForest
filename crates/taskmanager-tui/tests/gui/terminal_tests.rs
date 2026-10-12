use super::*;
use crate::terminal::TuiTerminalSignals;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use taskmanager_application::i18n::{Language, current_language, set_language};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_ui_contract::IconId;

#[test]
fn terminal_signals_resolve_explicit_color_and_glyph_capabilities() {
    assert_eq!(
        TuiTerminalProfile::from_signals(
            Some("xterm-256color"),
            None,
            false,
            None,
            Some("C.UTF-8"),
            Language::En,
        ),
        TuiTerminalProfile {
            color: TuiColorMode::Ansi256,
            glyphs: TuiGlyphMode::Unicode,
        }
    );
    assert_eq!(
        TuiTerminalProfile::from_signals(
            Some("xterm-256color"),
            Some("truecolor"),
            false,
            Some("ascii"),
            Some("C.UTF-8"),
            Language::En,
        ),
        TuiTerminalProfile {
            color: TuiColorMode::TrueColor,
            glyphs: TuiGlyphMode::Ascii,
        }
    );
}

#[test]
fn dumb_and_no_color_signals_fail_closed() {
    let dumb = TuiTerminalProfile::from_signals(
        Some("dumb"),
        Some("truecolor"),
        false,
        None,
        Some("C.UTF-8"),
        Language::En,
    );
    assert_eq!(dumb.color, TuiColorMode::Monochrome);
    assert_eq!(dumb.glyphs, TuiGlyphMode::Ascii);
    assert_eq!(dumb.color(Color::Rgb(12, 34, 56)), Color::Reset);

    let no_color = TuiTerminalProfile::from_signals(
        Some("xterm-256color"),
        Some("truecolor"),
        true,
        Some("unicode"),
        Some("C.UTF-8"),
        Language::En,
    );
    assert_eq!(no_color.color, TuiColorMode::Monochrome);
    assert_eq!(no_color.glyphs, TuiGlyphMode::Unicode);
    assert_eq!(no_color.color(Color::White), Color::Reset);
}

#[test]
fn zh_glyph_floor_preserves_terminal_and_explicit_override_precedence() {
    let cases = [
        (
            Some("xterm-256color"),
            None,
            Some("C"),
            None,
            TuiGlyphMode::Unicode,
            "Zh overrides a non-UTF-8 locale",
        ),
        (
            Some("xterm-256color"),
            None,
            None,
            None,
            TuiGlyphMode::Unicode,
            "Zh is a floor when locale is missing",
        ),
        (
            Some("dumb"),
            None,
            Some("zh_CN.UTF-8"),
            None,
            TuiGlyphMode::Ascii,
            "TERM=dumb stays ASCII under Zh",
        ),
        (
            Some("xterm-256color"),
            None,
            Some("C"),
            Some("ascii"),
            TuiGlyphMode::Ascii,
            "explicit ASCII override wins under Zh",
        ),
        (
            Some("dumb"),
            None,
            Some("C"),
            Some("unicode"),
            TuiGlyphMode::Unicode,
            "explicit Unicode override remains highest priority",
        ),
    ];

    for (term, color_term, locale, glyph_override, expected, reason) in cases {
        let profile = TuiTerminalProfile::from_signals(
            term,
            color_term,
            false,
            glyph_override,
            locale,
            Language::Zh,
        );
        assert_eq!(profile.glyphs, expected, "{reason}");
    }
}

#[test]
fn captured_terminal_signals_follow_live_ui_language_changes() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("language guard");
    let signals = TuiTerminalSignals {
        term: Some(String::from("xterm-256color")),
        color_term: None,
        no_color: false,
        glyph_override: None,
        locale: Some(String::from("C")),
    };

    set_language(Language::En);
    assert_eq!(
        signals.resolve(current_language()).glyphs,
        TuiGlyphMode::Ascii
    );
    set_language(Language::Zh);
    assert_eq!(
        signals.resolve(current_language()).glyphs,
        TuiGlyphMode::Unicode
    );
    set_language(Language::En);
    assert_eq!(
        signals.resolve(current_language()).glyphs,
        TuiGlyphMode::Ascii
    );
}

#[test]
fn ascii_icon_fallback_preserves_semantic_difference() {
    let profile = TuiTerminalProfile {
        color: TuiColorMode::Ansi16,
        glyphs: TuiGlyphMode::Ascii,
    };
    assert_eq!(profile.glyph(IconId::Cpu), "C");
    assert_eq!(profile.glyph(IconId::Network), "<>");
    assert_eq!(profile.glyph(IconId::CircleCheck), "+");
    assert_eq!(profile.glyph(IconId::CircleX), "x");
    assert_ne!(profile.glyph(IconId::Cpu), profile.glyph(IconId::Memory));
}

#[test]
fn color_profiles_emit_only_their_declared_palette() {
    let rgb = Color::Rgb(53, 132, 228);
    let truecolor = TuiTerminalProfile::default();
    assert_eq!(truecolor.color(rgb), rgb);

    let ansi256 = TuiTerminalProfile {
        color: TuiColorMode::Ansi256,
        glyphs: TuiGlyphMode::Unicode,
    };
    assert!(matches!(ansi256.color(rgb), Color::Indexed(_)));

    let ansi16 = TuiTerminalProfile {
        color: TuiColorMode::Ansi16,
        glyphs: TuiGlyphMode::Unicode,
    };
    assert!(matches!(
        ansi16.color(rgb),
        Color::Black
            | Color::Red
            | Color::Green
            | Color::Yellow
            | Color::Blue
            | Color::Magenta
            | Color::Cyan
            | Color::Gray
            | Color::DarkGray
            | Color::LightRed
            | Color::LightGreen
            | Color::LightYellow
            | Color::LightBlue
            | Color::LightMagenta
            | Color::LightCyan
            | Color::White
    ));
}

#[test]
fn monochrome_theme_routes_the_rendered_frame_to_reset_colors() {
    let profile = TuiTerminalProfile {
        color: TuiColorMode::Monochrome,
        glyphs: TuiGlyphMode::Ascii,
    };
    let mut app = crate::demo_app();
    for page in AppPage::ALL {
        let _ = app.apply_action(AppAction::SelectPage(page));
        assert_ascii_frame(&app, profile, page);
    }

    app.toggle_settings();
    assert_ascii_frame(&app, profile, AppPage::Performance);
}

fn assert_ascii_frame(app: &crate::TuiApp, profile: TuiTerminalProfile, page: AppPage) {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    let theme = crate::TuiTheme::from_params_with_profile(app.theme_params, profile);
    terminal
        .draw(|frame| crate::render(frame, app, theme))
        .expect("render");

    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset),
        "{page:?}: no-color must not leak a hard-coded foreground/background"
    );
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.symbol().is_ascii()),
        "{page:?}: the ASCII profile must leave an ASCII-only terminal buffer"
    );
    assert!(
        !terminal.backend().to_string().contains("◇"),
        "{page:?}: the ASCII profile must replace semantic Unicode icons"
    );
}
