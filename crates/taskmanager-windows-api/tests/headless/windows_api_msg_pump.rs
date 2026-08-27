use super::*;

#[test]
fn pump_on_idle_thread_dispatch_zero() {
    let dispatched = pump_pending_messages().expect("pump is available on Windows");
    assert_eq!(dispatched, 0);
}

#[test]
fn pump_never_exceeds_its_bounded_cap() {
    for _ in 0..3 {
        let dispatched = pump_pending_messages().expect("pump is available on Windows");
        assert!(dispatched <= MAX_PUMPED_MESSAGES_PER_CALL);
    }
}
