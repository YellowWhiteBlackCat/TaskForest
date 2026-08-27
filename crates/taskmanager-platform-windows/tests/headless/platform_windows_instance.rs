use super::*;

#[test]
fn spawn_on_non_windows_is_typed_unsupported() {
    #[cfg(not(windows))]
    {
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = acquire_single_instance("test", tx);
        assert!(matches!(result, Err(InstanceFailure::Unsupported)));
    }
}

#[cfg(windows)]
#[test]
fn first_acquire_is_primary_and_second_is_secondary() {
    let name = format!("test_{}", std::process::id());
    let (tx, rx) = std::sync::mpsc::channel();
    let primary = acquire_single_instance(&name, tx).expect("first acquire works");
    assert!(matches!(primary, InstanceRole::Primary(_)));

    let (tx2, _rx2) = std::sync::mpsc::channel();
    let secondary = acquire_single_instance(&name, tx2).expect("second acquire must not fail");
    assert!(matches!(secondary, InstanceRole::Secondary));

    // Dropping the primary guard releases the instance; a fresh acquire
    // is primary again.
    drop(primary);
    let (tx3, _rx3) = std::sync::mpsc::channel();
    let again = acquire_single_instance(&name, tx3).expect("reacquire works");
    assert!(matches!(again, InstanceRole::Primary(_)));
    drop(again);
    let _ = rx.try_recv();
}

#[cfg(windows)]
#[test]
fn primary_guard_delivers_activation_events() {
    let name = format!("test_act_{}", std::process::id());
    let (tx, rx) = std::sync::mpsc::channel();
    let primary = acquire_single_instance(&name, tx).expect("acquire works");
    let InstanceRole::Primary(_guard) = &primary else {
        panic!("must be primary");
    };
    // Ask another "instance" to activate this one: signal the event.
    signal_named_event(&native_event_name(&name)).expect("signal works");
    // The helper thread forwards the activation event.
    let event = rx.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(event, Ok(InstanceEvent::Activate));
    drop(primary);
}
