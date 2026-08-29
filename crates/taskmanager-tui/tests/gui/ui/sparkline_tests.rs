use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::test_support::ascii_device_trend;
use super::test_support::device_trend;
use super::*;
use crate::{TuiColorMode, TuiTerminalProfile};

/// A flat series renders as a constant mid-ramp line — honest about the
/// trend being flat, never a panic on a zero range. `(0.5 * 7.0).round()`
/// is 4, so the flat ramp is the index-4 block ('▅').
#[test]
fn flat_series_renders_a_constant_mid_ramp_line() {
    let spark = sparkline_in(TuiGlyphMode::Unicode, &[5.0, 5.0, 5.0]);
    assert!(spark.chars().all(|c| c == '▅'), "flat → mid ramp: {spark}");
    assert_eq!(spark.chars().count(), 3);
}

/// A rising series renders ascending ramp blocks so the trend SHAPE is
/// preserved even though the absolute values are normalized away.
#[test]
fn rising_series_renders_ascending_ramp_blocks() {
    let spark = sparkline_in(
        TuiGlyphMode::Unicode,
        &[10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0],
    );
    let indices: Vec<usize> = spark
        .chars()
        .map(|c| {
            SPARKLINE_BLOCKS
                .iter()
                .position(|&b| b == c)
                .expect("ramp block")
        })
        .collect();
    for pair in indices.windows(2) {
        assert!(
            pair[1] > pair[0],
            "strictly rising values must map to strictly rising blocks"
        );
    }
}

/// An empty window renders an empty trend, and the bounded window keeps
/// long rings at a stable width.
#[test]
fn empty_series_renders_empty_and_window_stays_bounded() {
    assert_eq!(sparkline_in(TuiGlyphMode::Unicode, &[]), "");
    let long: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let windowed = recent_window(&long);
    assert_eq!(windowed.len(), SPARKLINE_MAX_SAMPLES);
    assert_eq!(
        sparkline_in(TuiGlyphMode::Unicode, windowed)
            .chars()
            .count(),
        SPARKLINE_MAX_SAMPLES
    );
}

/// A per-device window with fewer than two samples renders the dotted
/// placeholder — a single sample cannot show a SHAPE, and rendering it as a
/// one-block line would read as a fabricated trend.
#[test]
fn device_trend_renders_the_placeholder_below_two_samples() {
    assert_eq!(device_trend(&[]), DEVICE_TREND_PLACEHOLDER);
    assert_eq!(device_trend(&[42.0]), DEVICE_TREND_PLACEHOLDER);
}

/// A per-device window with two or more samples renders a real sparkline
/// made of ramp blocks (never the placeholder mid-dots).
#[test]
fn device_trend_renders_ramp_blocks_at_two_or_more_samples() {
    let trend = device_trend(&[10.0, 20.0, 30.0]);
    assert!(!trend.is_empty());
    assert_ne!(trend, DEVICE_TREND_PLACEHOLDER);
    for c in trend.chars() {
        assert!(
            SPARKLINE_BLOCKS.contains(&c),
            "non-ramp char {c:?} in trend {trend:?}"
        );
    }
}

/// The per-device trend is bounded to the recent window so a full 64-sample
/// ring renders a stable-width sparkline rather than the whole window.
#[test]
fn device_trend_bounds_a_full_ring_to_the_recent_window() {
    let many: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let trend = device_trend(&many);
    assert_eq!(
        trend.chars().count(),
        SPARKLINE_MAX_SAMPLES,
        "trend must be bounded, got {trend:?}"
    );
}

/// The per-process CPU trend shares the per-device finite-sample gate and
/// bounded window: <2 samples renders the dotted placeholder, ≥2 renders a
/// real sparkline whose width never exceeds SPARKLINE_MAX_SAMPLES. This is
/// the parity seam the Applications-table per-row sparkline consumes.
#[test]
fn process_cpu_trend_mirrors_the_device_finite_sample_gate() {
    // Cold-start (<2 samples): the dotted placeholder, never a fabricated
    // single-block line.
    assert_eq!(process_cpu_trend(&[]), DEVICE_TREND_PLACEHOLDER);
    assert_eq!(process_cpu_trend(&[42.0]), DEVICE_TREND_PLACEHOLDER);

    // ≥2 samples: a real sparkline made of ramp blocks, never the
    // placeholder mid-dots.
    let trend = process_cpu_trend(&[10.0, 20.0, 30.0]);
    assert!(!trend.is_empty());
    assert_ne!(trend, DEVICE_TREND_PLACEHOLDER);
    for c in trend.chars() {
        assert!(
            SPARKLINE_BLOCKS.contains(&c),
            "non-ramp char {c:?} in trend {trend:?}"
        );
    }

    // A full 64-sample ring is bounded to the recent window so the trend
    // column stays a stable width as the ring fills.
    let many: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let trend = process_cpu_trend(&many);
    assert_eq!(
        trend.chars().count(),
        SPARKLINE_MAX_SAMPLES,
        "trend must be bounded, got {trend:?}"
    );
}

#[test]
fn device_summary_uses_latest_average_and_peak_without_gap_fabrication() {
    let line = device_summary_line("GPU", &[10.0, f32::NAN, 30.0], DeviceSummaryUnit::Percent)
        .expect("finite samples produce a summary");
    assert!(line.contains("30%"), "latest/peak missing: {line}");
    assert!(line.contains("20%"), "average missing: {line}");
    assert_eq!(
        device_summary_line("GPU", &[f32::NAN], DeviceSummaryUnit::Percent),
        None
    );
    assert!(
        device_summary_line("Disk", &[1_048_576.0], DeviceSummaryUnit::BytesPerSecond).is_some(),
        "a single real rate sample remains visible"
    );
}

/// Device summaries render the unit suffixes used by the remaining
/// power/temperature history consumers.
#[test]
fn device_series_units_format_watts_and_celsius() {
    let watts = device_summary_line("GPU", &[8.4, 9.2], DeviceSummaryUnit::Watts)
        .expect("finite watts window");
    assert!(
        watts.contains("9.2 W") && watts.contains("8.8 W"),
        "watts summary missing: {watts}"
    );
    let celsius = device_summary_line("GPU", &[63.0, 65.0], DeviceSummaryUnit::Celsius)
        .expect("finite temperature window");
    assert!(
        celsius.contains("65°C") && celsius.contains("64°C"),
        "celsius summary missing: {celsius}"
    );
}

// ── Glyph-mode-aware summary lines (`device_summary_line_in`) ───────────────

// test-intent: behavior
/// Every `DeviceSummaryUnit` in profile order, each with a fixture window
/// whose Latest/Avg/Peak reduction is exact in `f32`, plus the exact expected
/// summary line per repertoire. Full-line snapshots pin the composition so
/// any drift (separator, suffix, or statistic order) is caught byte for byte.
fn summary_fixture(
    unit: DeviceSummaryUnit,
) -> (&'static str, Vec<f32>, &'static str, &'static str) {
    match unit {
        DeviceSummaryUnit::Percent => (
            "GPU",
            vec![10.0, 30.0],
            "GPU · Latest 30% · Avg 20% · Peak 30%",
            "GPU | Latest 30% | Avg 20% | Peak 30%",
        ),
        DeviceSummaryUnit::BytesPerSecond => (
            "Disk",
            vec![1_048_576.0],
            "Disk · Latest 1.0 MiB/s · Avg 1.0 MiB/s · Peak 1.0 MiB/s",
            "Disk | Latest 1.0 MiB/s | Avg 1.0 MiB/s | Peak 1.0 MiB/s",
        ),
        DeviceSummaryUnit::Rpm => (
            "FAN",
            vec![1200.0, 1400.0],
            "FAN · Latest 1400 RPM · Avg 1300 RPM · Peak 1400 RPM",
            "FAN | Latest 1400 RPM | Avg 1300 RPM | Peak 1400 RPM",
        ),
        DeviceSummaryUnit::Watts => (
            "GPU",
            vec![8.4, 9.2],
            "GPU · Latest 9.2 W · Avg 8.8 W · Peak 9.2 W",
            "GPU | Latest 9.2 W | Avg 8.8 W | Peak 9.2 W",
        ),
        DeviceSummaryUnit::Celsius => (
            "GPU",
            vec![63.0, 65.0],
            "GPU · Latest 65°C · Avg 64°C · Peak 65°C",
            "GPU | Latest 65C | Avg 64C | Peak 65C",
        ),
    }
}

/// Pin the English labels (`common.latest` / `common.avg` / `common.peak`)
/// and serialize against the language-flipping i18n test, which owns the same
/// process-global guard.
fn guarded_english_summary(
    mode: TuiGlyphMode,
    label: &str,
    samples: &[f32],
    unit: DeviceSummaryUnit,
) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    device_summary_line_in(mode, label, samples, unit).expect("finite fixture window")
}

// test-intent: behavior
/// Under the Unicode repertoire every unit's summary line is byte-identical
/// to the historical mid-dot output: wiring the summary to the terminal
/// profile cannot shift what a Unicode terminal already renders.
#[test]
fn unicode_summary_lines_stay_byte_identical_per_unit() {
    for unit in [
        DeviceSummaryUnit::Percent,
        DeviceSummaryUnit::BytesPerSecond,
        DeviceSummaryUnit::Rpm,
        DeviceSummaryUnit::Watts,
        DeviceSummaryUnit::Celsius,
    ] {
        let (label, samples, expected, _) = summary_fixture(unit);
        let line = guarded_english_summary(TuiGlyphMode::Unicode, label, &samples, unit);
        assert_eq!(line, expected, "unicode summary drifted for {unit:?}");
    }
}

// test-intent: behavior
/// Under the ASCII repertoire every unit's summary line paints plain-ASCII
/// separators and a degree-free Celsius unit at paint time, so no `°` is ever
/// left for the post-paint cell pass to collapse into `?` — the line is fully
/// ASCII, separator-visible, and carries all three statistics unchanged.
#[test]
fn ascii_summary_lines_paint_plain_ascii_separators_and_units() {
    for unit in [
        DeviceSummaryUnit::Percent,
        DeviceSummaryUnit::BytesPerSecond,
        DeviceSummaryUnit::Rpm,
        DeviceSummaryUnit::Watts,
        DeviceSummaryUnit::Celsius,
    ] {
        let (label, samples, _, expected) = summary_fixture(unit);
        let line = guarded_english_summary(TuiGlyphMode::Ascii, label, &samples, unit);
        assert_eq!(line, expected, "ascii summary drifted for {unit:?}");
        assert!(
            line.is_ascii(),
            "ascii summary must be pure ASCII: {line:?}"
        );
        assert!(!line.contains('°'), "degree sign leaked: {line:?}");
        assert!(!line.contains('?'), "fallback glyph leaked: {line:?}");
        assert!(line.contains(" | "), "ascii separator missing: {line:?}");
        assert!(
            line.contains("Latest") && line.contains("Avg") && line.contains("Peak"),
            "statistics lost in ascii mode: {line:?}"
        );
    }
}

// test-intent: behavior
/// A rate sample that cannot form a byte count (negative) never collapses
/// into a fabricated zero or a `?`: the summary keeps the shared em-dash
/// placeholder, which the audited ASCII cell table maps to `-` without any
/// information-inventing rewrite.
#[test]
fn byte_rate_summary_keeps_the_shared_missing_value_placeholder() {
    let line = guarded_english_summary(
        TuiGlyphMode::Unicode,
        "Disk",
        &[-512.0],
        DeviceSummaryUnit::BytesPerSecond,
    );
    assert!(
        line.contains(missing_value().as_str()),
        "unavailable rate must render the shared placeholder: {line:?}"
    );
    assert!(!line.contains('0'), "no fabricated zero: {line:?}");
}

// Dual-direction rows (disk read/write, NIC rx/tx) — the split-series trend.

// test-intent: behavior
/// Both directions normalize against ONE shared min/max, so a constant
/// direction pinned at the pair's maximum renders the top block — not the
/// mid-ramp its own-only normalization would paint. The shared scale is what
/// keeps the two rows comparable in amplitude (the iced two-series chart's
/// contract in terminal form).
#[test]
fn dual_trend_rows_share_one_scale_across_directions() {
    let trend = device_dual_trend_with(&[10.0, 20.0, 30.0], &[30.0, 30.0, 30.0], 24);
    assert_eq!(
        trend.primary, "▁▅█",
        "rising direction spans the shared ramp"
    );
    assert_eq!(
        trend.secondary, "███",
        "max-pinned constant rides the top block"
    );
}

// test-intent: behavior
/// A `NaN` inside an otherwise live direction renders the gap glyph — the
/// same mid-dot family as the cold-start placeholder, distinct from every
/// ramp block — so an explicit missing sample never reads as a fabricated
/// drop to the baseline block.
#[test]
fn dual_trend_renders_nan_as_a_gap_glyph_not_a_baseline_block() {
    let trend = device_dual_trend_with(&[10.0, f32::NAN, 30.0], &[20.0, 20.0, 20.0], 24);
    assert_eq!(trend.primary, "▁·█");
    assert_eq!(trend.secondary, "▅▅▅");
}

// test-intent: behavior
/// Each direction gates independently on its own finite samples: a direction
/// still collecting (fewer than two finite samples) renders the dotted
/// placeholder while its companion plots a real ramp, and a never-seen
/// device keeps BOTH rows on the placeholder — one missing direction never
/// blanks or fabricates the other.
#[test]
fn dual_trend_directions_gate_independently_on_finite_samples() {
    let partial = device_dual_trend_with(&[f32::NAN, 42.0], &[10.0, 20.0], 24);
    assert_eq!(partial.primary, DEVICE_TREND_PLACEHOLDER);
    for c in partial.secondary.chars() {
        assert!(
            SPARKLINE_BLOCKS.contains(&c),
            "non-ramp char {c:?} in live companion row"
        );
    }
    let cold = device_dual_trend_with(&[], &[], 24);
    assert_eq!(cold.primary, DEVICE_TREND_PLACEHOLDER);
    assert_eq!(cold.secondary, DEVICE_TREND_PLACEHOLDER);
}

// test-intent: behavior
/// Both rows are bounded to the same explicit window so the pair keeps a
/// stable width as the device rings fill.
#[test]
fn dual_trend_bounds_each_row_to_the_window() {
    let rising: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let falling: Vec<f32> = (0..128).map(|i| 127.0 - i as f32).collect();
    let trend = device_dual_trend_with(&rising, &falling, 24);
    assert_eq!(trend.primary.chars().count(), 24);
    assert_eq!(trend.secondary.chars().count(), 24);
}

// test-intent: behavior
/// The two direction rows pad their labels to the pair's common width so the
/// sparklines start at the same column, and each row carries its own trend
/// string after the label.
#[test]
fn dual_trend_line_pads_labels_to_a_common_start_column() {
    let receive = dual_trend_line("Receive", 7, "▁▅█", Style::new());
    let send = dual_trend_line("Send", 7, "███", Style::new());
    let receive_text: String = receive.spans.iter().map(|s| s.content.as_ref()).collect();
    let send_text: String = send.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(
        receive_text.find('▁'),
        send_text.find('█'),
        "both trends must start at the same column:\n{receive_text:?}\n{send_text:?}"
    );
    assert!(receive_text.starts_with("  Receive "), "{receive_text:?}");
    assert!(send_text.starts_with("  Send    "), "{send_text:?}");
}

// ── Paint-time ASCII degradation (`TuiGlyphMode::Ascii`) ────────────────────

// test-intent: behavior
/// An eight-step rising fixture shared by the ASCII-mode tests.
fn rising_eight() -> [f32; 8] {
    [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0]
}

// test-intent: behavior
/// Explicit Unicode selection renders the historical block ramp byte for
/// byte: wiring a caller to the profile cannot shift what a Unicode terminal
/// already renders — ramp blocks, mid-dot gaps and the dotted placeholder all
/// stay exactly as before.
#[test]
fn unicode_mode_output_stays_byte_identical_to_the_block_ramp() {
    assert_eq!(
        sparkline_in(TuiGlyphMode::Unicode, &rising_eight()),
        "▁▂▃▄▅▆▇█"
    );
    assert_eq!(device_trend_in(TuiGlyphMode::Unicode, &[42.0], 24), "····");
    assert_eq!(
        device_trend_in(TuiGlyphMode::Unicode, &[42.0], 24),
        DEVICE_TREND_PLACEHOLDER
    );
    let dual = device_dual_trend_in(
        TuiGlyphMode::Unicode,
        &[10.0, f32::NAN, 30.0],
        &[20.0, 20.0, 20.0],
        24,
    );
    assert_eq!(
        dual,
        device_dual_trend_with(&[10.0, f32::NAN, 30.0], &[20.0, 20.0, 20.0], 24)
    );
    assert_eq!(dual.primary, "▁·█");
    assert_eq!(dual.secondary, "▅▅▅");
}

// test-intent: behavior
/// Under the ASCII repertoire the ramp is painted at paint time, not recovered
/// by the frame-level cell rewrite. The trend is rendered straight into a
/// TestBackend frame that the sanitize pass never visits, so the buffer shows
/// exactly the ladder — including `':'`, a character the cell-level fallback
/// table can never emit, which rules out a post-paint rewrite by
/// construction.
#[test]
fn ascii_profile_paints_ladder_cells_directly_without_the_fallback_pass() {
    let trend = sparkline_in(TuiGlyphMode::Ascii, &rising_eight());
    assert_eq!(trend, " .:-=+*#");
    assert!(!trend.contains('?'), "no fallback glyph: {trend:?}");

    let backend = TestBackend::new(16, 3);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let area = Rect::new(0, 1, 8, 1);
    terminal
        .draw(|frame| frame.render_widget(ratatui::text::Line::from(trend.clone()), area))
        .expect("draw");
    let expected = concat!(
        "\"                \"\n",
        "\" .:-=+*#        \"\n",
        "\"                \"\n",
    );
    assert_eq!(terminal.backend().to_string(), expected);
}

// test-intent: behavior
/// The ASCII ladder is monotonic in the sample value: strictly rising samples
/// paint strictly stronger characters, and a rising series with a plateau
/// never paints a weaker character — the level is carried by the glyph, so
/// the shape survives without any color cue.
#[test]
fn ascii_ladder_levels_are_monotonic_in_the_sample_value() {
    let ladder_index = |spark: &str| -> Vec<usize> {
        spark
            .chars()
            .map(|c| {
                SPARKLINE_ASCII_BLOCKS
                    .iter()
                    .position(|&b| b == c)
                    .expect("ladder char")
            })
            .collect()
    };
    let rising = ladder_index(&sparkline_in(TuiGlyphMode::Ascii, &rising_eight()));
    for pair in rising.windows(2) {
        assert!(
            pair[1] > pair[0],
            "strictly rising values must paint strictly stronger ladder chars"
        );
    }
    let plateau = ladder_index(&sparkline_in(TuiGlyphMode::Ascii, &[0.0, 10.0, 10.0, 20.0]));
    for pair in plateau.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "non-decreasing values must never paint a weaker ladder char"
        );
    }
}

// test-intent: behavior
/// The same character ladder carries the levels on a colorless terminal: with
/// the profile stripped to Monochrome the output is identical to the TrueColor
/// profile's, so "state is understandable without color" holds by glyph, not
/// by palette.
#[test]
fn ascii_levels_survive_a_colorless_profile() {
    let monochrome = TuiTerminalProfile {
        color: TuiColorMode::Monochrome,
        glyphs: TuiGlyphMode::Ascii,
    };
    let truecolor = TuiTerminalProfile {
        color: TuiColorMode::TrueColor,
        glyphs: TuiGlyphMode::Ascii,
    };
    assert_eq!(sparkline_in(monochrome.glyphs, &rising_eight()), " .:-=+*#");
    assert_eq!(
        sparkline_in(monochrome.glyphs, &rising_eight()),
        sparkline_in(truecolor.glyphs, &rising_eight())
    );
}

// test-intent: behavior
/// The ASCII repertoire keeps the Unicode gap semantics: an explicit `NaN`
/// gap and the cold-start placeholder render the `_` underline family, which
/// sits outside the ladder vocabulary exactly like the Unicode mid-dots sit
/// outside the block ramp, so absence can never read as a real level.
#[test]
fn ascii_mode_renders_gaps_and_cold_start_outside_the_ladder() {
    assert_eq!(device_trend_in(TuiGlyphMode::Ascii, &[42.0], 24), "____");
    let dual = device_dual_trend_in(
        TuiGlyphMode::Ascii,
        &[10.0, f32::NAN, 30.0],
        &[20.0, 20.0, 20.0],
        24,
    );
    assert_eq!(dual.primary, " _#");
    assert_eq!(dual.secondary, "===");
    assert!(
        !SPARKLINE_ASCII_BLOCKS.contains(&ASCII_TREND_GAP),
        "the gap glyph must stay outside the ladder"
    );
    assert!(
        ASCII_TREND_PLACEHOLDER
            .chars()
            .all(|c| c == ASCII_TREND_GAP),
        "the placeholder repeats the gap glyph"
    );
}

// test-intent: behavior
/// Every ASCII trend step is one ASCII byte — one terminal cell per sample,
/// so the painted trend can neither overlap nor misalign its columns — and a
/// 24-sample trend clipped into a six-cell row leaves the row's remaining
/// cells and the neighboring rows untouched: no spill past the clipped area.
#[test]
fn ascii_trends_keep_one_cell_per_sample_and_clip_on_narrow_frames() {
    let many: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let trend = ascii_device_trend(&many);
    assert_eq!(trend.chars().count(), SPARKLINE_MAX_SAMPLES);
    assert!(trend.is_ascii(), "every trend char is one terminal cell");

    let backend = TestBackend::new(8, 3);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let area = Rect::new(0, 1, 6, 1);
    terminal
        .draw(|frame| frame.render_widget(ratatui::text::Line::from(trend.clone()), area))
        .expect("draw");
    let prefix: String = trend.chars().take(6).collect();
    let expected = format!("\"        \"\n\"{prefix}  \"\n\"        \"\n");
    assert_eq!(terminal.backend().to_string(), expected);
}

// test-intent: behavior
/// The ASCII ladder vocabulary itself stays a single-cell, distinct,
/// full-length ramp: eight distinct ASCII characters matching the Unicode
/// ramp's level count, so both repertoires carry the same normalization
/// granularity.
#[test]
fn ascii_ladder_vocabulary_is_single_cell_ascii_and_distinct() {
    let mut seen: Vec<char> = Vec::new();
    for c in SPARKLINE_ASCII_BLOCKS {
        assert!(c.is_ascii(), "ladder char {c:?} must be ASCII");
        assert!(!seen.contains(&c), "ladder char {c:?} must be distinct");
        seen.push(c);
    }
    assert_eq!(
        seen.len(),
        SPARKLINE_BLOCKS.len(),
        "the ASCII ladder carries the same level count as the Unicode ramp"
    );
}
