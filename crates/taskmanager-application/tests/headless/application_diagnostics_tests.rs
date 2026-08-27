use std::collections::VecDeque;

use taskmanager_core::DiagnosticSource;

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
fn service_log_plan_is_sanitized_before_crossing_the_port() {
    let entries = [crate::ServiceLogEntry {
        cursor: "1".into(),
        realtime_timestamp_micros: Some(1),
        priority: Some(6),
        level: crate::ServiceLogLevel::Info,
        message: "user alice at /home/<user>".into(),
    }];
    let plan = prepare_service_log_bundle(&entries).expect("plan");
    let encoded = String::from_utf8(plan.encoded().expect("encoded")).expect("utf8");
    assert!(!encoded.contains("/home/<user>"));
}
