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

#[cfg(target_os = "macos")]
#[test]
fn primary_guard_drops_without_waiting_for_a_client() {
    let name = format!("testdrop{}", std::process::id());
    let (tx, _rx) = std::sync::mpsc::channel();
    let role = acquire_single_instance(&name, tx).expect("macOS instance acquisition works");
    let InstanceRole::Primary(guard) = role else {
        panic!("the first macOS acquisition must be primary");
    };
    drop(guard);
    assert!(
        !socket_path(&name).exists(),
        "dropping the primary removes the socket"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn primary_guard_drops_after_an_idle_local_client() {
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    let name = format!("testidle{}", std::process::id());
    let (tx, _rx) = std::sync::mpsc::channel();
    let role = acquire_single_instance(&name, tx).expect("macOS instance acquisition works");
    let InstanceRole::Primary(guard) = role else {
        panic!("the first macOS acquisition must be primary");
    };
    let client = UnixStream::connect(socket_path(&name)).expect("primary socket is listening");
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(guard);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "an idle local client must not make singleton teardown unbounded"
    );
    drop(client);
}
