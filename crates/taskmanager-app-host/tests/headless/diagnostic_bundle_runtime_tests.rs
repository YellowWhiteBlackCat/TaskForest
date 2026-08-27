use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_core::{DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource};

use super::*;

fn plan(contents: &str) -> DiagnosticBundlePlan {
    DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "facts.txt".into(),
            contents: contents.into(),
        }],
        [],
    )
    .expect("plan")
}

fn test_directory(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp")
        .join(format!(
            "taskforest-diagnostic-{label}-{}-{stamp}",
            std::process::id()
        ))
}

fn wait_for_completion(
    session: &mut DiagnosticBundleSession<DiagnosticBundleClient>,
) -> taskmanager_application::DiagnosticBundleCompletion {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(completion) = session.drain().into_iter().next() {
            return completion;
        }
        assert!(Instant::now() < deadline, "completion timed out");
        std::thread::yield_now();
    }
}

#[test]
fn app_host_worker_publishes_sanitized_bundle_transactionally() {
    let directory = test_directory("write");
    std::fs::create_dir(&directory).expect("directory");
    let destination = directory.join("bundle.json");
    let coordinator = DiagnosticBundleCoordinator::start().expect("worker");
    let mut session = DiagnosticBundleSession::new(coordinator.client());
    session
        .submit(
            plan("safe facts"),
            DiagnosticBundleTarget::path(&destination),
        )
        .expect("submit");
    let completion = wait_for_completion(&mut session);
    assert_eq!(completion.destination, destination);
    completion.result.expect("write");
    assert!(
        std::fs::read_to_string(&destination)
            .expect("bundle")
            .contains("safe facts")
    );
    drop(session);
    drop(coordinator);
    std::fs::remove_dir_all(&directory).expect("cleanup");
}

#[test]
fn filesystem_failure_remains_typed() {
    let directory = test_directory("missing-parent");
    let destination = directory.join("bundle.json");
    let coordinator = DiagnosticBundleCoordinator::start().expect("worker");
    let mut session = DiagnosticBundleSession::new(coordinator.client());
    session
        .submit(plan("safe"), DiagnosticBundleTarget::path(&destination))
        .expect("admission");
    let error = wait_for_completion(&mut session)
        .result
        .expect_err("missing parent");
    assert_eq!(error.kind(), DiagnosticBundleErrorKind::Io);
}

#[test]
fn executor_panic_resolves_the_request_and_types_the_dead_lane() {
    let coordinator = DiagnosticBundleCoordinator::start_with_executor(Arc::new(|_request| {
        panic!("fixture bundle fault");
    }))
    .expect("worker");
    let mut session = DiagnosticBundleSession::new(coordinator.client());
    session
        .submit(
            plan("safe"),
            DiagnosticBundleTarget::current_directory("fault.json"),
        )
        .expect("admission");
    let completion = wait_for_completion(&mut session);
    let error = completion.result.expect_err("faulted bundle must fail");
    assert_eq!(error.kind(), DiagnosticBundleErrorKind::Unavailable);
    assert!(
        error
            .detail()
            .is_some_and(|detail| detail.contains("fixture bundle fault"))
    );

    // Probe on an independent session: `close()` resets its one-active-request
    // state, so a request admitted during the brief window between the
    // terminal completion and the exit-flag publication cannot wedge the
    // probe at Busy before the typed stop becomes visible.
    let mut probe = DiagnosticBundleSession::new(coordinator.client());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match probe.submit(
            plan("probe"),
            DiagnosticBundleTarget::current_directory("probe.json"),
        ) {
            Err(error) if error.kind() == DiagnosticBundleErrorKind::Unavailable => break,
            outcome => {
                assert!(
                    Instant::now() < deadline,
                    "lane never reported its typed stop (last outcome: {outcome:?})"
                );
                probe.close();
                std::thread::yield_now();
            }
        }
    }
    assert!(probe.drain().is_empty());
    drop(probe);
    drop(session);
    drop(coordinator);
}
