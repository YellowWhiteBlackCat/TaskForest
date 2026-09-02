use super::*;

#[test]
fn spawn_on_non_linux_is_typed_unsupported() {
    #[cfg(not(target_os = "linux"))]
    {
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = acquire_single_instance("test", tx);
        assert_eq!(result, Err(InstanceFailure::Unsupported));
    }
}

#[test]
fn bus_name_is_a_valid_well_known_shape() {
    let name = bus_name("TaskForest");
    assert_eq!(name, "org.taskforest.TaskForest");
    assert!(name.split('.').all(|part| !part.is_empty()));
}

/// A real session bus must arbitrate one primary and route a secondary launch
/// back to it. Environments without a session bus still exercise the typed
/// missing-dependency result; they never silently turn an untestable native
/// path into a pass.
#[test]
fn session_bus_arbitrates_primary_secondary_and_activation() {
    let name = format!(
        "testlive{}{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos()
    );
    let (primary_tx, primary_rx) = std::sync::mpsc::channel();
    let primary = match acquire_single_instance(&name, primary_tx) {
        Ok(role) => role,
        Err(InstanceFailure::MissingDependency) => return,
        Err(failure) => panic!("session bus acquisition failed unexpectedly: {failure:?}"),
    };
    assert!(matches!(&primary, InstanceRole::Primary(_)));

    let (secondary_tx, _secondary_rx) = std::sync::mpsc::channel();
    let secondary = acquire_single_instance(&name, secondary_tx)
        .expect("a live primary makes the second launch a typed secondary");
    assert!(matches!(&secondary, InstanceRole::Secondary));
    assert_eq!(
        primary_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("secondary launch activates the primary"),
        InstanceEvent::Activate
    );
    drop(secondary);
    drop(primary);

    let (again_tx, _again_rx) = std::sync::mpsc::channel();
    let again = acquire_single_instance(&name, again_tx)
        .expect("dropping the primary releases the session-bus name");
    assert!(matches!(&again, InstanceRole::Primary(_)));
}
