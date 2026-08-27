use super::*;

fn unique_name(tag: &str) -> String {
    format!("test_{tag}_{}", std::process::id())
}

#[test]
fn fresh_mutex_reports_not_exists_and_second_open_reports_exists() {
    let name = unique_name("mutex");
    let (guard, already) = InstanceMutex::create(&name).expect("mutex create works");
    assert!(!already, "a fresh mutex must not already exist");
    // While the first guard is alive, a second create must report that
    // the instance already exists.
    let (second, already) = InstanceMutex::create(&name).expect("mutex reopen works");
    assert!(already, "a mutex created earlier must already exist");
    // After the last guard drops, the named mutex is destroyed and a new
    // create is fresh again.
    drop(guard);
    drop(second);
    let (_fresh, already) = InstanceMutex::create(&name).expect("mutex create works");
    assert!(!already, "a destroyed mutex must not already exist");
}

#[test]
fn event_wait_returns_after_signal() {
    let name = unique_name("event");
    let event = InstanceEvent::create(&name).expect("event create works");
    signal_named_event(&name).expect("signal works");
    event.wait().expect("wait returns after the signal");
}

#[test]
fn invalid_names_are_rejected() {
    for bad in ["", "has\\backslash", "中文名", "a".repeat(65).as_str()] {
        assert!(
            matches!(
                InstanceMutex::create(bad),
                Err(WindowsApiError::InvalidInput)
            ),
            "name {bad:?}"
        );
    }
}
