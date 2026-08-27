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
