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
