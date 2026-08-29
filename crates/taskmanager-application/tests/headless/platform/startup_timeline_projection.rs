//! Table tests for the shared boot-timeline rows projection. The TUI and the
//! iced frontend both render through [`super::boot_timeline_rows`], so the
//! ordering/overlap, untimed, collapse, and silence semantics are tested once
//! here (ADR-020 single-source).

use super::boot_timeline_rows;
use taskmanager_core::core::startup::{
    DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS, DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupEvidenceFailure,
};

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

fn snapshot(chain: Vec<StartupCriticalChainNode>) -> StartupBootEvidenceSnapshot {
    StartupBootEvidenceSnapshot {
        critical_chain: chain,
        ..StartupBootEvidenceSnapshot::default()
    }
}

#[test]
fn typed_failure_keeps_the_waterfall_silent() {
    let mut failing = snapshot(vec![node("dbus.service", Some(500), Some(1_200))]);
    failing.critical_chain_failure = Some(StartupEvidenceFailure::MissingTool);
    assert!(
        boot_timeline_rows(&failing).is_none(),
        "a typed failure must suppress the waterfall, never render stale bars"
    );
}

#[test]
fn empty_typed_evidence_keeps_the_waterfall_silent() {
    assert!(boot_timeline_rows(&StartupBootEvidenceSnapshot::default()).is_none());
    assert!(
        boot_timeline_rows(&snapshot(Vec::new())).is_none(),
        "a true empty chain is a silent absence, not a fabricated zero-ms block"
    );
}

#[test]
fn measured_nodes_become_start_sorted_segments_with_total_span() {
    let chain = [
        node("multi-user.target", Some(3_000), Some(2_500)),
        node("graphical.target", Some(6_000), Some(900)),
        node("dbus.service", Some(500), Some(1_200)),
    ];
    let timeline =
        boot_timeline_rows(&snapshot(chain.to_vec())).expect("measured chain must project rows");
    let units: Vec<&str> = timeline
        .segments
        .iter()
        .map(|segment| segment.unit.as_str())
        .collect();
    assert_eq!(
        units,
        ["dbus.service", "multi-user.target", "graphical.target"]
    );
    assert_eq!(timeline.total_ms, 6_900);
    assert_eq!(timeline.untimed_count, 0);
    assert_eq!(timeline.collapsed_count, 0);
}

#[test]
fn overlapping_windows_are_placed_not_reordered_or_merged() {
    // Overlap is a fact of parallel activation; the projection must keep
    // every measured window (sorted by start, stable) rather than inventing a
    // serialized order.
    let chain = [
        node("later.service", Some(800), Some(400)),
        node("earlier.service", Some(100), Some(1_000)),
    ];
    let timeline =
        boot_timeline_rows(&snapshot(chain.to_vec())).expect("measured chain must project rows");
    assert_eq!(timeline.segments.len(), 2);
    assert_eq!(timeline.segments[0].unit, "earlier.service");
    assert_eq!(timeline.segments[0].end_ms, 1_100);
    assert_eq!(timeline.segments[1].unit, "later.service");
    assert_eq!(timeline.segments[1].end_ms, 1_200);
    assert_eq!(timeline.total_ms, 1_200);
    // The later-start window still overlaps the earlier one on the axis.
    assert!(timeline.segments[1].start_ms < timeline.segments[0].end_ms);
}

#[test]
fn missing_duration_with_valid_activation_is_an_activation_mark() {
    let chain = [node("foo.service", Some(1_000), None)];
    let timeline =
        boot_timeline_rows(&snapshot(chain.to_vec())).expect("activated node must project a row");
    assert_eq!(timeline.segments.len(), 1);
    assert_eq!(timeline.segments[0].duration_ms, 0);
    assert_eq!(timeline.segments[0].start_ms, 1_000);
    assert_eq!(timeline.segments[0].end_ms, 1_000);
    assert_eq!(
        timeline.untimed_count, 0,
        "an activation offset is timing data"
    );
}

#[test]
fn nodes_without_activation_are_counted_and_listed_never_placed() {
    let chain = [
        node("early.service", Some(100), Some(50)),
        node("mystery.service", None, None),
        node("network-online.target", None, Some(300)),
        node("later.service", Some(300), Some(200)),
    ];
    let timeline = boot_timeline_rows(&snapshot(chain.to_vec()))
        .expect("partially measured chain must project rows");
    assert_eq!(timeline.segments.len(), 2);
    assert_eq!(timeline.untimed_count, 2);
    assert_eq!(
        timeline.untimed_units,
        ["mystery.service", "network-online.target"]
    );
}

#[test]
fn segment_cap_collapses_the_tail_and_keeps_the_earliest_stable() {
    let chain = (0..25)
        .map(|i| {
            let i: u64 = i;
            node(&format!("unit{i}.service"), Some(i * 100), Some(50))
        })
        .collect();
    let timeline = boot_timeline_rows(&snapshot(chain)).expect("large chain must project rows");
    assert_eq!(timeline.segments.len(), DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS);
    assert_eq!(timeline.segments[0].unit, "unit0.service");
    assert_eq!(timeline.segments[19].unit, "unit19.service");
    assert_eq!(
        timeline.collapsed_count,
        25 - DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS
    );
}

#[test]
fn untimed_names_stay_bounded_even_on_a_pathological_chain() {
    let chain = (0..15)
        .map(|i| node(&format!("u{i}.service"), None, None))
        .collect();
    let timeline = boot_timeline_rows(&snapshot(chain)).expect("untimed chain must project rows");
    assert_eq!(timeline.segments.len(), 0);
    assert_eq!(timeline.untimed_count, 15);
    assert_eq!(
        timeline.untimed_units.len(),
        DEFAULT_BOOT_TIMELINE_MAX_UNTIMED
    );
}

#[test]
fn fraction_of_total_normalizes_and_clamps_to_the_measured_span() {
    let chain = [
        node("a.service", Some(0), Some(500)),
        node("b.service", Some(500), Some(1_500)),
    ];
    let timeline = boot_timeline_rows(&snapshot(chain.to_vec())).expect("measured chain");
    let a = timeline.fraction_of_total(&timeline.segments[0]);
    let b = timeline.fraction_of_total(&timeline.segments[1]);
    assert!((a - 0.25).abs() < f32::EPSILON);
    assert!((b - 0.75).abs() < f32::EPSILON);
}
