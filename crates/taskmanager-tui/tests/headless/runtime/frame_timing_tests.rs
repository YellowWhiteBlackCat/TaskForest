use super::*;
use std::time::Duration;

#[test]
fn a_dirty_draw_keeps_interval_and_execution_distinct() {
    let observation = FrameTimingObservation::new(
        3,
        Some(Duration::from_millis(120)),
        Some(Duration::from_millis(4)),
    );
    assert_eq!(observation.interval_ms, Some(120.0));
    assert_eq!(observation.execution_ms, Some(4.0));
}
