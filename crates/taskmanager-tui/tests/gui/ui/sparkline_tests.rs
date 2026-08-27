use super::test_support::device_trend;
use super::*;

/// A flat series renders as a constant mid-ramp line — honest about the
/// trend being flat, never a panic on a zero range. `(0.5 * 7.0).round()`
/// is 4, so the flat ramp is the index-4 block ('▅').
#[test]
fn flat_series_renders_a_constant_mid_ramp_line() {
    let spark = sparkline(&[5.0, 5.0, 5.0]);
    assert!(spark.chars().all(|c| c == '▅'), "flat → mid ramp: {spark}");
    assert_eq!(spark.chars().count(), 3);
}

/// A rising series renders ascending ramp blocks so the trend SHAPE is
/// preserved even though the absolute values are normalized away.
#[test]
fn rising_series_renders_ascending_ramp_blocks() {
    let spark = sparkline(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0]);
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
    assert_eq!(sparkline(&[]), "");
    let long: Vec<f32> = (0..128).map(|i| i as f32).collect();
    let windowed = recent_window(&long);
    assert_eq!(windowed.len(), SPARKLINE_MAX_SAMPLES);
    assert_eq!(sparkline(windowed).chars().count(), SPARKLINE_MAX_SAMPLES);
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
