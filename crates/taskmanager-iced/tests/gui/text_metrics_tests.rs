// test-intent: behavior
//! The measured-text contract for canvas badge geometry: a measured run is
//! finite and non-negative, and more content never measures narrower. Exact
//! pixel values are font-and-host truth and are deliberately not asserted.
//! Measured truncation is additionally exercised with a synthetic width
//! function so the prefix search is deterministic headlessly.

use super::{measured_text_width, truncate_by_measure};

#[test]
fn measured_width_is_finite_and_grows_with_content() {
    let short = measured_text_width("1.5 KiB", 11.0);
    let long = measured_text_width("192.168.1.1 · 12.5 GiB/s", 11.0);

    assert!(
        short.is_finite() && short >= 0.0,
        "a measured run is finite"
    );
    assert!(
        long >= short,
        "more content never measures narrower than less"
    );
}

#[test]
fn an_empty_run_measures_zero_width() {
    assert_eq!(measured_text_width("", 11.0), 0.0);
}

/// A synthetic monospace measure (10px per char) makes the prefix search
/// fully deterministic: the survivor is exactly the longest prefix whose
/// width plus the ellipsis stays inside the budget.
fn each_char_is_10px(text: &str) -> f32 {
    text.chars().count() as f32 * 10.0
}

#[test]
fn text_that_fits_is_returned_unchanged() {
    assert_eq!(truncate_by_measure("abc", 100.0, &each_char_is_10px), "abc");
}

#[test]
fn an_overlong_run_truncates_to_the_longest_fitting_prefix_plus_ellipsis() {
    // budget = 50 - 10(ellipsis) = 40 → four 10px chars survive.
    assert_eq!(
        truncate_by_measure("abcdefghij", 50.0, &each_char_is_10px),
        "abcd…"
    );
}

#[test]
fn a_multibyte_run_never_clips_mid_character() {
    // Each CJK char is 3 bytes but one glyph: the prefix search walks byte
    // indices and must land on boundaries only.
    let truncated = truncate_by_measure("接口if0值", 35.0, &each_char_is_10px);
    assert!(
        truncated.ends_with('…') && truncated.starts_with("接口"),
        "the survivor keeps whole glyphs: {truncated:?}"
    );
}

#[test]
fn nothing_fits_when_even_the_first_glyph_exceeds_the_budget() {
    assert_eq!(truncate_by_measure("value", 5.0, &each_char_is_10px), "");
}
