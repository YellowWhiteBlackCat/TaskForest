//! Inline test modules for the app-adaptation element helpers. Split out of
//! `elements.rs` only to satisfy the source-line guard; the three groups pin
//! the theme-token invariants behind `titlebar_border` / `card_shadow` and the
//! shared search-highlight matcher parity (ADR-020). `super::` resolves back
//! into the parent `elements` module where the helpers live.

// Re-export every helper from the parent `elements` module so the nested
// `#[cfg(test)]` sub-modules below keep their verbatim `use super::<helper>;`
// imports resolving one level deeper than they did inline in `elements.rs`.
use super::*;

#[cfg(test)]
mod titlebar_border_tests {

    use super::titlebar_border;
    use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

    /// The active-window look must stay byte-identical to the theme token —
    /// every existing screenshot baseline depends on it. Exercised across all
    /// 8 skins × light/dark × contrast variants.
    #[test]
    fn active_preserves_theme_border_token() {
        for skin in Skin::ALL {
            for mode in [LightDark::Light, LightDark::Dark] {
                for contrast in [HighContrast::Off, HighContrast::On] {
                    let theme = Theme::build(skin, mode, contrast, ResolvedFonts::system_for(skin));
                    assert_eq!(
                        titlebar_border(&theme, true),
                        taskmanager_ui::theme_binding::rgba(theme.border)
                    );
                }
            }
        }
    }

    /// Inactive windows dim the border to 60% alpha: same RGB hue, weaker
    /// alpha, and strictly weaker than the active token (border tokens are
    /// opaque, so the comparison is meaningful for every variant).
    #[test]
    fn inactive_dims_border_alpha_keeping_hue() {
        for skin in Skin::ALL {
            for mode in [LightDark::Light, LightDark::Dark] {
                for contrast in [HighContrast::Off, HighContrast::On] {
                    let theme = Theme::build(skin, mode, contrast, ResolvedFonts::system_for(skin));
                    let dim = titlebar_border(&theme, false);
                    let expected = taskmanager_ui::theme_binding::rgba(
                        theme.border.with_alpha(theme.border.a * 0.6),
                    );
                    assert_eq!(dim, expected);
                    assert_eq!(
                        (dim.r, dim.g, dim.b),
                        (theme.border.r, theme.border.g, theme.border.b),
                        "inactive border must keep the theme hue"
                    );
                    assert!(
                        dim.a < theme.border.a,
                        "inactive border must be strictly weaker than the active token"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod card_shadow_tests {
    use super::card_shadow;
    use crate::gpui_app::elements::CARD_SHADOW_AMBIENT_ALPHA;
    use gpui::Hsla;
    use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

    /// The card shadow helper returns the Mission Center two-layer pair in
    /// the theme's own ink: an ambient layer that is wider AND weaker than
    /// the edge layer, and the edge layer carrying the token color verbatim
    /// (both layers translucent in every skin × mode).
    #[test]
    fn card_shadow_is_two_layer_token_ink() {
        for skin in Skin::ALL {
            for mode in [LightDark::Light, LightDark::Dark] {
                let theme = Theme::build(
                    skin,
                    mode,
                    HighContrast::Off,
                    ResolvedFonts::system_for(skin),
                );
                let shadow = card_shadow(&theme);
                assert_eq!(
                    shadow.len(),
                    2,
                    "{} {} must cast two layers",
                    skin.label(),
                    mode.label()
                );
                let ink: Hsla = taskmanager_ui::theme_binding::hsla(theme.card_shadow());
                let ambient: Hsla = taskmanager_ui::theme_binding::hsla(
                    theme
                        .card_shadow()
                        .with_alpha(theme.card_shadow().a * CARD_SHADOW_AMBIENT_ALPHA),
                );
                assert_eq!(
                    shadow[1].color,
                    ink,
                    "{} {} edge layer must carry the token ink",
                    skin.label(),
                    mode.label(),
                );
                assert_eq!(
                    shadow[0].color,
                    ambient,
                    "{} {} ambient layer must scale the ink alpha",
                    skin.label(),
                    mode.label(),
                );
                assert!(
                    shadow[0].blur_radius > shadow[1].blur_radius,
                    "{} {} ambient layer must blur wider",
                    skin.label(),
                    mode.label(),
                );
                assert!(
                    shadow[0].offset.y > shadow[1].offset.y,
                    "{} {} ambient layer must drop further",
                    skin.label(),
                    mode.label(),
                );
            }
        }
    }

    /// 稳固效果 ceiling (2026-08 owner directive "很深的 blur 效果很糟糕"):
    /// the ambient layer must stay a WHISPER of lift, not a halo. The
    /// pre-change y4/blur16@60%-ink treatment is the ceiling this test keeps
    /// from ever coming back — separation comes from the tone ladder.
    #[test]
    fn card_shadow_stays_under_the_flatten_policy_ceiling() {
        let theme = Theme::build(
            Skin::Gnome,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Gnome),
        );
        let ambient = &card_shadow(&theme)[0];
        assert!(
            f32::from(ambient.blur_radius) <= 8.0,
            "ambient blur must stay subtle, got {}",
            ambient.blur_radius
        );
        assert!(f32::from(ambient.offset.y) <= 3.0);
        let ink: Hsla = taskmanager_ui::theme_binding::hsla(theme.card_shadow());
        let painted: Hsla = ambient.color;
        assert!(
            painted.a <= ink.a * 0.4,
            "ambient alpha must stay ≤ 40% of the ink"
        );
    }

    /// The shadow ink adapts to light/dark: dark skins cast pure translucent
    /// black, light skins a darkened-shade grey — so the pair never looks
    /// identical across modes (the helper reads the theme token, not a
    /// hardcoded color).
    #[test]
    fn card_shadow_adapts_to_mode_through_the_theme_token() {
        let dark = Theme::build(
            Skin::Windows,
            LightDark::Dark,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Windows),
        );
        let light = Theme::build(
            Skin::Windows,
            LightDark::Light,
            HighContrast::Off,
            ResolvedFonts::system_for(Skin::Windows),
        );
        let dark_shadow = card_shadow(&dark);
        let light_shadow = card_shadow(&light);
        assert_ne!(
            dark_shadow[1].color, light_shadow[1].color,
            "light and dark cards must cast different shadow ink"
        );
        let black_ink: Hsla = taskmanager_ui::theme_binding::hsla(Theme::dark().card_shadow());
        assert_eq!(
            dark_shadow[1].color, black_ink,
            "dark skins cast the locked black ink"
        );
    }
}

#[cfg(test)]
mod highlighted_text_tests {
    use std::ops::Range;

    use taskmanager_core::core::text::match_ranges_ascii_ci;
    use taskmanager_ui::data::highlighter::find_matches;

    /// The GPUI search-highlight geometry now comes from the shared
    /// `match_ranges_ascii_ci` engine (ADR-020); the legacy `find_matches`
    /// remains only as the old reference. Both must agree — byte ranges,
    /// non-overlapping, case-insensitive — for every category the legacy
    /// engine was relied on for: ASCII case folding, multi-segment (repeated /
    /// adjacent) matches, no-match haystacks, and multi-byte (Chinese) text
    /// whose ranges never split a character.
    #[test]
    fn shared_and_legacy_matchers_produce_identical_ranges() {
        let cases: [(&str, &str); 9] = [
            // Case-insensitive ASCII folding.
            ("Firefox", "fire"),
            ("firefox", "FIRE"),
            ("Hello hello HELLO", "hello"),
            // Multi-segment / adjacent non-overlapping matches.
            ("abcABCabc", "abc"),
            ("aaaa", "aa"),
            // No match (including a needle longer than the haystack).
            ("systemd-resolved", "plasma"),
            ("short", "much longer needle"),
            // Multi-byte (Chinese) text: byte-aligned, never split.
            ("中文搜索中文", "中文"),
            ("Réseau", "seau"),
        ];
        for (text, query) in cases {
            let legacy: Vec<Range<usize>> = find_matches(text, query, false)
                .into_iter()
                .map(|m| m.range)
                .collect();
            let shared = match_ranges_ascii_ci(text, query);
            assert_eq!(
                shared, legacy,
                "shared matcher diverged from the legacy engine for ({text:?}, {query:?})"
            );
            for range in &shared {
                assert!(text.is_char_boundary(range.start));
                assert!(text.is_char_boundary(range.end));
            }
        }
    }

    /// The highlighted element must consume the whole haystack exactly once:
    /// non-overlapping highlighted segments plus the gaps between them
    /// reconstruct the original text (no dropped or duplicated bytes).
    #[test]
    fn match_ranges_cover_the_haystack_without_overlap() {
        for (text, query) in [
            ("aaaa", "aa"),
            ("abcABCabc", "abc"),
            ("中文搜索中文", "中文"),
        ] {
            let matches = match_ranges_ascii_ci(text, query);
            let mut cursor = 0usize;
            for range in &matches {
                assert!(
                    range.start >= cursor,
                    "segments must never overlap: {range:?} after {cursor}"
                );
                cursor = range.end;
            }
            assert!(cursor <= text.len());
        }
    }
}

#[cfg(test)]
mod more_rows_hint_tests {
    use super::more_rows_label;
    use taskmanager_application::i18n::{self, Language};

    /// The bounded-list hint must carry the real hidden count into the
    /// localized template, in both shipped locales (pin + restore the global
    /// language around each read, matching `system_about`'s locale tests).
    #[test]
    fn more_rows_label_substitutes_the_hidden_count_in_both_locales() {
        let prior = i18n::current_language();

        i18n::set_language(Language::En);
        let en = more_rows_label(804);
        assert!(
            en.contains("804") && en.contains("more"),
            "English hint must carry the count, got: {en}"
        );

        i18n::set_language(Language::Zh);
        let zh = more_rows_label(12);
        assert!(
            zh.contains("12") && zh.contains("另"),
            "Chinese hint must carry the count, got: {zh}"
        );

        i18n::set_language(prior);
        assert!(
            !more_rows_label(0).contains("{count}"),
            "the placeholder must always be substituted"
        );
    }
}
