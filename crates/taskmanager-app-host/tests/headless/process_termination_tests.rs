use super::*;

#[test]
fn installing_the_termination_handler_twice_shares_the_existing_observation() {
    let first = ProcessTermination::install().expect("first install must succeed");
    // The native handler registry admits exactly one closure per process:
    // before this was made idempotent, the second install failed (or
    // replaced the handler) and the first handle could never observe a
    // signal. Both calls must now succeed over the same process-wide flag.
    let second = ProcessTermination::install()
        .expect("second install must observe the already-installed handler");
    assert!(
        !first.is_requested(),
        "a fresh process must not report termination"
    );
    assert!(
        !second.is_requested(),
        "the shared observation must start untriggered"
    );
}
