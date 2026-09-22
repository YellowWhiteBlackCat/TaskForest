//! System-page unit tests (line split).
//!
//! Every helper asserted here is the production builder the page calls; there
//! is no test-tree-local row/model builder. A former `detail_rows` module
//! defined battery and thermal-control row folds that no product code ever
//! rendered (their i18n keys had zero production references); it was deleted
//! rather than kept as a second, unshipped delivery surface. The GPUI battery
//! surface is the Performance page's battery panel (asserted by
//! `perf_views::dynamic_stats::tests`) and the thermal surface is the System
//! Health sensor center (`system_health_view::stats::tests`).

#[cfg(test)]
mod tests_inner {
    // Import ONLY the helpers under test — NOT `use super::*`. The parent module
    // has `use gpui::*;`, whose prelude shadows the built-in `#[test]` attribute
    // macro once re-globbed in here, which trips a recursion-limit error on the
    // attribute itself. Keeping this scope minimal resolves `#[test]` to the
    // std built-in.
    use super::super::{
        fmt_cache_kb, fmt_clock_ghz, fmt_observed_clock_ghz, joined_optional_text, kernel_display,
        optional_text, truncate_cmdline,
    };

    #[test]
    fn cache_readout_distinguishes_unknown_from_measured_zero() {
        assert_eq!(
            fmt_cache_kb(
                None,
                taskmanager_core::core::units::UnitPreferences::default()
            ),
            "—"
        );
        assert_eq!(
            fmt_cache_kb(
                Some(0),
                taskmanager_core::core::units::UnitPreferences::default()
            ),
            "0 B"
        );
        assert_eq!(
            fmt_cache_kb(
                Some(2048),
                taskmanager_core::core::units::UnitPreferences::default()
            ),
            "2.0 MiB"
        );
    }

    #[test]
    fn optional_max_clock_distinguishes_missing_from_measured_zero() {
        assert_eq!(fmt_observed_clock_ghz(None), "—");
        assert_eq!(fmt_observed_clock_ghz(Some(0)), "0.00 GHz");
        assert_eq!(fmt_observed_clock_ghz(Some(4_400)), "4.40 GHz");
    }

    #[test]
    fn optional_base_clock_distinguishes_missing_from_measured_zero() {
        assert_eq!(fmt_clock_ghz(None), "—");
        assert_eq!(fmt_clock_ghz(Some(0)), "0.00 GHz");
        assert_eq!(fmt_clock_ghz(Some(2_400)), "2.40 GHz");
    }

    #[test]
    fn kernel_display_composes_platform_neutral_version_and_build_facts() {
        assert_eq!(
            kernel_display(Some("6.1.5-example"), Some("(builder@host) #1 SMP")),
            "6.1.5-example · (builder@host) #1 SMP"
        );
    }

    #[test]
    fn kernel_display_falls_back_to_version_when_build_is_unavailable() {
        assert_eq!(kernel_display(Some("6.1.0"), None), "6.1.0");
        assert_eq!(kernel_display(Some("6.1.0"), Some("   ")), "6.1.0");
        assert_eq!(kernel_display(None, Some("native build")), "native build");
        assert_eq!(kernel_display(None, None), "—");
        assert_eq!(joined_optional_text(None, None), "—");
    }

    #[test]
    fn unavailable_hardware_identity_renders_as_missing_not_a_platform_fallback() {
        assert_eq!(optional_text(None), "—");
        assert_eq!(optional_text(Some("   ")), "—");
        assert_eq!(
            joined_optional_text(None, Some("ExampleOS 1")),
            "ExampleOS 1"
        );
        assert_eq!(
            joined_optional_text(Some("ExampleOS"), Some("1")),
            "ExampleOS 1"
        );
    }

    #[test]
    fn truncate_cmdline_short_passes_through_unchanged() {
        assert_eq!(truncate_cmdline("quiet"), "quiet");
        assert_eq!(truncate_cmdline(""), "");
        // Exactly MAX (80) chars: boundary — no truncation.
        let exact: String = "a".repeat(80);
        assert_eq!(truncate_cmdline(&exact), exact);
    }

    #[test]
    fn truncate_cmdline_long_gets_ellipsis_suffix() {
        // 81 chars (>MAX): truncate to 79 chars + "…" = 80 chars total.
        let long: String = "a".repeat(81);
        let t = truncate_cmdline(&long);
        assert_eq!(t.chars().count(), 80);
        assert!(t.ends_with('…'));
        // The first 79 source chars are preserved.
        assert_eq!(t.chars().filter(|c| *c == 'a').count(), 79);
    }

    #[test]
    fn truncate_cmdline_counts_chars_not_bytes() {
        // Non-ASCII: 2 emoji (8 bytes) + 78 ASCII = 80 chars → unchanged.
        let s: String = "😀😀".to_string() + &"a".repeat(78);
        assert_eq!(truncate_cmdline(&s), s);
        // 3 emoji + 78 ASCII = 81 chars → truncate to 80 chars (79 source + "…"),
        // no codepoint split.
        let s: String = "😀😀😀".to_string() + &"a".repeat(78);
        let t = truncate_cmdline(&s);
        assert_eq!(t.chars().count(), 80);
        assert!(t.ends_with('…'));
        // The 2 leading emoji are intact (no split mid-codepoint).
        assert!(t.starts_with("😀😀"));
    }
}
