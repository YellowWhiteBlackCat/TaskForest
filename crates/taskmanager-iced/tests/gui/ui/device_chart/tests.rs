//! Tests for the per-device mini-graph canvas.

use std::rc::Rc;

use iced::widget::canvas;
use iced::{Color, Length, Point, Rectangle, Size, mouse};
use taskmanager_application::i18n::t;
use taskmanager_telemetry_store::live_graph::MetricSeries;

use super::scale::summary_value;
use super::*;
use crate::perf_chart::ReadoutColors;

/// Build one hover-capable test chart; `hover` switches the readout on/off.
fn hover_chart(hover: bool) -> DeviceChart {
    DeviceChart {
        samples: Rc::from([10.0, 30.0, 60.0, 80.0].as_slice()),
        color: Color::WHITE,
        max: 100.0,
        grid_color: Color::BLACK,
        smooth: false,
        hover,
        scale: DeviceMetricScale::Percent,
        readout: ReadoutColors {
            bg: Color::BLACK,
            fg: Color::WHITE,
        },
    }
}

/// The hover readout formats the hovered sample in the graph's own unit
/// family through the same `summary_value` rule the caption uses, so the
/// pill and the caption never disagree on a unit; an index beyond the
/// window yields no label (the honest partial-buffer state).
#[test]
fn device_readout_text_formats_the_hovered_sample_in_the_graph_unit() {
    assert_eq!(
        device_readout_text(DeviceMetricScale::Percent, &[10.0, 42.0], 1),
        Some("42%".to_string())
    );
    assert_eq!(
        device_readout_text(
            DeviceMetricScale::BytesPerSecond {
                use_bytes: true,
                use_base2: true
            },
            &[0.0, 1_048_576.0],
            1
        ),
        Some("1.0 MiB/s".to_string())
    );
    assert_eq!(
        device_readout_text(
            DeviceMetricScale::BytesPerSecond {
                use_bytes: false,
                use_base2: false
            },
            &[0.0, 1_000_000.0],
            1
        ),
        Some("8.0 Mb/s".to_string()),
        "the bits+base-10 pair (the network product default) reads out in decimal bits"
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Rpm, &[0.0, 1_500.0], 1),
        Some("1500 RPM".to_string())
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::AutoPeak, &[0.0, 37.5], 1),
        Some("37.5".to_string())
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Watts, &[0.0, 37.5], 1),
        Some("37.5 W".to_string())
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Celsius, &[0.0, 47.4], 1),
        Some("47 \u{b0}C".to_string())
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Megahertz, &[0.0, 2_395.0], 1),
        Some("2395 MHz".to_string())
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Percent, &[10.0, 42.0], 2),
        None,
        "an index beyond the window has nothing to read out"
    );
    assert_eq!(
        device_readout_text(DeviceMetricScale::Percent, &[10.0, f32::NAN], 1),
        None,
        "an explicit history gap must not be formatted into the pill"
    );
}

/// Cursor motion over a hover-enabled device chart writes the hovered
/// sample index into the persistent widget-tree state and requests a
/// redraw; leaving the frame clears it; unchanged motion requests
/// nothing — the same contract as the CPU chart.
#[test]
fn device_chart_hover_tracks_cursor_motion_and_clears_on_cursor_left() {
    let chart = hover_chart(true);
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let mut state = DeviceChartState::default();

    // Move to x = 50 of 200 → nearest sample 1 of 0..3.
    let position = Point::new(50.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position }),
        bounds,
        mouse::Cursor::Available(position),
    );
    assert_eq!(state.hover.index, Some(1));
    assert!(action.is_some(), "a state change must request a redraw");

    // Same index again → no action (no pointless redraw).
    let same = Point::new(49.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position: same }),
        bounds,
        mouse::Cursor::Available(same),
    );
    assert_eq!(state.hover.index, Some(1));
    assert!(
        action.is_none(),
        "unchanged hover must not request a redraw"
    );

    // Leaving the frame clears the hover and requests a redraw.
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorLeft),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert_eq!(state.hover.index, None);
    assert!(action.is_some(), "clearing hover must request a redraw");
}

/// A hover-disabled device chart (the secondary engine/power/temperature
/// graphs) ignores cursor motion entirely: no state, no redraw requests.
#[test]
fn device_chart_without_hover_ignores_cursor_motion() {
    let chart = hover_chart(false);
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let mut state = DeviceChartState::default();
    let position = Point::new(50.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position }),
        bounds,
        mouse::Cursor::Available(position),
    );
    assert_eq!(
        state.hover.index, None,
        "hover-off charts never track the cursor"
    );
    assert!(action.is_none(), "hover-off charts never request redraws");
}

/// The crosshair affordance is offered only while the pointer is over a
/// hover-enabled graph; hover-off graphs keep the default interaction.
#[test]
fn device_chart_crosshair_only_for_hover_enabled_graphs() {
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
    assert_eq!(
        canvas::Program::mouse_interaction(
            &hover_chart(true),
            &DeviceChartState::default(),
            bounds,
            inside,
        ),
        mouse::Interaction::Crosshair
    );
    assert_eq!(
        canvas::Program::mouse_interaction(
            &hover_chart(false),
            &DeviceChartState::default(),
            bounds,
            inside,
        ),
        mouse::Interaction::default(),
        "hover-off graphs must not claim the crosshair"
    );
    assert_eq!(
        canvas::Program::mouse_interaction(
            &hover_chart(true),
            &DeviceChartState::default(),
            bounds,
            mouse::Cursor::Unavailable,
        ),
        mouse::Interaction::default(),
        "no crosshair outside the frame"
    );
}

/// The graph factories compose for both states of the `GraphPrefs.hover`
/// switch (the render smoke for the hover flag): the hover-enabled main
/// graph and the hover-off secondary graph both build a full caption +
/// canvas element, and the caption resolves the collecting/plotted states
/// through the shared rule.
#[test]
fn device_mini_graph_factories_compose_with_hover_on_and_off() {
    let theme_snapshot = taskmanager_theme::Theme::dark();
    let main = device_mini_graph(
        vec![10.0, 20.0, 30.0],
        DeviceMetricScale::Percent,
        Color::WHITE,
        "Utilization".to_string(),
        &theme_snapshot,
        GraphPrefs {
            smooth: false,
            max_override: None,
            hover: true,
        },
    );
    assert_eq!(
        main.as_widget().size().height,
        Length::Shrink,
        "the caption column reports its own height"
    );
    let secondary = device_mini_graph_with_height(
        vec![10.0],
        DeviceMetricScale::Percent,
        Color::WHITE,
        "Power".to_string(),
        &theme_snapshot,
        SECONDARY_DEVICE_CHART_HEIGHT,
        GraphPrefs {
            smooth: false,
            max_override: None,
            hover: false,
        },
    );
    let _ = secondary;
}

/// Percentage series always pin the frame ceiling at 100 — a 120% reading
/// clamps inside the frame, the ceiling does NOT grow.
#[test]
fn percentage_series_ceiling_is_one_hundred_regardless_of_samples() {
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::CpuUsagePercent, &[]), PERCENT_MAX);
    assert_eq!(
        series_max(taskmanager_shell::presentation::trend::TrendSeries::GpuUsagePercent, &[200.0, 300.0]),
        PERCENT_MAX
    );
    assert_eq!(
        series_max(taskmanager_shell::presentation::trend::TrendSeries::MemoryUsagePercent, &[12.0]),
        PERCENT_MAX
    );
    assert_eq!(
        series_max(taskmanager_shell::presentation::trend::TrendSeries::DiskActiveTimePct, &[200.0]),
        PERCENT_MAX,
        "disk active time is percentage-typed: the ceiling stays 100"
    );
}

/// A bytes/sec series scales to its own finite peak so traffic actually
/// moves the line; an all-zero/empty (idle) window yields 0.0, which the
/// projection renders as a flat baseline.
#[test]
fn bytes_per_sec_series_scales_to_its_finite_peak() {
    // Idle / empty → 0.0 (flat baseline, never a fabricated mid-line).
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::DiskBytesPerSec, &[]), 0.0);
    assert_eq!(
        series_max(taskmanager_shell::presentation::trend::TrendSeries::NetworkBytesPerSec, &[0.0, 0.0]),
        0.0
    );
    // A real peak sets the ceiling so the peak sample reaches the top.
    let disk = &[1_000_000.0_f32, 5_000_000.0, 2_000_000.0];
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::DiskBytesPerSec, disk), 5_000_000.0);
    let net = &[300.0_f32, 900.0];
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::NetworkBytesPerSec, net), 900.0);
}

/// The decoupled scale enum is the single source of truth for the per-device
/// graph ceiling and summary unit — battery charge % and fan RPM (which have
/// no MetricSeries variant) resolve onto it directly. Percent pins 100;
/// magnitude variants track the finite peak; and every MetricSeries bridges
/// onto the rule it needs, so graph pixels and readouts never disagree.
#[test]
fn device_metric_scale_picks_the_ceiling_and_bridges_metric_series() {
    // Percent pins the frame ceiling at 100 regardless of the samples — a
    // 120% battery/GPU reading clamps inside the frame, the ceiling does NOT grow.
    assert_eq!(series_max(DeviceMetricScale::Percent, &[]), PERCENT_MAX);
    assert_eq!(
        series_max(DeviceMetricScale::Percent, &[200.0, 300.0]),
        PERCENT_MAX
    );
    // AutoPeak tracks the finite peak so fan RPM / bytes/sec rise with the
    // value; an empty/zero (idle) window yields 0.0 — a flat baseline.
    assert_eq!(series_max(DeviceMetricScale::AutoPeak, &[]), 0.0);
    assert_eq!(
        series_max(DeviceMetricScale::AutoPeak, &[0.0_f32, 1_200.0, 800.0]),
        1_200.0,
        "a fan RPM window scales to its finite peak"
    );
    // Every MetricSeries bridges onto the scale it always had.
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::GpuUsagePercent),
        DeviceMetricScale::Percent,
        "GPU% (and battery charge %) is percentage-typed"
    );
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::DiskActiveTimePct),
        DeviceMetricScale::Percent,
        "disk active-time % bridges onto the fixed percentage ceiling"
    );
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::DiskBytesPerSec),
        DeviceMetricScale::BytesPerSecond {
            use_bytes: true,
            use_base2: true
        },
        "bytes/sec keeps its rate unit while using magnitude scaling"
    );
    assert_eq!(summary_value(DeviceMetricScale::Rpm, 1_500.0), "1500 RPM");
    assert_eq!(
        summary_value(
            DeviceMetricScale::BytesPerSecond {
                use_bytes: true,
                use_base2: true
            },
            1_048_576.0
        ),
        "1.0 MiB/s"
    );
}

/// Every scale variant formats its summary with its own unit token — the
/// unit-carrying magnitude variants print W / °C / MHz (GPUI's
/// `format_graph_value` parity) so an AutoPeak-family graph never shows a
/// bare number. Enumerated through `ALL` with an explicit expected string
/// per variant (the table IS the contract); the length check keeps the
/// table exhaustive when a variant is added. The bytes/sec variant then
/// covers the full persisted preference matrix — the caption summary and
/// hover pill follow the resolved Drive/Network pair (decimal bits at the
/// network product default), never a hardcoded binary-bytes readout.
#[test]
fn every_scale_variant_formats_summary_values_with_its_unit() {
    let expectations: &[(DeviceMetricScale, f32, &str)] = &[
        (DeviceMetricScale::Percent, 37.5, "38%"),
        (DeviceMetricScale::AutoPeak, 37.5, "37.5"),
        (
            DeviceMetricScale::BytesPerSecond {
                use_bytes: true,
                use_base2: true,
            },
            1_048_576.0,
            "1.0 MiB/s",
        ),
        (DeviceMetricScale::Rpm, 37.5, "38 RPM"),
        (DeviceMetricScale::Watts, 37.5, "37.5 W"),
        (DeviceMetricScale::Celsius, 37.5, "38 \u{b0}C"),
        (DeviceMetricScale::Megahertz, 37.5, "38 MHz"),
    ];
    for &(scale, value, expected) in expectations {
        assert_eq!(summary_value(scale, value), expected, "{scale:?}");
    }
    assert_eq!(
        expectations.len(),
        DeviceMetricScale::ALL.len(),
        "every scale variant must carry a summary-formatting expectation"
    );
    // The bytes/sec preference matrix: bytes vs bits × base-2 vs base-10.
    let bps = |use_bytes: bool, use_base2: bool| DeviceMetricScale::BytesPerSecond {
        use_bytes,
        use_base2,
    };
    assert_eq!(summary_value(bps(false, false), 1_000_000.0), "8.0 Mb/s");
    assert_eq!(summary_value(bps(true, false), 1_500_000.0), "1.5 MB/s");
    assert_eq!(summary_value(bps(false, true), 1_048_576.0), "8.0 Mib/s");
    // A non-finite sample stays an honest dash in every unit family.
    assert_eq!(summary_value(bps(false, false), f32::NAN), "—");
    // The fixed CPU chart stack keeps unit-carrying histories typed.
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::CpuTemperatureC),
        DeviceMetricScale::Celsius
    );
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::CpuFrequencyMhz),
        DeviceMetricScale::Megahertz
    );
}

/// CPU package power (RAPL watts) is a magnitude series like temperature
/// °C / clock MHz — it must auto-scale to its finite peak so the trace
/// rises with the draw, NOT clamp flat against the 100% ceiling (which would
/// pin every reading to the top since watts are typically 5..125, never
/// 0..100). Empty (RAPL unavailable) → 0.0 (flat baseline, never fabricated).
#[test]
fn cpu_power_watts_auto_scales_to_peak_not_clamped_to_one_hundred() {
    assert_eq!(
        DeviceMetricScale::from(taskmanager_shell::presentation::trend::TrendSeries::CpuPowerW),
        DeviceMetricScale::Watts,
        "CPU power watts is magnitude-typed (unit W), not percentage-typed"
    );
    // Empty (RAPL absent) → 0.0, the flat-baseline idle — never a fake line.
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::CpuPowerW, &[]), 0.0);
    // A real RAPL trace scales to its finite peak; the 142 W sample reaches
    // the top of the frame instead of clamping against a meaningless 100.
    let power = &[7.5_f32, 142.0, 38.0];
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::CpuPowerW, power), 142.0);
    // The percentage ceiling does NOT apply — a 42 W reading is well under
    // 100 yet still maps its peak (42) to the frame top, not to 42% of it.
    assert_eq!(series_max(taskmanager_shell::presentation::trend::TrendSeries::CpuPowerW, &[42.0, 42.0]), 42.0);
}

/// The mini-graph caption honors the DeviceChart's <2-point draw gate: an
/// empty or single-sample window is honestly "· collecting" (the canvas
/// strokes nothing), while two-plus samples plot and the caption is just the
/// metric label. The collecting suffix resolves through the shared catalog,
/// so the expectation is composed from `t` — pinning that the suffix is
/// catalog-driven, never a frozen English literal.
#[test]
fn mini_graph_caption_marks_sub_two_sample_windows_as_collecting() {
    // An empty / single-sample window is honestly collecting, never blank.
    assert_eq!(
        mini_graph_caption("Throughput (read + write)", 0),
        format!("Throughput (read + write) · {}", t("graph.collecting"))
    );
    assert_eq!(
        mini_graph_caption("Utilization", 1),
        format!("Utilization · {}", t("graph.collecting"))
    );
    // Two or more samples plot, so the caption is just the metric label.
    assert_eq!(mini_graph_caption("Utilization", 2), "Utilization");
    assert_eq!(
        mini_graph_caption("Throughput (rx + tx)", 64),
        "Throughput (rx + tx)"
    );
}

#[test]
fn mini_graph_summary_uses_latest_average_and_peak_without_gap_fabrication() {
    let summary = mini_graph_summary(DeviceMetricScale::Percent, &[10.0, f32::NAN, 30.0])
        .expect("finite samples produce a summary");
    assert!(summary.contains("30%"), "latest/peak missing: {summary}");
    assert!(summary.contains("20%"), "average missing: {summary}");
    assert_eq!(
        mini_graph_summary(DeviceMetricScale::Percent, &[f32::NAN]),
        None
    );
}

/// The device graph's area fill is the shared Mission-Center vertical
/// gradient (category color ~0.35 at the top → transparent at the
/// baseline) at the device chart's own height — never a solid alpha wash,
/// and never a banded stand-in, because iced 0.14's public canvas
/// `gradient::Linear` + `Fill::from` path renders the real gradient.
#[test]
fn device_area_fill_uses_the_shared_vertical_gradient() {
    let color = Color::from_rgb(0.2, 0.5, 0.9);
    let gradient = crate::perf_chart::vertical_area_gradient(color, DEVICE_CHART_HEIGHT);
    assert_eq!(gradient.start, iced::Point::new(0.0, 0.0));
    assert_eq!(gradient.end, iced::Point::new(0.0, DEVICE_CHART_HEIGHT));
    let stops: Vec<iced::gradient::ColorStop> = gradient.stops.iter().flatten().copied().collect();
    assert_eq!(stops.len(), 3);
    let alphas: Vec<f32> = stops.iter().map(|stop| stop.color.a).collect();
    assert!(
        alphas.windows(2).all(|pair| pair[0] > pair[1]),
        "alpha must strictly decrease down the fill: {alphas:?}"
    );
    assert_eq!(alphas[0], 0.35);
    assert_eq!(alphas[2], 0.0, "the fill ends fully transparent");
}

/// The DATA fingerprint combines immutable snapshot generation, auto-scale
/// `max`, and smoothing policy.
/// The `max` field is the regression bait for a bytes/sec series whose
/// finite peak moved but whose newest sample happened to stay the same —
/// without `max` in the fingerprint the line would reuse stale geometry
/// clamped to the old ceiling.
#[test]
fn device_data_fingerprint_keys_on_generation_max_and_smooth() {
    let samples: Rc<[f32]> = Rc::from([10.0, 50.0].as_slice());
    let base = DeviceChartDataFingerprint::from_window(&samples, 100.0, false);
    assert_eq!(
        base,
        DeviceChartDataFingerprint::from_window(&samples, 100.0, false)
    );
    let same_len_same_tail: Rc<[f32]> = Rc::from([90.0, 50.0].as_slice());
    assert_ne!(
        base,
        DeviceChartDataFingerprint::from_window(&same_len_same_tail, 100.0, false),
        "middle/leading changes cannot hide behind the same length and tail"
    );
    // Auto-scale max changed (bytes/sec peak rose) → not equal.
    assert_ne!(
        base,
        DeviceChartDataFingerprint::from_window(&samples, 200.0, false),
        "a max change must force a data-cache rebuild"
    );
    // Smooth toggled → not equal (the cached path family must switch).
    assert_ne!(
        base,
        DeviceChartDataFingerprint::from_window(&samples, 100.0, true),
        "a smooth toggle must force a data-cache rebuild"
    );
}

/// The program's `fingerprint()` mirrors `DeviceChartDataFingerprint::from_window`
/// — the seam `draw()` keys the data-cache-clear gate on. Colors/theme
/// tokens are NOT in the fingerprint (a theme switch is rare and one
/// stale-color frame is acceptable — matches round-1 process_sparkline;
/// asserted here so it is not "fixed" back).
#[test]
fn device_chart_fingerprint_tracks_data_not_color() {
    let base = hover_chart(true);
    let base_fp = base.fingerprint();
    let stable = DeviceChart {
        samples: Rc::clone(&base.samples),
        ..hover_chart(true)
    };
    assert_eq!(base_fp, stable.fingerprint());
    // Different colors (theme switch) → SAME fingerprint.
    let recolored = DeviceChart {
        samples: Rc::clone(&base.samples),
        color: Color::from_rgb(1.0, 0.0, 0.0),
        grid_color: Color::from_rgb(0.0, 1.0, 0.0),
        readout: ReadoutColors {
            bg: Color::from_rgb(0.0, 0.0, 1.0),
            fg: Color::from_rgb(1.0, 1.0, 0.0),
        },
        ..hover_chart(true)
    };
    assert_eq!(
        base_fp,
        recolored.fingerprint(),
        "color/theme must NOT be in the fingerprint"
    );
    // A max change → different fingerprint.
    let rescaled = DeviceChart {
        samples: Rc::clone(&base.samples),
        max: 200.0,
        ..hover_chart(true)
    };
    assert_ne!(base_fp, rescaled.fingerprint());
    // A smooth toggle → different fingerprint.
    let smoothed = DeviceChart {
        samples: Rc::clone(&base.samples),
        smooth: true,
        ..hover_chart(true)
    };
    assert_ne!(base_fp, smoothed.fingerprint());
}

/// The OVERLAY fingerprint combines the hover index and the data
/// fingerprint, so the overlay rebuilds when EITHER the cursor moves OR the
/// data ticks. When hover is disabled the overlay fingerprint's hover_index
/// stays `None` and the overlay is a constant empty frame.
#[test]
fn device_overlay_fingerprint_combines_hover_and_data() {
    let samples: Rc<[f32]> = Rc::from([10.0, 50.0].as_slice());
    let data = DeviceChartDataFingerprint::from_window(&samples, 100.0, false);
    let none = DeviceChartOverlayFingerprint {
        hover_index: None,
        data: data.clone(),
    };
    // Same → equal.
    assert_eq!(
        none,
        DeviceChartOverlayFingerprint {
            hover_index: None,
            data: data.clone(),
        }
    );
    // Hover appears → not equal.
    assert_ne!(
        none,
        DeviceChartOverlayFingerprint {
            hover_index: Some(1),
            data: data.clone(),
        }
    );
    // Same hover but data ticked → not equal (pill text must refresh).
    let ticked_samples: Rc<[f32]> = Rc::from([10.0, 60.0].as_slice());
    let ticked = DeviceChartDataFingerprint::from_window(&ticked_samples, 100.0, false);
    assert_ne!(
        DeviceChartOverlayFingerprint {
            hover_index: Some(1),
            data,
        },
        DeviceChartOverlayFingerprint {
            hover_index: Some(1),
            data: ticked,
        },
        "a data tick must refresh the overlay so the pill text stays live"
    );
}

impl DeviceMetricScale {
    /// Every variant, in declaration order — the single list the unit-suffix
    /// table test enumerates, so a new variant cannot ship without a formatting
    /// decision (the exhaustive matches already force the code side).
    pub(crate) const ALL: [Self; 7] = [
        Self::Percent,
        Self::AutoPeak,
        Self::BytesPerSecond {
            use_bytes: true,
            use_base2: true,
        },
        Self::Rpm,
        Self::Watts,
        Self::Celsius,
        Self::Megahertz,
    ];
}
