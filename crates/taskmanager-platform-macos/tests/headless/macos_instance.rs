use super::*;

#[test]
fn spawn_on_non_macos_is_typed_unsupported() {
    #[cfg(not(target_os = "macos"))]
    {
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = acquire_single_instance("test", tx);
        assert!(matches!(result, Err(InstanceFailure::Unsupported)));
    }
}

#[test]
fn socket_path_is_bounded_and_namespaced() {
    let path = socket_path("TaskForest");
    let file_name = path.file_name().unwrap().to_string_lossy();
    assert!(file_name.starts_with("taskmanager."), "file: {file_name}");
    assert!(file_name.ends_with(".sock"), "file: {file_name}");
    assert!(file_name.contains("TaskForest"));
}
