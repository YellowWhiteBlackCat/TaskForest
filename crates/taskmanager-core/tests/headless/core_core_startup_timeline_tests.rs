use super::*;

fn node(
    unit: &str,
    activated_at_ms: Option<u64>,
    duration_ms: Option<u64>,
) -> StartupCriticalChainNode {
    StartupCriticalChainNode {
        unit: unit.to_owned(),
        activated_at_ms,
        duration_ms,
    }
}

#[test]
fn empty_chain_projects_to_empty_timeline() {
    let timeline = BootTimeline::from_critical_chain(&[], 20, 10);
    assert_eq!(timeline.total_ms, 0);
    assert!(timeline.segments.is_empty());
    assert_eq!(timeline.collapsed_count, 0);
    assert_eq!(timeline.untimed_count, 0);
}

#[test]
fn timed_nodes_become_sorted_segments_with_span() {
    let chain = [
        node("multi-user.target", Some(3_000), Some(2_500)),
        node("graphical.target", Some(6_000), Some(900)),
        node("dbus.service", Some(500), Some(1_200)),
    ];
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.total_ms, 6_900);
    let units: Vec<_> = timeline.segments.iter().map(|s| s.unit.as_str()).collect();
    assert_eq!(
        units,
        ["dbus.service", "multi-user.target", "graphical.target"]
    );
    assert_eq!(timeline.segments[0].start_ms, 500);
    assert_eq!(timeline.segments[0].end_ms, 1_700);
    assert_eq!(timeline.collapsed_count, 0);
    assert_eq!(timeline.untimed_count, 0);
}

#[test]
fn missing_duration_with_valid_offset_is_zero_window() {
    let chain = [node("foo.service", Some(1_000), None)];
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.segments.len(), 1);
    assert_eq!(timeline.segments[0].duration_ms, 0);
    assert_eq!(timeline.segments[0].end_ms, 1_000);
    assert_eq!(timeline.total_ms, 1_000);
    assert_eq!(timeline.untimed_count, 0);
}

#[test]
fn nodes_without_activation_are_counted_not_placed() {
    let chain = [
        node("early.service", Some(100), Some(50)),
        node("mystery.service", None, None),
        node("later.service", Some(300), Some(200)),
    ];
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.segments.len(), 2);
    assert_eq!(timeline.untimed_count, 1);
    assert_eq!(timeline.untimed_units, ["mystery.service"]);
}

#[test]
fn segment_cap_collapses_earliest_kept_stable() {
    let chain = (0..25)
        .map(|i| {
            node(
                &format!("unit{i}.service"),
                Some(u64::try_from(i).unwrap_or(0) * 100),
                Some(50),
            )
        })
        .collect::<Vec<_>>();
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.segments.len(), 20);
    assert_eq!(timeline.segments[0].unit, "unit0.service");
    assert_eq!(timeline.segments[19].unit, "unit19.service");
    assert_eq!(timeline.collapsed_count, 5);
}

#[test]
fn untimed_unit_names_are_bounded() {
    let chain = (0..15)
        .map(|i| node(&format!("u{i}.service"), None, None))
        .collect::<Vec<_>>();
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.untimed_count, 15);
    assert_eq!(timeline.untimed_units.len(), 10);
}

#[test]
fn fractions_normalize_and_clamp() {
    let chain = [
        node("a.service", Some(0), Some(500)),
        node("b.service", Some(500), Some(1_500)),
    ];
    let timeline = BootTimeline::from_critical_chain(&chain, 20, 10);
    assert_eq!(timeline.total_ms, 2_000);
    let a = timeline.fraction_of_total(&timeline.segments[0]);
    let b = timeline.fraction_of_total(&timeline.segments[1]);
    assert!((a - 0.25).abs() < f32::EPSILON);
    assert!((b - 0.75).abs() < f32::EPSILON);

    let empty = BootTimeline::default();
    assert_eq!(empty.fraction_of_total(&timeline.segments[0]), 0.0);
}
