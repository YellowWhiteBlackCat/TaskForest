use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use taskmanager_application::window_capture::{
    WindowCaptureController, WindowCaptureDisposition, WindowCaptureErrorKind,
    WindowCaptureOutcome, WindowCaptureState, WindowCaptureTarget,
};
use taskmanager_platform_contract::{
    WindowCaptureBackend, WindowCaptureFailure, WindowCaptureFailureKind, WindowCaptureReceipt,
};

use super::*;

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const ONE_PIXEL_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn test_directory(label: &str) -> std::path::PathBuf {
    let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    crate::test_support::repo_temp_dir()
        .join(format!("taskforest-window-capture-{label}-{sequence}"))
}

fn request(
    path: std::path::PathBuf,
) -> taskmanager_application::window_capture::WindowCaptureRequest {
    let mut controller = WindowCaptureController::new();
    controller
        .begin(WindowCaptureTarget::path(path))
        .expect("request")
}

#[test]
fn worker_validates_and_atomically_publishes_one_png() {
    let directory = test_directory("publish");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(|stage| {
        fs::write(stage, ONE_PIXEL_PNG).expect("write staged PNG");
        Ok(WindowCaptureReceipt::new(
            1,
            1,
            WindowCaptureBackend::SpectacleActiveWindow,
        ))
    }))
    .expect("worker");
    let mut client = coordinator.client();
    let mut controller = WindowCaptureController::new();
    let request = controller
        .begin(WindowCaptureTarget::path(&destination))
        .expect("request");
    client.try_submit(request.clone()).expect("accepted");
    assert_eq!(
        controller.mark_running(request.id()),
        WindowCaptureDisposition::Applied
    );
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    client.outstanding = client.outstanding.saturating_sub(1);
    assert_eq!(
        controller.complete(completion),
        WindowCaptureDisposition::Applied
    );
    assert!(matches!(
        controller.state(),
        WindowCaptureState::Ready {
            width: 1,
            height: 1,
            backend: WindowCaptureBackend::SpectacleActiveWindow,
            ..
        }
    ));
    assert_eq!(
        fs::read(&destination).expect("published PNG"),
        ONE_PIXEL_PNG
    );
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 1);
    drop(client);
    drop(coordinator);
    fs::remove_file(&destination).expect("remove published PNG");
    fs::remove_dir(&directory).expect("remove isolated directory");
}

#[test]
fn native_failure_removes_stage_and_preserves_existing_png() {
    let directory = test_directory("failure");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    let original = b"existing png bytes";
    fs::write(&destination, original).expect("write original");
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(|stage| {
        fs::write(stage, b"partial output").expect("write partial stage");
        Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::ProviderFault,
            "provider failed",
        ))
    }))
    .expect("worker");
    let mut client = coordinator.client();
    let request = request(destination.clone());
    client.try_submit(request).expect("accepted");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    client.outstanding = client.outstanding.saturating_sub(1);
    assert!(matches!(
        completion.outcome,
        WindowCaptureOutcome::Failed(ref error)
            if error.kind() == WindowCaptureErrorKind::Native(WindowCaptureFailureKind::ProviderFault)
    ));
    assert_eq!(fs::read(&destination).expect("original PNG"), original);
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 1);
    drop(client);
    drop(coordinator);
    fs::remove_file(&destination).expect("remove original PNG");
    fs::remove_dir(&directory).expect("remove isolated directory");
}

#[test]
fn invalid_destination_is_rejected_before_native_capture() {
    let directory = test_directory("inspect");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    fs::create_dir(&destination).expect("create conflicting directory");
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_by_worker = Arc::clone(&called);
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(move |_| {
        called_by_worker.store(true, Ordering::Release);
        Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::ProviderFault,
            "must not run",
        ))
    }))
    .expect("worker");
    let mut client = coordinator.client();
    client
        .try_submit(request(destination.clone()))
        .expect("accepted for async validation");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    assert!(matches!(
        completion.outcome,
        WindowCaptureOutcome::Failed(ref error) if error.kind() == WindowCaptureErrorKind::Inspect
    ));
    assert!(!called.load(Ordering::Acquire));
    drop(client);
    drop(coordinator);
    fs::remove_dir(&destination).expect("remove conflicting directory");
    fs::remove_dir(&directory).expect("remove isolated directory");
}

#[test]
fn malformed_png_is_rejected_as_stage_failure_and_cleaned_up() {
    let directory = test_directory("malformed");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(|stage| {
        fs::write(stage, b"not a png").expect("write malformed stage");
        Ok(WindowCaptureReceipt::new(
            1,
            1,
            WindowCaptureBackend::SpectacleActiveWindow,
        ))
    }))
    .expect("worker");
    let mut client = coordinator.client();
    client
        .try_submit(request(destination.clone()))
        .expect("accepted");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("completion");
    client.outstanding = client.outstanding.saturating_sub(1);
    assert!(matches!(
        completion.outcome,
        WindowCaptureOutcome::Failed(ref error) if error.kind() == WindowCaptureErrorKind::Stage
    ));
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    drop(client);
    drop(coordinator);
    fs::remove_dir(&directory).expect("remove isolated directory");
}

#[test]
fn worker_panic_is_reported_as_a_typed_stopped_completion() {
    let directory = test_directory("panic");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(|_| {
        panic!("fixture worker panic");
    }))
    .expect("worker");
    let mut client = coordinator.client();
    client
        .try_submit(request(destination.clone()))
        .expect("accepted");
    let completion = client
        .completion_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("panic completion");
    client.outstanding = client.outstanding.saturating_sub(1);
    assert!(matches!(
        completion.outcome,
        WindowCaptureOutcome::Failed(ref error)
            if error.kind() == WindowCaptureErrorKind::WorkerStopped
    ));
    assert!(!destination.exists());
    drop(client);
    drop(coordinator);
    fs::remove_dir(&directory).expect("remove isolated directory");
}

#[test]
fn blocked_provider_does_not_make_coordinator_drop_wait_unboundedly() {
    let directory = test_directory("shutdown");
    fs::create_dir(&directory).expect("create isolated directory");
    let destination = directory.join("window.png");
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let finished = Arc::new(AtomicBool::new(false));
    let entered_by_worker = Arc::clone(&entered);
    let release_by_worker = Arc::clone(&release);
    let finished_by_worker = Arc::clone(&finished);
    let coordinator = WindowCaptureCoordinator::start_with_executor(Arc::new(move |_| {
        entered_by_worker.wait();
        release_by_worker.wait();
        finished_by_worker.store(true, Ordering::Release);
        Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::ProviderFault,
            "blocked fixture released",
        ))
    }))
    .expect("worker");
    let mut client = coordinator.client();
    client
        .try_submit(request(destination.clone()))
        .expect("accepted");
    entered.wait();

    drop(client);
    let started = std::time::Instant::now();
    drop(coordinator);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "coordinator drop must remain bounded while a native provider is blocked"
    );

    release.wait();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !finished.load(Ordering::Acquire)
        || fs::read_dir(&directory)
            .expect("directory")
            .next()
            .is_some()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "released worker must finish and clean its staging file"
        );
        std::thread::yield_now();
    }
    assert!(!destination.exists());
    fs::remove_dir(&directory).expect("remove isolated directory");
}
