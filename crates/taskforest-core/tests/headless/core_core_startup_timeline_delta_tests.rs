use super::*;

fn segment(unit: &str, duration_ms: u64) -> BootTimelineSegment {
    BootTimelineSegment {
        unit: unit.to_owned(),
        start_ms: 0,
        end_ms: duration_ms,
        duration_ms,
    }
}

fn timeline(segments: &[BootTimelineSegment]) -> BootTimeline {
    BootTimeline {
        total_ms: segments.iter().map(|s| s.duration_ms).max().unwrap_or(0),
        segments: segments.to_vec(),
        collapsed_count: 0,
        untimed_count: 0,
        untimed_units: Vec::new(),
    }
}

#[test]
fn segment_deltas_match_units_and_keep_the_current_order() {
    let current = timeline(&[
        segment("network.service", 800),
        segment("dev-node.service", 1_200),
    ]);
    let previous = timeline(&[
        segment("dev-node.service", 1_000),
        segment("network.service", 900),
    ]);
    let deltas = segment_deltas(&current, &previous);
    assert_eq!(
        deltas
            .iter()
            .map(|delta| (delta.unit.as_str(), delta.delta_ms))
            .collect::<Vec<_>>(),
        vec![("network.service", -100), ("dev-node.service", 200)],
        "matched units only, in the CURRENT timeline's order"
    );
}

#[test]
fn units_without_a_counterpart_are_skipped_not_fabricated() {
    let current = timeline(&[segment("only-now.service", 5), segment("both.service", 7)]);
    let previous = timeline(&[
        segment("both.service", 7),
        segment("only-before.service", 9),
    ]);
    let deltas = segment_deltas(&current, &previous);
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].unit, "both.service");
    assert_eq!(deltas[0].delta_ms, 0);
    assert_eq!(deltas[0].current_ms, deltas[0].previous_ms);
}
