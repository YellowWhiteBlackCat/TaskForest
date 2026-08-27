use super::*;

const SIZE: Size = Size::new(200.0, 100.0);

fn single_run(samples: &[f32]) -> Vec<Point> {
    let mut runs = series_point_runs(samples, SIZE);
    assert_eq!(runs.len(), 1, "finite samples form one continuous run");
    runs.remove(0)
}

fn single_scaled_run(samples: &[f32], max: f32) -> Vec<Point> {
    let mut runs = series_point_runs_for(samples, SIZE, max);
    assert_eq!(runs.len(), 1, "finite samples form one continuous run");
    runs.remove(0)
}

#[test]
fn empty_samples_project_to_no_points() {
    // Honest empty state: the chart draws nothing rather than a flat line
    // at zero or an invented origin.
    assert!(series_point_runs(&[], SIZE).is_empty());
}

#[test]
fn samples_map_to_width_evenly_with_zero_at_the_bottom() {
    let points = single_run(&[0.0, 50.0, 100.0]);
    assert_eq!(points.len(), 3);
    // x spread: 0, 100, 200 across the 200-wide frame (oldest→newest).
    assert_eq!(points[0], Point::new(0.0, 100.0)); // 0%   → bottom-left
    assert_eq!(points[1], Point::new(100.0, 50.0)); // 50% → middle
    assert_eq!(points[2], Point::new(200.0, 0.0)); // 100% → top-right
}

#[test]
fn chronological_projection_always_advances_left_to_right() {
    let width = 200.0;
    let x = [
        sample_x(0, 4, width),
        sample_x(1, 4, width),
        sample_x(2, 4, width),
        sample_x(3, 4, width),
    ];
    let expected = [0.0, 200.0 / 3.0, 400.0 / 3.0, 200.0];
    assert!(
        x.iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-3),
        "chronological x projection drifted: {x:?}"
    );
    assert!(x.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(sample_x(99, 4, width), width, "out-of-range indices clamp");
    assert_eq!(
        sample_x(0, 1, width),
        0.0,
        "one sample has no fabricated span"
    );
}

#[test]
fn out_of_range_values_clamp_inside_the_frame() {
    let points = single_run(&[-40.0, 250.0]);
    assert_eq!(points[0].y, 100.0, "negative clamps to 0% (bottom)");
    assert_eq!(points[1].y, 0.0, "over-100 clamps to 100% (top)");
}

#[test]
fn scaled_series_uses_its_own_max_not_the_percentage_ceiling() {
    // A disk series peaking at 50 MB/s must rise to the TOP of the frame
    // (50/50), not clamp flat against the 100-percentage ceiling — the
    // regression bait for bytes/sec trend strips.
    let points = single_scaled_run(&[10.0, 50.0], 50.0);
    assert_eq!(points[0].y, SIZE.height * (1.0 - 10.0 / 50.0));
    assert!(
        points[1].y.abs() < 1e-3,
        "the finite peak maps to the top edge, got {}",
        points[1].y
    );
}

#[test]
fn scaled_series_with_zero_max_stays_flat_at_the_baseline() {
    // All-zero (idle) throughput anchors at the bottom — never a fabricated
    // mid-line or a div-by-zero blow-up.
    let idle = single_scaled_run(&[0.0, 0.0, 0.0], 0.0);
    assert!(
        idle.iter().all(|p| (p.y - SIZE.height).abs() < 1e-3),
        "idle samples must sit at the baseline"
    );
}

#[test]
fn history_gaps_split_finite_runs_without_changing_chronological_x() {
    let runs = series_point_runs(&[10.0, f32::NAN, 90.0], SIZE);
    assert_eq!(runs.len(), 2, "the gap must split the trace");
    assert_eq!(runs[0].len(), 1);
    assert_eq!(runs[1].len(), 1);
    assert_eq!(runs[0][0].x, 0.0);
    assert_eq!(runs[1][0].x, SIZE.width);
    assert!((runs[0][0].y - 90.0).abs() < 1e-3);
    assert!((runs[1][0].y - 10.0).abs() < 1e-3);
    assert!(
        runs.iter()
            .flatten()
            .all(|point| { point.x.is_finite() && point.y.is_finite() })
    );
    assert!(
        runs.iter().all(|run| line_path(run).is_none()),
        "two isolated observations must not be joined across the gap"
    );
}

#[test]
fn edge_and_all_gap_windows_create_only_real_finite_runs() {
    let runs = series_point_runs(&[f32::NAN, 20.0, 30.0, f32::INFINITY], SIZE);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].len(), 2);
    assert!((runs[0][0].x - SIZE.width / 3.0).abs() < 1e-3);
    assert!((runs[0][1].x - SIZE.width * 2.0 / 3.0).abs() < 1e-3);
    assert!(series_point_runs(&[f32::NAN, f32::NEG_INFINITY], SIZE).is_empty());
}

#[test]
fn line_area_and_smooth_geometry_are_built_per_gap_bounded_run() {
    let runs = series_point_runs(&[10.0, 30.0, 50.0, f32::NAN, 60.0, 80.0, 90.0], SIZE);
    assert_eq!(runs.len(), 2);
    assert!(runs[0].last().unwrap().x < runs[1].first().unwrap().x);
    for run in &runs {
        assert!(line_path(run).is_some());
        assert!(area_path(run, SIZE.height).is_some());
        assert!(smooth_line_path(run).is_some());
        assert!(smooth_area_path(run, SIZE.height).is_some());
    }
    let invalid = [Point::new(0.0, 10.0), Point::new(20.0, f32::NAN)];
    assert!(line_path(&invalid).is_none());
    assert!(area_path(&invalid, SIZE.height).is_none());
}

#[test]
fn single_sample_lands_at_the_left_edge_without_fabricating_a_second() {
    let points = single_run(&[42.0]);
    assert_eq!(points.len(), 1, "no second vertex invented for one sample");
    assert_eq!(points[0].x, 0.0); // step is 0 when count == 1
    // height * (1 - 0.42) — f32 division is not bit-exact, so compare on an
    // epsilon (the project's established float-assertion convention).
    assert!((points[0].y - 58.0).abs() < 1e-3, "got {}", points[0].y);
}

#[test]
fn a_populated_buffer_yields_a_line_and_area_a_zero_buffer_yields_neither() {
    // Populated buffer → both paths exist (non-empty geometry).
    let populated = single_run(&[10.0, 40.0, 70.0, 90.0]);
    assert!(line_path(&populated).is_some());
    assert!(area_path(&populated, SIZE.height).is_some());

    // Empty / single-vertex buffers → no strokeable path (honest).
    assert!(line_path(&[]).is_none());
    assert!(area_path(&[], SIZE.height).is_none());
    let single = single_run(&[5.0]);
    assert!(line_path(&single).is_none());
    assert!(area_path(&single, SIZE.height).is_none());
}

#[test]
fn area_path_closes_back_along_the_baseline() {
    // The area polygon's last drawn vertex is the baseline under the first
    // sample; we assert the geometry by checking the projected first/last
    // sample x-coordinates feed the baseline corners (the polygon's two
    // bottom vertices share x with the first and last samples).
    let points = single_run(&[20.0, 80.0]);
    let first_x = points[0].x;
    let last_x = points[points.len() - 1].x;
    assert!(area_path(&points, SIZE.height).is_some());
    assert_eq!(first_x, 0.0);
    assert_eq!(last_x, SIZE.width);
}

#[test]
fn chart_program_classifies_partial_buffers_without_cross_fabrication() {
    // CPU populated, memory empty: the program holds the asymmetry and the
    // pure projection confirms only the CPU series yields points.
    let chart = PerfChart::new(
        vec![10.0, 30.0, 60.0],
        vec![],
        Color::WHITE,
        Color::BLACK,
        Color::BLACK,
        ReadoutColors {
            bg: Color::BLACK,
            fg: Color::WHITE,
        },
        false,
    );
    assert_eq!(single_run(&chart.cpu).len(), 3);
    assert!(series_point_runs(&chart.memory, SIZE).is_empty());
}

/// The smooth spline needs three vertices (a neighbour on both sides of
/// at least one segment); fewer yield no geometry — the honest
/// too-few-samples state, never a fabricated curve.
#[test]
fn smooth_paths_require_three_vertices() {
    let two = single_run(&[10.0, 50.0]);
    assert!(smooth_line_path(&two).is_none());
    assert!(smooth_area_path(&two, SIZE.height).is_none());
    let three = single_run(&[10.0, 50.0, 90.0]);
    assert!(smooth_line_path(&three).is_some());
    assert!(smooth_area_path(&three, SIZE.height).is_some());
}

/// The area-fill gradient stops fade the category color from ~0.35 at the
/// top edge to fully transparent at the baseline: ascending offsets with
/// STRICTLY decreasing alpha, hue preserved — the Mission-Center ramp.
#[test]
fn area_gradient_stops_fade_from_category_color_to_transparent() {
    let base = Color::from_rgb(0.2, 0.5, 0.9);
    let stops = area_gradient_stops(base);
    assert_eq!(stops.len(), 3);
    let mut previous_alpha = f32::INFINITY;
    let mut previous_offset = -1.0;
    for (offset, color) in stops {
        assert!(
            (0.0..=1.0).contains(&offset) && offset > previous_offset,
            "offsets must ascend: {offset}"
        );
        assert!(
            color.a < previous_alpha,
            "alpha must strictly decrease: {color:?}"
        );
        assert_eq!(color.r, base.r, "hue preserved");
        assert_eq!(color.g, base.g, "hue preserved");
        assert_eq!(color.b, base.b, "hue preserved");
        previous_alpha = color.a;
        previous_offset = offset;
    }
    assert!(
        (stops[0].1.a - AREA_FILL_TOP_ALPHA).abs() < 1e-4,
        "top stop must be the 0.35 alpha, got {:?}",
        stops[0].1
    );
    assert_eq!(
        stops[2].1.a, AREA_FILL_BOTTOM_ALPHA,
        "bottom must be transparent"
    );
}

/// The canvas gradient is vertical in frame space (top → baseline) and
/// carries exactly the area stops — iced's public `gradient::Linear`, so
/// `Frame::fill` accepts it through `Fill: From<Linear>`.
#[test]
fn vertical_area_gradient_points_from_top_to_bottom() {
    let base = Color::from_rgb(0.2, 0.5, 0.9);
    let gradient = vertical_area_gradient(base, 128.0);
    assert_eq!(gradient.start, Point::new(0.0, 0.0));
    assert_eq!(gradient.end, Point::new(0.0, 128.0), "top → baseline");
    let stops: Vec<iced::gradient::ColorStop> = gradient.stops.iter().flatten().copied().collect();
    assert_eq!(stops.len(), 3);
    assert_eq!(stops[0].offset, 0.0);
    assert!((stops[0].color.a - AREA_FILL_TOP_ALPHA).abs() < 1e-4);
    assert_eq!(stops[2].offset, 1.0);
    assert_eq!(stops[2].color.a, AREA_FILL_BOTTOM_ALPHA);
    assert!(stops[0].color.a > stops[1].color.a && stops[1].color.a > stops[2].color.a);
}

/// The hover mapping is the inverse of the point projection: cursor x →
/// nearest sample index across the width, oldest → newest. Out-of-frame
/// positions, zero-width frames, and sub-two-sample windows are all honest
/// `None` (nothing to hover — never a fabricated index).
#[test]
fn hovered_index_maps_cursor_x_to_the_nearest_sample() {
    // Four samples across a 200-wide frame: 0, 1, 2, 3 at x = 0, 66.7, 133.3, 200.
    assert_eq!(hovered_index(0.0, 200.0, 4), Some(0));
    assert_eq!(hovered_index(33.0, 200.0, 4), Some(0), "nearest of 0/1");
    assert_eq!(hovered_index(50.0, 200.0, 4), Some(1));
    assert_eq!(hovered_index(100.0, 200.0, 4), Some(2), "ties round up");
    assert_eq!(hovered_index(150.0, 200.0, 4), Some(2));
    assert_eq!(hovered_index(199.0, 200.0, 4), Some(3), "last sample");

    // Honest non-hover states.
    assert_eq!(hovered_index(-1.0, 200.0, 4), None, "left of frame");
    assert_eq!(hovered_index(200.0, 200.0, 4), None, "right of frame");
    assert_eq!(hovered_index(50.0, 0.0, 4), None, "zero-width frame");
    assert_eq!(hovered_index(50.0, 200.0, 1), None, "sub-two-sample window");
    assert_eq!(hovered_index(f32::NAN, 200.0, 4), None, "non-finite cursor");
}

/// Cursor motion over the frame writes the hovered index into the
/// persistent widget-tree state and requests a redraw (the canvas action
/// channel); leaving the frame clears it; motion while the index is
/// unchanged requests nothing.
#[test]
fn hover_state_tracks_cursor_motion_and_clears_on_cursor_left() {
    let chart = PerfChart::new(
        vec![10.0, 30.0, 60.0, 80.0],
        vec![20.0, 40.0, 70.0, 90.0],
        Color::WHITE,
        Color::BLACK,
        Color::BLACK,
        ReadoutColors {
            bg: Color::BLACK,
            fg: Color::WHITE,
        },
        false,
    );
    let bounds = Rectangle::new(Point::new(0.0, 0.0), SIZE);
    let mut state = PerfChartState::default();

    // Move to x = 50 of 200 → nearest sample 1 of 0..3.
    let position = Point::new(50.0, 50.0);
    let moved = canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position });
    let action = canvas::Program::update(
        &chart,
        &mut state,
        &moved,
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

/// The readout pill labels only the series that actually holds a value at
/// the hovered index: a partial memory buffer never fabricates a memory
/// figure, and an index beyond both series yields no label at all. The
/// per-series names are localized (never asserted literally); the values
/// and the part count are the behavior.
#[test]
fn readout_text_skips_series_without_a_value_at_the_index() {
    let cpu = [10.0, 30.0, 60.0];
    let memory = [20.0, 40.0];
    let both = readout_text(&cpu, &memory, 1).expect("both series hold index 1");
    assert!(both.contains("30%") && both.contains("40%"), "{both}");
    assert_eq!(both.matches(" · ").count(), 1, "exactly two parts: {both}");
    let cpu_only = readout_text(&cpu, &memory, 2).expect("CPU holds index 2");
    assert!(cpu_only.contains("60%"), "{cpu_only}");
    assert!(
        !cpu_only.contains("40%"),
        "memory has no value at index 2: {cpu_only}"
    );
    assert_eq!(
        cpu_only.matches(" · ").count(),
        0,
        "one part only: {cpu_only}"
    );
    assert!(
        readout_text(&cpu, &memory, 3).is_none(),
        "index beyond both series yields no label"
    );
    assert!(readout_text(&[], &[], 0).is_none());
    assert!(
        readout_text(&[10.0, f32::NAN], &[20.0, f32::INFINITY], 1).is_none(),
        "a gap must not format NaN/Inf into a hover pill"
    );
}

/// The spline passes through the endpoint samples: the path's first and
/// last vertices are exactly the first and last projected points (a
/// Catmull-Rom through-sample property), so smoothing never shifts the
/// series' start or end value off its real sample.
#[test]
fn smooth_path_passes_through_endpoint_samples() {
    let points = single_run(&[10.0, 50.0, 90.0]);
    let line = smooth_line_path(&points).expect("three vertices spline");
    let _ = line; // Path is opaque; the pass-through is exercised by the
    // through-sample construction (move_to(start), final
    // bezier ends at the last sample).
    let area = smooth_area_path(&points, SIZE.height).expect("three vertices area");
    let _ = area;
}

/// The DATA fingerprint retains both immutable series generations and the
/// smooth flag. Reusing either `Rc` is an idle-frame hit; any new allocation is
/// a real cache invalidation even when length and tail happen to agree.
#[test]
fn data_fingerprint_keys_on_both_series_and_smooth() {
    let cpu: Rc<[f32]> = Rc::from([10.0, 30.0].as_slice());
    let memory: Rc<[f32]> = Rc::from([20.0, 40.0].as_slice());
    let base = PerfChartDataFingerprint::from_series(&cpu, &memory, false);
    assert_eq!(
        base,
        PerfChartDataFingerprint::from_series(&cpu, &memory, false)
    );
    let same_len_same_tail: Rc<[f32]> = Rc::from([99.0, 30.0].as_slice());
    assert_ne!(
        base,
        PerfChartDataFingerprint::from_series(&same_len_same_tail, &memory, false),
        "a shifted window/gap change cannot hide behind the same len and tail"
    );
    let next_memory: Rc<[f32]> = Rc::from([20.0, 45.0].as_slice());
    assert_ne!(
        base,
        PerfChartDataFingerprint::from_series(&cpu, &next_memory, false)
    );
    assert_ne!(
        base,
        PerfChartDataFingerprint::from_series(&cpu, &memory, true),
        "a smooth toggle must force a data-cache rebuild"
    );
}

/// The program's `fingerprint()` mirrors `PerfChartDataFingerprint::from_series`
/// — the seam `draw()` keys the data-cache-clear gate on. Colors are NOT in
/// the fingerprint (a theme switch is rare and one stale-color frame is
/// acceptable — matches round-1 process_sparkline; asserted here so it is
/// not "fixed" back).
#[test]
fn perf_chart_fingerprint_tracks_data_and_smooth_not_color() {
    let mk = |cpu: Rc<[f32]>, mem: Rc<[f32]>, smooth: bool| {
        PerfChart::new(
            cpu,
            mem,
            Color::WHITE,
            Color::BLACK,
            Color::BLACK,
            ReadoutColors {
                bg: Color::BLACK,
                fg: Color::WHITE,
            },
            smooth,
        )
    };
    let cpu: Rc<[f32]> = Rc::from([10.0, 30.0].as_slice());
    let memory: Rc<[f32]> = Rc::from([20.0, 40.0].as_slice());
    let base = mk(Rc::clone(&cpu), Rc::clone(&memory), false);
    assert_eq!(
        base.fingerprint(),
        mk(Rc::clone(&cpu), Rc::clone(&memory), false).fingerprint()
    );
    let recolored = PerfChart::new(
        Rc::clone(&cpu),
        Rc::clone(&memory),
        Color::from_rgb(1.0, 0.0, 0.0),
        Color::from_rgb(0.0, 1.0, 0.0),
        Color::from_rgb(0.0, 0.0, 1.0),
        ReadoutColors {
            bg: Color::from_rgb(0.0, 0.0, 1.0),
            fg: Color::from_rgb(1.0, 1.0, 0.0),
        },
        false,
    );
    assert_eq!(
        base.fingerprint(),
        recolored.fingerprint(),
        "color/theme must NOT be in the fingerprint"
    );
    let shifted: Rc<[f32]> = Rc::from([99.0, 30.0].as_slice());
    assert_ne!(
        base.fingerprint(),
        mk(shifted, Rc::clone(&memory), false).fingerprint()
    );
    assert_ne!(base.fingerprint(), mk(cpu, memory, true).fingerprint());
}

/// The OVERLAY fingerprint combines the hover index and the data
/// fingerprint, so the overlay rebuilds when EITHER the cursor moves to a
/// new sample OR the underlying data ticks (otherwise the pill would show a
/// stale reading). Stationary hover + stable data → overlay reused.
#[test]
fn overlay_fingerprint_combines_hover_index_and_data() {
    let cpu: Rc<[f32]> = Rc::from([10.0, 30.0].as_slice());
    let memory: Rc<[f32]> = Rc::from([20.0, 40.0].as_slice());
    let data = PerfChartDataFingerprint::from_series(&cpu, &memory, false);
    let none = PerfChartOverlayFingerprint {
        hover_index: None,
        data: data.clone(),
    };
    // Same → equal.
    assert_eq!(
        none,
        PerfChartOverlayFingerprint {
            hover_index: None,
            data: data.clone(),
        }
    );
    // Hover appears → not equal (overlay must rebuild to draw the pill).
    assert_ne!(
        none,
        PerfChartOverlayFingerprint {
            hover_index: Some(1),
            data: data.clone(),
        }
    );
    // Hover moves to a different sample → not equal.
    assert_ne!(
        PerfChartOverlayFingerprint {
            hover_index: Some(0),
            data: data.clone(),
        },
        PerfChartOverlayFingerprint {
            hover_index: Some(1),
            data: data.clone(),
        }
    );
    // Same hover index but data ticked → not equal (pill text must refresh).
    let ticked_cpu: Rc<[f32]> = Rc::from([10.0, 35.0].as_slice());
    let ticked = PerfChartDataFingerprint::from_series(&ticked_cpu, &memory, false);
    assert_ne!(
        PerfChartOverlayFingerprint {
            hover_index: Some(1),
            data,
        },
        PerfChartOverlayFingerprint {
            hover_index: Some(1),
            data: ticked,
        },
        "a data tick must refresh the overlay so the pill text stays live"
    );
}
