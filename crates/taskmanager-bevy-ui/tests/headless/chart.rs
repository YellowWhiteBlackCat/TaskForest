use super::*;

#[test]
fn chart_projection_maps_extrema_and_keeps_newest_at_right_edge() {
    let segments = line_segments(&[1.0, 3.0, 2.0], 100.0, 40.0, MAX_CHART_POINTS);
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].start, ChartVertex { x: 0.0, y: 40.0 });
    assert_eq!(segments[0].end, ChartVertex { x: 50.0, y: 0.0 });
    assert_eq!(segments[1].end, ChartVertex { x: 100.0, y: 20.0 });
}

#[test]
fn unavailable_samples_break_the_line_instead_of_becoming_zero() {
    let segments = line_segments(&[1.0, f32::NAN, 3.0, 4.0], 90.0, 30.0, MAX_CHART_POINTS);
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].start.x, 60.0);
    assert!((segments[0].start.y - 10.0).abs() < 1e-4);
    assert_eq!(segments[0].end, ChartVertex { x: 90.0, y: 0.0 });
}

#[test]
fn constant_and_all_unavailable_windows_stay_honest_and_bounded() {
    let constant = line_segments(&[2.0, 2.0, 2.0], 60.0, 20.0, MAX_CHART_POINTS);
    assert!(
        constant
            .iter()
            .all(|segment| { segment.start.y == 10.0 && segment.end.y == 10.0 })
    );
    assert!(line_segments(&[f32::NAN, f32::INFINITY], 60.0, 20.0, MAX_CHART_POINTS).is_empty());

    let samples = (0..(MAX_CHART_POINTS + 17))
        .map(|value| value as f32)
        .collect::<Vec<_>>();
    let bounded = line_segments(&samples, 60.0, 20.0, MAX_CHART_POINTS);
    assert_eq!(bounded.len(), MAX_CHART_POINTS - 1);
    assert_eq!(bounded[0].start.x, 0.0);
    assert_eq!(bounded[0].start.y, 20.0);
}

#[test]
fn zero_budget_does_not_accidentally_render_a_sample() {
    assert!(line_segments(&[1.0], 10.0, 10.0, 0).is_empty());
}

// ---- polyline render adapter: segment geometry is the drawn truth --------

#[test]
fn segment_layout_aims_each_rectangle_from_start_to_end() {
    // A 45° down-right segment: the rotated 2px rectangle's center sits on
    // the midpoint, its length is the endpoint distance, and its clockwise
    // rotation is +45° (screen y grows downward).
    let diagonal = segment_layout(ChartSegment {
        start: ChartVertex { x: 0.0, y: 0.0 },
        end: ChartVertex { x: 40.0, y: 40.0 },
    });
    assert!((diagonal.length - 40.0 * std::f32::consts::SQRT_2).abs() < 1e-4);
    assert!((diagonal.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
    let center = (diagonal.left + diagonal.length / 2.0, diagonal.top + 1.0);
    assert!((center.0 - 20.0).abs() < 1e-4 && (center.1 - 20.0).abs() < 1e-4);

    // A horizontal segment has zero rotation; an upward segment (a rising
    // sample, screen y shrinking) rotates counter-clockwise (negative).
    let flat = segment_layout(ChartSegment {
        start: ChartVertex { x: 5.0, y: 9.0 },
        end: ChartVertex { x: 25.0, y: 9.0 },
    });
    assert!(flat.rotation.abs() < 1e-6);
    let rising = segment_layout(ChartSegment {
        start: ChartVertex { x: 0.0, y: 30.0 },
        end: ChartVertex { x: 10.0, y: 10.0 },
    });
    assert!(rising.rotation < 0.0, "rising values rotate upward");

    // Two coincident observations still draw a 1px mark: a real sample pair
    // is never rendered as nothing.
    let degenerate = segment_layout(ChartSegment {
        start: ChartVertex { x: 7.0, y: 7.0 },
        end: ChartVertex { x: 7.0, y: 7.0 },
    });
    assert_eq!(degenerate.length, 1.0);
}
