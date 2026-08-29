//! Application-history page render tests: the durable trend column must paint
//! its levels for the terminal's selected glyph repertoire — the Unicode block
//! ramp byte for byte as captured before the paint-time migration, and a
//! monotonic ASCII ink ladder at paint time on ASCII-only terminals, so the
//! post-paint cell rewrite never has to collapse (and lose) the levels.

use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::{ApplicationHistoryMetricSeries, ApplicationHistoryRow};
use taskmanager_core::core::history::ApplicationHistoryIdentity;

use super::*;
use crate::TuiColorMode;
use crate::TuiTerminalProfile;
use crate::ui::test_support::LANG_TEST_GUARD;

// test-intent: behavior
/// One durable CPU series over a uniform 1 s cadence, so
/// `gap_aware_samples` passes the samples through unchanged; `NaN` samples
/// stay explicit recording-downtime gaps. The peak feeds only the text column.
fn cpu_series(samples: &[f32]) -> ApplicationHistoryMetricSeries {
    let times: Vec<u64> = (0..samples.len())
        .map(|index| 1_000 * (index as u64 + 1))
        .collect();
    let peak = samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);
    let peak_measured_at_ms = times.last().copied();
    ApplicationHistoryMetricSeries {
        samples: Arc::from(samples),
        sample_times_ms: Arc::from(times),
        peak_value: if peak.is_finite() {
            Some(f64::from(peak))
        } else {
            None
        },
        peak_measured_at_ms,
        observed: samples.iter().filter(|sample| sample.is_finite()).count(),
        gaps: samples.iter().filter(|sample| !sample.is_finite()).count(),
        clock_jumps: 0,
    }
}

// test-intent: behavior
fn durable_row(name: &str, cpu: Option<ApplicationHistoryMetricSeries>) -> ApplicationHistoryRow {
    let identity = ApplicationHistoryIdentity::unverified_process_name(name)
        .expect("non-empty fixture identity");
    ApplicationHistoryRow {
        identity,
        cpu_usage: cpu,
        memory: None,
        process_count: None,
    }
}

// test-intent: behavior
fn ready_projection(rows: Vec<ApplicationHistoryRow>) -> ApplicationHistoryProjection {
    ApplicationHistoryProjection {
        status: taskmanager_application::ApplicationHistoryStatus::Ready,
        selected_window: HistoryWindow::OneHour,
        rows_window: Some(HistoryWindow::OneHour),
        rows: Arc::from(rows),
        source_request: None,
        refreshing: false,
        failure: None,
        unavailable_reason: None,
        loaded_at_ms: Some(10_000),
    }
}

// test-intent: behavior
/// Resolve a terminal capability profile onto the page's palette.
fn profile(color: TuiColorMode, glyphs: TuiGlyphMode) -> TuiTheme {
    TuiTheme::from_theme_with_profile(
        &taskmanager_theme::Theme::dark(),
        TuiTerminalProfile { color, glyphs },
    )
}

// test-intent: behavior
/// Paint the App-history page body from a hand-built typed projection with the
/// given terminal capability profile. English is pinned under the shared i18n
/// guard so the assertions cannot depend on the host locale.
fn page_frame(theme: TuiTheme, projection: &ApplicationHistoryProjection) -> String {
    let _guard = LANG_TEST_GUARD.lock().expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(140, 48);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            super::render_app_history_projection(frame, theme, frame.area(), projection, 0, None);
        })
        .expect("draw");
    terminal.backend().to_string()
}

// test-intent: behavior
/// The one table row line carrying the fixture application, so assertions can
/// target the trend region instead of the whole frame.
fn trend_row_line(frame: &str, name: &str) -> String {
    frame
        .lines()
        .find(|line| line.contains(name))
        .unwrap_or_else(|| panic!("row {name} must render in the frame"))
        .to_owned()
}

const RISING_EIGHT: [f32; 8] = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];

// ── a) Unicode output stays byte-identical to the pre-migration renderer ────

// test-intent: behavior
/// The Unicode repertoire keeps the pre-migration rendering byte for byte.
/// Every expected string below was captured from the pre-migration
/// `history_trend` implementation before the glyph-mode migration: the same
/// block ramp, the same constant-series mid-ramp, the same space gap inside a
/// live trend, the same bounded recent window, and the same honest collecting
/// text for a window without a finite sample.
#[test]
fn unicode_trend_stays_byte_identical_to_the_pre_migration_ramp() {
    let _guard = LANG_TEST_GUARD.lock().expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &RISING_EIGHT),
        "▁▂▃▄▅▆▇█"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &[5.0, 5.0, 5.0]),
        "▅▅▅",
        "a constant series must stay the mid-ramp flat line"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &[10.0, f32::NAN, 30.0]),
        "▁ █",
        "a downtime gap must stay a space between its levels"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &[20.0, f32::NAN, 40.0, 10.0]),
        "▃ █▁"
    );
    let long: Vec<f32> = (0..30).map(|i| i as f32).collect();
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &long),
        "▁▁▂▂▂▃▃▃▃▄▄▄▅▅▅▆▆▆▆▇▇▇██",
        "the trend must stay bounded to the recent window"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &[f32::NAN, f32::NAN]),
        "Collecting persistent application history",
        "no finite sample must stay the honest collecting text"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Unicode, &[]),
        "Collecting persistent application history"
    );
}

// test-intent: behavior
/// The page under the Unicode profile renders the pinned block ramp in the
/// trend column — byte-identical to the pre-migration page paint.
#[test]
fn unicode_page_frame_renders_the_pinned_block_ramp() {
    let projection = ready_projection(vec![durable_row(
        "org.example.Forest",
        Some(cpu_series(&RISING_EIGHT)),
    )]);
    let frame = page_frame(
        profile(TuiColorMode::TrueColor, TuiGlyphMode::Unicode),
        &projection,
    );
    let line = trend_row_line(&frame, "org.example.Forest");
    assert!(
        line.contains("▁▂▃▄▅▆▇█"),
        "the pinned Unicode ramp must render in the trend column: {line:?}"
    );
}

// ── b) ASCII profile: paint-time ladder, ASCII-only, monotonic, no fallback ─

// test-intent: behavior
/// The ASCII repertoire paints the same normalized levels through the ASCII
/// ink ladder: every trend cell is one ASCII byte, gaps render the underline
/// that sits outside the ladder, and a window without a finite sample keeps
/// the same honest collecting text as Unicode.
#[test]
fn ascii_trend_paints_the_ladder_with_unicode_aligned_semantics() {
    let trend = history_trend_in(TuiGlyphMode::Ascii, &RISING_EIGHT);
    assert_eq!(trend, " .:-=+*#");
    assert!(trend.is_ascii(), "one cell per sample");
    assert_eq!(
        history_trend_in(TuiGlyphMode::Ascii, &[5.0, 5.0, 5.0]),
        "===",
        "a constant series must stay the mid-ladder flat line"
    );
    assert_eq!(
        history_trend_in(TuiGlyphMode::Ascii, &[10.0, f32::NAN, 30.0]),
        " _#",
        "a downtime gap must render the underline outside the ladder"
    );
    let long: Vec<f32> = (0..30).map(|i| i as f32).collect();
    assert_eq!(
        history_trend_in(TuiGlyphMode::Ascii, &long).chars().count(),
        history_trend_in(TuiGlyphMode::Unicode, &long)
            .chars()
            .count(),
        "both repertoires bound the trend to the same recent window"
    );
    let _guard = LANG_TEST_GUARD.lock().expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    assert_eq!(
        history_trend_in(TuiGlyphMode::Ascii, &[f32::NAN, f32::NAN]),
        history_trend_in(TuiGlyphMode::Unicode, &[f32::NAN, f32::NAN]),
        "an all-gap window keeps the Unicode collecting semantics"
    );
}

// test-intent: behavior
/// The ASCII ladder is monotonic in the sample value: strictly rising samples
/// paint strictly stronger characters, and a series with a plateau never
/// paints a weaker character — the level is carried by the glyph itself.
#[test]
fn ascii_ladder_is_monotonic_in_the_sample_value() {
    let ladder_index = |trend: &str| -> Vec<usize> {
        trend
            .chars()
            .map(|c| {
                crate::ui::sparkline::SPARKLINE_ASCII_BLOCKS
                    .iter()
                    .position(|&b| b == c)
                    .expect("ladder char")
            })
            .collect()
    };
    let indices = ladder_index(&history_trend_in(TuiGlyphMode::Ascii, &RISING_EIGHT));
    for pair in indices.windows(2) {
        assert!(
            pair[1] > pair[0],
            "strictly rising values must paint strictly stronger ladder chars"
        );
    }
    // A longer rising series quantizes onto the eight ladder levels, so
    // adjacent steps may share a level — but the level must never weaken.
    let long_rise: Vec<f32> = (0..24).map(|i| i as f32).collect();
    let indices = ladder_index(&history_trend_in(TuiGlyphMode::Ascii, &long_rise));
    for pair in indices.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "a rising series must never paint a weaker ladder char"
        );
    }
    let plateau = ladder_index(&history_trend_in(
        TuiGlyphMode::Ascii,
        &[0.0, 10.0, 10.0, 20.0],
    ));
    for pair in plateau.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "non-decreasing values must never paint a weaker ladder char"
        );
    }
}

// test-intent: behavior
/// Under the ASCII profile the page's trend region is painted through the
/// ladder at paint time: the row line carries the exact ASCII trend, the whole
/// page never shows a fallback glyph, and `:` — a character the post-paint
/// rewrite table can never emit — proves paint-time rendering by construction.
#[test]
fn ascii_page_frame_paints_the_ladder_without_the_fallback_pass() {
    let projection = ready_projection(vec![durable_row(
        "org.example.Forest",
        Some(cpu_series(&RISING_EIGHT)),
    )]);
    let frame = page_frame(
        profile(TuiColorMode::TrueColor, TuiGlyphMode::Ascii),
        &projection,
    );
    let line = trend_row_line(&frame, "org.example.Forest");
    let trend_start = line
        .find(" .:-=+*#")
        .unwrap_or_else(|| panic!("the ASCII ladder must render in the trend column: {line:?}"));
    let region: String = line[trend_start..].chars().take(8).collect();
    assert_eq!(region, " .:-=+*#", "exact pinned ASCII trend region");
    assert!(
        region.is_ascii(),
        "every trend cell must be one ASCII byte: {region:?}"
    );
    assert!(
        !frame.contains('?'),
        "no cell on the page may carry the fallback glyph:\n{frame}"
    );
}

// test-intent: behavior
/// Recording downtime keeps its gap semantics under both repertoires on the
/// painted page: the Unicode space and the ASCII underline sit outside their
/// ramps, and the levels around the gap survive.
#[test]
fn page_frame_keeps_downtime_gaps_outside_both_ramps() {
    let projection = ready_projection(vec![durable_row(
        "org.example.Forest",
        Some(cpu_series(&[10.0, f32::NAN, 30.0])),
    )]);
    let unicode = trend_row_line(
        &page_frame(
            profile(TuiColorMode::TrueColor, TuiGlyphMode::Unicode),
            &projection,
        ),
        "org.example.Forest",
    );
    assert!(unicode.contains("▁ █"), "Unicode gap missing: {unicode:?}");
    let ascii = trend_row_line(
        &page_frame(
            profile(TuiColorMode::TrueColor, TuiGlyphMode::Ascii),
            &projection,
        ),
        "org.example.Forest",
    );
    assert!(ascii.contains(" _#"), "ASCII gap missing: {ascii:?}");
}

// test-intent: behavior
/// A row whose series holds no finite sample renders the honest collecting
/// text in both repertoires — never a fabricated flat trend.
#[test]
fn a_series_without_finite_samples_renders_the_collecting_text_in_both_repertoires() {
    let projection = ready_projection(vec![durable_row(
        "org.example.Forest",
        Some(cpu_series(&[f32::NAN, f32::NAN])),
    )]);
    for glyphs in [TuiGlyphMode::Unicode, TuiGlyphMode::Ascii] {
        let line = trend_row_line(
            &page_frame(profile(TuiColorMode::TrueColor, glyphs), &projection),
            "org.example.Forest",
        );
        assert!(
            line.contains("Collecting persistent"),
            "collecting text missing under {glyphs:?}: {line:?}"
        );
    }
}

// ── c) Monochrome: the level information travels through the characters ─────

// test-intent: behavior
/// On a colorless terminal the trend stays readable through the characters
/// alone: the Monochrome profile renders exactly the same block ramp / ASCII
/// ladder strings as the TrueColor profile, so no level depends on a color cue.
#[test]
fn monochrome_profile_keeps_the_trend_readable_through_glyphs_alone() {
    let projection = ready_projection(vec![durable_row(
        "org.example.Forest",
        Some(cpu_series(&RISING_EIGHT)),
    )]);
    let monochrome_unicode = trend_row_line(
        &page_frame(
            profile(TuiColorMode::Monochrome, TuiGlyphMode::Unicode),
            &projection,
        ),
        "org.example.Forest",
    );
    assert!(
        monochrome_unicode.contains("▁▂▃▄▅▆▇█"),
        "colorless Unicode trend missing: {monochrome_unicode:?}"
    );
    let monochrome_ascii = trend_row_line(
        &page_frame(
            profile(TuiColorMode::Monochrome, TuiGlyphMode::Ascii),
            &projection,
        ),
        "org.example.Forest",
    );
    assert!(
        monochrome_ascii.contains(" .:-=+*#"),
        "colorless ASCII trend missing: {monochrome_ascii:?}"
    );
    let truecolor_ascii = trend_row_line(
        &page_frame(
            profile(TuiColorMode::TrueColor, TuiGlyphMode::Ascii),
            &projection,
        ),
        "org.example.Forest",
    );
    assert_eq!(
        monochrome_ascii, truecolor_ascii,
        "the color mode must not change the painted trend characters"
    );
}
