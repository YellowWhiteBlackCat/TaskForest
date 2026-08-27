use super::*;

#[test]
fn empty_batch_counts_zero() {
    let batch = PlatformEventBatch {
        failures: Vec::new(),
        ..Default::default()
    };
    assert_eq!(batch_event_count(&batch), 0);
}
