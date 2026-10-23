//! Headless behavior tests for the boot timeline gantt (ICED-044): the
//! axis carries tick lines, every measured window is PLACED at its
//! activation offset, and the unit labels are staggered into lanes so
//! concurrently starting units never wear their names on top of each other.

use super::*;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::startup::{StartupBootEvidenceSnapshot, StartupCriticalChainNode};

const TRACK_WIDTH: f32 = TIMELINE_TRACK_WIDTH;
const FONT_SIZE: f32 = 11.0;

fn node(unit: &str, activated_at_ms: u64, duration_ms: u64) -> StartupCriticalChainNode {
    StartupCriticalChainNode {
        unit: unit.into(),
        activated_at_ms: Some(activated_at_ms),
        duration_ms: Some(duration_ms),
    }
}

/// Three windows: two that start within the same instant and one that
/// starts much later.
fn concurrent_evidence() -> StartupBootEvidenceSnapshot {
    let healthy = DeviceState::healthy(1_785_292_800_000);
    StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        // The fixture carries no boot phase split.
        phase_timeline_state: healthy,
        phase_timeline_failure: None,
        phase_timeline: None,
        failed_units: Vec::new(),
        critical_chain: vec![
            node("alpha.service", 100, 500),
            node("beta.service", 150, 500),
            node("gamma.service", 2_000, 100),
        ],
    }
}

/// The axis' tick lines sit on "nice" time steps and never leave the
/// measured span.
#[test]
fn axis_carries_tick_lines_on_nice_steps_inside_the_measured_span() {
    let interval = timeline_tick_interval(4_200, TIMELINE_TICK_TARGET);
    assert_eq!(
        interval, 1_000,
        "the tick step is a 1/2/5 × 10ⁿ step, never a fractional millisecond"
    );
    let ticks = timeline_ticks(4_200, interval);
    assert!(!ticks.is_empty(), "a measured span carries tick lines");
    for pair in ticks.windows(2) {
        assert!(pair[0].at_ms < pair[1].at_ms, "the ticks advance in time");
        assert!(
            pair[1].fraction > pair[0].fraction,
            "the ticks advance on the axis"
        );
    }
    assert!(ticks.iter().all(|tick| tick.at_ms <= 4_200));
    assert_eq!(ticks.last().map(|tick| tick.at_ms), Some(4_000));
}

/// A span that was never measured carries no ticks: no invented axis.
#[test]
fn an_unmeasured_span_carries_no_tick_lines() {
    assert_eq!(timeline_tick_interval(0, TIMELINE_TICK_TARGET), 0);
    assert!(timeline_ticks(0, 1_000).is_empty());
}

/// Every measured window is PLACED at its activation offset, not stacked at
/// the axis' origin.
#[test]
fn measured_windows_are_placed_at_their_activation_offset() {
    let gantt = timeline_gantt(Some(&concurrent_evidence()), TRACK_WIDTH, FONT_SIZE)
        .expect("measured evidence projects a gantt");
    assert_eq!(gantt.total_ms, 2_100);
    assert_eq!(gantt.windows.len(), 3);
    let alpha = &gantt.windows[0];
    let beta = &gantt.windows[1];
    let gamma = &gantt.windows[2];
    assert!(
        alpha.bar_left < beta.bar_left,
        "a later activation starts further right"
    );
    assert!(
        beta.bar_left < gamma.bar_left,
        "a much later activation starts much further right"
    );
    // The bar's own width is the duration's share of the measured span.
    let expected_alpha = (500.0 / 2_100.0) * TRACK_WIDTH;
    assert!(
        (alpha.bar_width - expected_alpha).abs() < 0.01,
        "the bar is sized by the duration's share, got {}",
        alpha.bar_width
    );
}

/// Two units that start within the same instant wear DIFFERENT lanes: the
/// staggered label row is what keeps their names apart.
#[test]
fn concurrent_units_get_staggered_label_lanes() {
    let gantt = timeline_gantt(Some(&concurrent_evidence()), TRACK_WIDTH, FONT_SIZE)
        .expect("measured evidence projects a gantt");
    let alpha = &gantt.labels[0];
    let beta = &gantt.labels[1];
    assert_ne!(
        alpha.lane, beta.lane,
        "concurrently starting units must not share a label lane"
    );
    assert!(
        gantt.lanes >= 2,
        "the staggered band needs at least the base lane and one more"
    );
}

/// Within one lane no two labels overlap: the lane assignment is what
/// guarantees the names never collide.
#[test]
fn labels_in_one_lane_never_overlap() {
    let gantt = timeline_gantt(Some(&concurrent_evidence()), TRACK_WIDTH, FONT_SIZE)
        .expect("measured evidence projects a gantt");
    for lane in 0..gantt.lanes {
        let mut in_lane: Vec<&TimelineLabel> = gantt
            .labels
            .iter()
            .filter(|label| label.lane == lane)
            .collect();
        in_lane.sort_by(|left, right| left.left.total_cmp(&right.left));
        for pair in in_lane.windows(2) {
            assert!(
                pair[0].left + pair[0].width <= pair[1].left + 0.01,
                "lane {lane}: labels must not overlap ({:?} then {:?})",
                pair[0],
                pair[1]
            );
        }
    }
}

/// A label never runs past the axis it sits on: a long unit name is
/// ellipsized to the space that is left.
#[test]
fn labels_stay_inside_the_track() {
    let mut evidence = concurrent_evidence();
    evidence.critical_chain.push(node(
        "systemd-networkd-wait-online-extra-long-unit-name.service",
        2_050,
        40,
    ));
    let gantt = timeline_gantt(Some(&evidence), TRACK_WIDTH, FONT_SIZE)
        .expect("measured evidence projects a gantt");
    for label in &gantt.labels {
        assert!(label.left >= 0.0, "a label starts on the axis");
        assert!(
            label.left + label.width <= TRACK_WIDTH + 0.01,
            "a label stays inside the track: {:?}",
            label
        );
    }
}

/// The gantt stays silent without typed evidence — no invented axis.
#[test]
fn the_gantt_stays_silent_without_typed_evidence() {
    assert!(timeline_gantt(None, TRACK_WIDTH, FONT_SIZE).is_none());
}
