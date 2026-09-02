use std::collections::VecDeque;

use taskmanager_core::core::diagnostics::DiagnosticSource;
use taskmanager_core::core::services::{ServiceLogEntry, ServiceLogLevel};

use super::*;

fn plan(contents: &str) -> DiagnosticBundlePlan {
    DiagnosticBundlePlan::prepare(
        vec![DiagnosticSource {
            name: "facts.txt".into(),
            contents: contents.into(),
        }],
        [],
    )
    .expect("valid plan")
}

#[derive(Debug, Default)]
struct FakePort {
    submitted: Vec<DiagnosticBundleRequest>,
    completions: VecDeque<DiagnosticBundleCompletion>,
    reject: bool,
}

impl DiagnosticBundlePort for FakePort {
    fn try_submit(
        &mut self,
        request: DiagnosticBundleRequest,
    ) -> Result<(), DiagnosticBundleError> {
        if self.reject {
            Err(DiagnosticBundleError::new(DiagnosticBundleErrorKind::Busy))
        } else {
            self.submitted.push(request);
            Ok(())
        }
    }

    fn drain(&mut self) -> Vec<DiagnosticBundleCompletion> {
        self.completions.drain(..).collect()
    }
}

#[test]
fn session_rejects_duplicate_submit_and_correlates_terminal() {
    let mut session = DiagnosticBundleSession::new(FakePort::default());
    let request = session
        .submit(
            plan("safe"),
            DiagnosticBundleTarget::current_directory("bundle.json"),
        )
        .expect("accepted");
    let error = session
        .submit(
            plan("second"),
            DiagnosticBundleTarget::current_directory("second.json"),
        )
        .expect_err("active request is busy");
    assert_eq!(error.kind(), DiagnosticBundleErrorKind::Busy);
    session
        .port
        .completions
        .push_back(DiagnosticBundleCompletion {
            request,
            destination: PathBuf::from("bundle.json"),
            result: Ok(()),
        });
    assert_eq!(session.drain().len(), 1);
    assert_eq!(session.active_request(), None);
}

#[test]
fn close_makes_late_completion_inert() {
    let mut session = DiagnosticBundleSession::new(FakePort::default());
    let request = session
        .submit(
            plan("safe"),
            DiagnosticBundleTarget::current_directory("bundle.json"),
        )
        .expect("accepted");
    session.close();
    session
        .port
        .completions
        .push_back(DiagnosticBundleCompletion {
            request,
            destination: PathBuf::from("bundle.json"),
            result: Ok(()),
        });
    assert!(session.drain().is_empty());
}

#[test]
fn session_rejects_invalid_current_directory_target_before_port_admission() {
    let mut session = DiagnosticBundleSession::new(FakePort::default());
    let error = session
        .submit(
            plan("safe"),
            DiagnosticBundleTarget::current_directory("../escape.json"),
        )
        .expect_err("current-directory target must be one filename component");

    assert_eq!(error.kind(), DiagnosticBundleErrorKind::InvalidTarget);
    assert_eq!(session.active_request(), None);
    assert!(session.port.submitted.is_empty());
}

#[test]
fn diagnostic_targets_share_the_portable_filename_contract() {
    assert!(DiagnosticBundleTarget::current_directory("bundle.json").is_valid());
    for file_name in [
        "",
        ".",
        "..",
        "nested/bundle.json",
        r"nested\bundle.json",
        r"C:\bundle.json",
        "CON.json",
        "trailing-dot.",
    ] {
        assert!(!DiagnosticBundleTarget::current_directory(file_name).is_valid());
    }
    let too_long = "a".repeat(256);
    let maximum = "a".repeat(255);
    assert!(!DiagnosticBundleTarget::current_directory(too_long.as_str()).is_valid());
    assert!(DiagnosticBundleTarget::current_directory(maximum.as_str()).is_valid());
    assert!(DiagnosticBundleTarget::current_directory("诊断包-01.json").is_valid());
    assert!(DiagnosticBundleTarget::path("../explicit/bundle.json").is_valid());
}

#[test]
fn service_log_plan_is_sanitized_before_crossing_the_port() {
    let entries = [ServiceLogEntry {
        cursor: "1".into(),
        realtime_timestamp_micros: Some(1),
        priority: Some(6),
        level: ServiceLogLevel::Info,
        message: "user alice at /home/<user>".into(),
    }];
    let plan = prepare_service_log_bundle(&entries).expect("plan");
    let encoded = String::from_utf8(plan.encoded().expect("encoded")).expect("utf8");
    assert!(!encoded.contains("/home/<user>"));
}
