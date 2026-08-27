// test-intent: behavior
//! Behavior tests for the iced chart window/hover upgrades: the unified
//! `WindowSlots` mapping (right-anchored capacity slots, GPUI `sample_x`
//! parity), the windowed NaN-gap projection, the two-series device graph's
//! fingerprints/colors/readout, and the y-axis tick ladder. All pure-function
//! contracts — no renderer involved.

use std::rc::Rc;

use iced::widget::canvas;
use iced::{Color, Length, Point, Rectangle, Size, mouse};

use super::*;
use crate::perf_chart::{
    ChartOpts, WindowSlots, hovered_index, sample_x, scaled_y, series_point_runs_windowed,
    y_axis_tick_values,
};

const SIZE: Size = Size::new(100.0, 50.0);

/// The newest sample pins to the right edge and every sample owns a fixed
/// slot of the capacity window — a growing graph extends right-to-left
/// instead of stretching its first two samples across the full width (GPUI
/// `sample_x` semantics).
#[test]
fn window_slots_pin_the_newest_sample_right_and_grow_leftward() {
    // Two samples of a five-slot window: the newest sits at the right edge,
    // the older one exactly one slot (width / 4) to its left.
    let slots = WindowSlots::new(2, 5, 100.0);
    assert!((slots.x(1) - 100.0).abs() < 1e-4, "newest pinned right");
    assert!(
        (slots.x(0) - 75.0).abs() < 1e-4,
        "older sample one fixed slot left: {}",
        slots.x(0)
    );
    // Once the window fills, the mapping becomes the usual full-span spread.
    let full = WindowSlots::new(5, 5, 100.0);
    for index in 0..5 {
        assert!((full.x(index) - 100.0 * index as f32 / 4.0).abs() < 1e-4);
    }
    // A capacity below the sample count degrades safely to the full spread.
    assert!((WindowSlots::new(6, 2, 100.0).x(5) - 100.0).abs() < 1e-4);
}

/// The full-window (`spread`) form IS the legacy projection: every existing
/// chart that spreads its window renders pixel-identical through the unified
/// mapping, both forward (drawing x) and inverse (hover index).
#[test]
fn window_slots_spread_matches_the_legacy_projection() {
    for count in 2..8_usize {
        let slots = WindowSlots::spread(count, 200.0);
        for index in 0..count {
            assert!(
                (slots.x(index) - sample_x(index, count, 200.0)).abs() < 1e-4,
                "x drift at index {index} of {count}"
            );
        }
        for cursor in [1.0, 25.0, 99.5, 150.0, 199.0] {
            assert_eq!(
                slots.index_at(cursor),
                hovered_index(cursor, 200.0, count),
                "hover drift at cursor {cursor} of {count}"
            );
        }
    }
}

/// The inverse mapping round-trips every slot: hovering at a sample's x
/// resolves to that sample — the guarantee that the crosshair and the tooltip
/// never point beside the value they report. The pinned-right newest sample
/// sits exactly ON the frame edge, which the half-open cursor range excludes,
/// so its probe steps just inside the edge.
#[test]
fn window_slots_index_at_round_trips_every_slot() {
    for (count, capacity) in [(3_usize, 10_usize), (7, 7), (10, 10), (4, 64)] {
        let slots = WindowSlots::new(count, capacity, 240.0);
        for index in 0..count {
            let x = slots.x(index);
            let probe = if x >= 240.0 { 240.0 - 1e-3 } else { x };
            assert_eq!(
                slots.index_at(probe),
                Some(index),
                "slot {index} of {count}/{capacity} did not round-trip"
            );
        }
    }
}

/// Degenerate frames and windows resolve no hover: outside the frame, a
/// non-positive or non-finite width, and fewer than two samples (nothing to
/// plot or hover — the honest too-few-samples state).
#[test]
fn window_slots_reject_degenerate_frames_and_windows() {
    let slots = WindowSlots::new(4, 8, 200.0);
    assert_eq!(slots.index_at(-0.5), None);
    assert_eq!(slots.index_at(200.0), None);
    assert_eq!(slots.index_at(f32::NAN), None);
    assert_eq!(WindowSlots::new(4, 8, 0.0).index_at(50.0), None);
    assert_eq!(WindowSlots::new(4, 8, f32::NAN).index_at(50.0), None);
    assert_eq!(WindowSlots::new(0, 8, 200.0).index_at(50.0), None);
    assert_eq!(WindowSlots::new(1, 8, 200.0).index_at(50.0), None);
    assert_eq!(WindowSlots::spread(1, 200.0).x(0), 0.0);
}

/// The windowed projection splits NaN gaps into separate runs and every
/// survivor keeps its own slot x — a gap is missing evidence, never a bridge
/// and never a shift that would reconnect the runs.
#[test]
fn windowed_runs_split_gaps_and_keep_slot_positions() {
    let slots = WindowSlots::spread(4, SIZE.width);
    let runs = series_point_runs_windowed(&[10.0, 20.0, f32::NAN, 40.0], SIZE, 50.0, &slots);
    assert_eq!(runs.len(), 2, "the gap splits the window");
    assert_eq!(runs[0].len(), 2);
    assert_eq!(runs[1].len(), 1);
    // x positions stay on the original slots: 0, 100/3, …, 100.
    assert!((runs[0][0].x - 0.0).abs() < 1e-4);
    assert!((runs[0][1].x - 100.0 / 3.0).abs() < 1e-4);
    assert!(
        (runs[1][0].x - 100.0).abs() < 1e-4,
        "the post-gap run starts at its own slot, not shifted left"
    );
}

/// A partial capacity window keeps its right anchor in the drawn geometry
/// too — the same x the hover resolves through, proving curve and tooltip
/// share the one mapping.
#[test]
fn windowed_runs_use_the_right_anchor_for_partial_windows() {
    let slots = WindowSlots::new(2, 5, SIZE.width);
    let runs = series_point_runs_windowed(&[10.0, 40.0], SIZE, 50.0, &slots);
    assert_eq!(runs.len(), 1);
    assert!((runs[0][0].x - 75.0).abs() < 1e-4);
    assert!((runs[0][1].x - 100.0).abs() < 1e-4);
}

/// The y normalization the runs use is exported as one pure function, so a
/// hover dot lands exactly on the drawn sample's y.
#[test]
fn scaled_y_matches_the_run_projection() {
    for (value, max) in [(0.0_f32, 50.0), (25.0, 50.0), (50.0, 50.0), (120.0, 50.0)] {
        let runs = series_point_runs_windowed(&[value], SIZE, max, &WindowSlots::spread(2, 100.0));
        assert_eq!(runs[0][0].y, scaled_y(value, max, SIZE.height));
    }
    // An idle/non-finite ceiling anchors at the baseline — never a fabricated
    // mid-line — and clamping keeps out-of-range values inside the frame.
    assert_eq!(scaled_y(30.0, 0.0, 50.0), 50.0);
    assert_eq!(scaled_y(30.0, f32::NAN, 50.0), 50.0);
    assert_eq!(scaled_y(f32::NAN, 50.0, 50.0), 50.0);
}

/// The tick ladder is the readable three-label form and disappears for an
/// idle/unavailable max (no fabricated scale).
#[test]
fn y_axis_tick_ladder_is_top_middle_bottom_or_empty() {
    assert_eq!(y_axis_tick_values(200.0), vec![200.0, 100.0, 0.0]);
    assert_eq!(y_axis_tick_values(0.0), Vec::<f32>::new());
    assert_eq!(y_axis_tick_values(f32::NAN), Vec::<f32>::new());
}

/// Tick labels render only in graphs tall enough to read them: the 260px
/// primary and 128px secondary graphs carry them; the 56px engine strips
/// stay clean.
#[test]
fn axis_ticks_visible_follows_the_three_chart_heights() {
    assert!(axis_ticks_visible(DEVICE_CHART_HEIGHT));
    assert!(axis_ticks_visible(SECONDARY_DEVICE_CHART_HEIGHT));
    assert!(!axis_ticks_visible(ENGINE_DEVICE_CHART_HEIGHT));
    assert!(!axis_ticks_visible(f32::NAN));
}

/// The default grid knobs are the legacy look — parameterizing the grid
/// through `ChartOpts` changes no existing chart's pixels.
#[test]
fn chart_opts_default_is_the_legacy_grid_look() {
    let opts = ChartOpts::default();
    assert_eq!(opts.hlines, 4);
    assert_eq!(opts.vlines, 6);
    assert_eq!(opts.grid_alpha, 0.48);
}

// --- Two-series device graph ------------------------------------------------

fn series(label: &str, samples: &[f32], color: Color) -> multi::DeviceMultiSeries {
    multi::DeviceMultiSeries {
        samples: Rc::from(samples),
        label: label.to_string(),
        color,
    }
}

/// A series sharing one `Rc` generation, so fingerprint equality tests vary
/// only the identity the fingerprint should (or should not) capture.
fn shared_series(label: &str, samples: &Rc<[f32]>, color: Color) -> multi::DeviceMultiSeries {
    multi::DeviceMultiSeries {
        samples: Rc::clone(samples),
        label: label.to_string(),
        color,
    }
}

fn multi_chart() -> multi::DeviceMultiChart {
    multi::DeviceMultiChart {
        primary: series("Read", &[10.0, 20.0, 30.0, 40.0], Color::WHITE),
        secondary: series("Write", &[5.0, f32::NAN, 15.0, 20.0], Color::BLACK),
        max: 50.0,
        capacity: 10,
        grid_color: Color::BLACK,
        tick_color: Color::WHITE,
        smooth: false,
        hover: true,
        format_value: |value| format!("{value:.0} MB/s"),
        readout: ReadoutColors {
            bg: Color::BLACK,
            fg: Color::WHITE,
        },
        opts: ChartOpts::default(),
    }
}

/// The secondary series color is the SAME family token lifted toward white —
/// a tint of the existing product color, never a new hue — and the alpha is
/// untouched so dark/high-contrast skins keep their contrast contract.
#[test]
fn dual_series_colors_tint_the_family_token_toward_white() {
    let base = Color::from_rgba(0.2, 0.5, 0.9, 1.0);
    let (primary, secondary) = multi::dual_series_colors(base);
    assert_eq!(primary, base);
    for (b, s) in [
        (base.r, secondary.r),
        (base.g, secondary.g),
        (base.b, secondary.b),
    ] {
        assert!(s > b, "every channel lifts toward white: {b} -> {s}");
        assert!(s <= 1.0);
    }
    assert_eq!(secondary.a, base.a);
    // A fully-lit channel stays at 1.0 (no overflow).
    let lifted = multi::dual_series_colors(Color::from_rgba(1.0, 0.0, 0.5, 0.8)).1;
    assert_eq!(lifted.r, 1.0);
    assert_eq!(lifted.a, 0.8);
}

/// The hover pill joins both series' formatted values through the injected
/// unit formatter; a series holding a gap at the index contributes nothing,
/// and a shared gap suppresses the pill entirely.
#[test]
fn multi_readout_text_joins_both_series_and_skips_gaps() {
    let chart = multi_chart();
    assert_eq!(
        multi::multi_readout_text(&chart.primary, &chart.secondary, 3, chart.format_value),
        Some("Read 40 MB/s · Write 20 MB/s".to_string())
    );
    // Index 1: the secondary window holds an explicit gap — only the series
    // with evidence reads out.
    assert_eq!(
        multi::multi_readout_text(&chart.primary, &chart.secondary, 1, chart.format_value),
        Some("Read 20 MB/s".to_string())
    );
    let gapped = multi::DeviceMultiSeries {
        samples: Rc::from([f32::NAN, f32::NAN].as_slice()),
        label: "Write".to_string(),
        color: Color::BLACK,
    };
    assert_eq!(
        multi::multi_readout_text(&chart.primary, &gapped, 0, chart.format_value),
        Some("Read 10 MB/s".to_string())
    );
    let both_gapped = multi::DeviceMultiSeries {
        samples: Rc::from([f32::NAN, f32::NAN].as_slice()),
        label: "Read".to_string(),
        color: Color::WHITE,
    };
    assert_eq!(
        multi::multi_readout_text(&both_gapped, &gapped, 0, chart.format_value),
        None,
        "a shared gap has nothing honest to read out"
    );
}

/// The DATA fingerprint keys on BOTH series' generations, the shared max,
/// the capacity (the slot grid), smoothing, and the legend labels — but NOT
/// on colors (the established one-stale-color-frame contract).
#[test]
fn multi_data_fingerprint_keys_on_both_series_and_labels() {
    // Two programs sharing the same immutable Rc generations must be equal…
    let primary_samples: Rc<[f32]> = Rc::from([10.0, 20.0, 30.0, 40.0].as_slice());
    let secondary_samples: Rc<[f32]> = Rc::from([5.0, f32::NAN, 15.0, 20.0].as_slice());
    let with_shared = || multi::DeviceMultiChart {
        primary: shared_series("Read", &primary_samples, Color::WHITE),
        secondary: shared_series("Write", &secondary_samples, Color::BLACK),
        ..multi_chart()
    };
    let base = with_shared();
    assert_eq!(base.fingerprint(), with_shared().fingerprint());
    // …while the baseline helper's fresh allocations differ (generation key).
    assert_ne!(
        base.fingerprint(),
        multi_chart().fingerprint(),
        "a new history revision (fresh allocation) must not reuse cached geometry"
    );
    // Either series' window moving must rebuild the data layer.
    let moved_primary = multi::DeviceMultiChart {
        primary: series("Read", &[10.0, 20.0, 30.0, 41.0], Color::WHITE),
        ..multi_chart()
    };
    assert_ne!(base.fingerprint(), moved_primary.fingerprint());
    let moved_secondary = multi::DeviceMultiChart {
        secondary: series("Write", &[5.0, f32::NAN, 15.0, 21.0], Color::BLACK),
        ..multi_chart()
    };
    assert_ne!(base.fingerprint(), moved_secondary.fingerprint());
    // Shared max / capacity / smooth / legend labels each force a rebuild.
    for mutated in [
        multi::DeviceMultiChart {
            max: 80.0,
            ..multi_chart()
        },
        multi::DeviceMultiChart {
            capacity: 60,
            ..multi_chart()
        },
        multi::DeviceMultiChart {
            smooth: true,
            ..multi_chart()
        },
        multi::DeviceMultiChart {
            primary: series("Recv", &[10.0, 20.0, 30.0, 40.0], Color::WHITE),
            ..multi_chart()
        },
    ] {
        assert_ne!(base.fingerprint(), mutated.fingerprint());
    }
    // Colors are NOT in the fingerprint (theme switch = one stale frame).
    let recolored = multi::DeviceMultiChart {
        primary: shared_series("Read", &primary_samples, Color::from_rgb(1.0, 0.0, 0.0)),
        secondary: shared_series("Write", &secondary_samples, Color::from_rgb(0.0, 0.0, 1.0)),
        ..multi_chart()
    };
    assert_eq!(
        base.fingerprint(),
        recolored.fingerprint(),
        "color/theme must NOT be in the fingerprint"
    );
}

/// Hover tracking resolves through the capacity slot grid: with a partial
/// window the newest sample still pins right, and cursor motion/leave drives
/// the same persistent-state contract as the single-series chart.
#[test]
fn multi_chart_hover_tracks_the_capacity_slot_grid() {
    let mut chart = multi_chart();
    chart.primary = series("Read", &[10.0, 40.0], Color::WHITE);
    chart.secondary = series("Write", &[5.0, 20.0], Color::BLACK);
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let mut state = multi::DeviceMultiChartState::default();

    // Near the right edge the cursor resolves to the NEWEST sample (index 1)
    // even though the window holds only 2 of 10 slots.
    let position = Point::new(190.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position }),
        bounds,
        mouse::Cursor::Available(position),
    );
    assert_eq!(state.hover.index, Some(1));
    assert!(action.is_some(), "a state change must request a redraw");

    // The same slot again requests nothing (no pointless redraw). With 2 of
    // 10 slots occupied, slot 1 spans x = 177.8..200; 195 is inside it.
    let same_slot = Point::new(195.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: same_slot,
        }),
        bounds,
        mouse::Cursor::Available(same_slot),
    );
    assert_eq!(state.hover.index, Some(1));
    assert!(action.is_none());

    // Far left of the occupied region still resolves to the oldest sample.
    let left = Point::new(10.0, 50.0);
    canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position: left }),
        bounds,
        mouse::Cursor::Available(left),
    );
    assert_eq!(state.hover.index, Some(0));

    // Leaving clears the hover and requests a redraw.
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorLeft),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert_eq!(state.hover.index, None);
    assert!(action.is_some());
}

/// A hover-disabled multi graph ignores cursor motion entirely — no state, no
/// redraw requests (the secondary-chart contract).
#[test]
fn multi_chart_without_hover_ignores_cursor_motion() {
    let mut chart = multi_chart();
    chart.hover = false;
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
    let mut state = multi::DeviceMultiChartState::default();
    let position = Point::new(190.0, 50.0);
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position }),
        bounds,
        mouse::Cursor::Available(position),
    );
    assert_eq!(state.hover.index, None);
    assert!(action.is_none());
}

/// The two-series factory composes a caption + canvas element and its
/// `GraphPrefs` switch carries through — the render smoke for the public
/// factory surface page wiring will call.
#[test]
fn device_multi_graph_factory_composes_the_caption_and_canvas() {
    let theme_snapshot = taskmanager_theme::Theme::dark();
    let element = multi::device_multi_graph_fill(
        multi::DeviceMultiGraphSpec {
            primary: series("Read", &[10.0, 20.0, 30.0], Color::WHITE),
            secondary: series("Write", &[5.0, 15.0, 25.0], Color::BLACK),
            family_color: Color::from_rgba(0.2, 0.5, 0.9, 1.0),
            capacity: 60,
            format_value: |value| format!("{value:.0} MB/s"),
            prefs: GraphPrefs {
                smooth: true,
                max_override: None,
                hover: true,
            },
        },
        "Throughput".to_string(),
        &theme_snapshot,
        true,
    );
    assert_eq!(
        element.as_widget().size().height,
        Length::Shrink,
        "the caption column reports its own height"
    );
}

/// The Fill-height two-series factory routes BOTH branches of the shared
/// compact/wide height policy ([`primary_graph_height`]). The canvas child's
/// `Length` is observable through iced's construction rule — `Column::push`
/// encloses each child's size hint, and `Length::enclose` lifts only
/// `Fill`/`FillPortion` — so the wide graph's canvas (`Length::Fill`) lifts the
/// caption column's height hint to `Fill`, while the compact branch's fixed
/// canvas height leaves it `Shrink`; the canvas itself fills the card width in
/// both layouts.
#[test]
fn device_multi_graph_fill_factory_composes_both_height_policies() {
    let theme_snapshot = taskmanager_theme::Theme::dark();
    let spec = || multi::DeviceMultiGraphSpec {
        primary: series("Read", &[10.0, 20.0, 30.0], Color::WHITE),
        secondary: series("Write", &[5.0, 15.0, 25.0], Color::BLACK),
        family_color: Color::from_rgba(0.2, 0.5, 0.9, 1.0),
        capacity: 60,
        format_value: |value| format!("{value:.0} MB/s"),
        prefs: GraphPrefs {
            smooth: true,
            max_override: None,
            hover: true,
        },
    };
    for compact in [false, true] {
        let element = multi::device_multi_graph_fill(
            spec(),
            "Throughput".to_string(),
            &theme_snapshot,
            compact,
        );
        let expected_height = if compact {
            Length::Shrink
        } else {
            Length::Fill
        };
        assert_eq!(
            element.as_widget().size().height,
            expected_height,
            "the canvas's height Length lifts the column hint only on the wide card \
             (compact={compact})"
        );
        assert_eq!(
            element.as_widget().size().width,
            Length::Fill,
            "the canvas fills the card width in both layouts (compact={compact})"
        );
    }
    assert_eq!(
        primary_graph_height(false),
        Length::Fill,
        "a wide card gives the canvas the remaining column height"
    );
    assert_eq!(
        primary_graph_height(true),
        Length::Fixed(DEVICE_CHART_HEIGHT),
        "a compact scrollable keeps the fixed primary height"
    );
}
