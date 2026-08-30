use super::*;
use std::sync::Mutex;

struct FixedProcess {
    output: Mutex<Option<io::Result<HelperOutput>>>,
    seen: Mutex<Vec<String>>,
}

impl FixedProcess {
    fn output(status_code: Option<i32>, stdout: &str) -> Self {
        Self {
            output: Mutex::new(Some(Ok(HelperOutput {
                status_code,
                stdout: stdout.as_bytes().to_vec(),
            }))),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl ForeignProcessControlProcess for FixedProcess {
    fn run(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> io::Result<HelperOutput> {
        self.seen.lock().expect("seen mutex").push(format!(
            "{}:{}:{}",
            target.pid(),
            target.start_token(),
            operation.argument()
        ));
        self.output
            .lock()
            .expect("output mutex")
            .take()
            .unwrap_or_else(|| Err(io::Error::other("fixed process exhausted")))
    }
}

fn target() -> ForeignProcessControlTarget {
    ForeignProcessControlTarget::new(42, 9_000).expect("valid target")
}

#[test]
fn operation_arguments_are_fixed_and_shell_free() {
    assert_eq!(
        ForeignProcessControlOperation::SetPriority(-10).argument(),
        "priority:-10"
    );
    assert_eq!(
        ForeignProcessControlOperation::SetAffinity(vec![3, 1]).argument(),
        "affinity:3,1"
    );
    assert_eq!(
        ForeignProcessControlOperation::Signal(ForeignProcessSignal::User2).argument(),
        "signal:user2"
    );
}

#[test]
fn applied_contract_maps_to_success_and_preserves_target_call() {
    let process = FixedProcess::output(
        Some(0),
        r#"{"schema":1,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#,
    );
    assert_eq!(
        invoke_foreign_process_control_with(
            &process,
            target(),
            ForeignProcessControlOperation::Kill,
        ),
        ForeignProcessControlOutcome::Applied
    );
    assert_eq!(
        process.seen.lock().expect("seen mutex").as_slice(),
        ["42:9000:kill"]
    );
}

#[test]
fn helper_error_contract_preserves_identity_failure() {
    let process = FixedProcess::output(
        Some(3),
        r#"{"schema":1,"status":"error","kind":"identity_changed","detail":"reused"}"#,
    );
    assert_eq!(
        invoke_foreign_process_control_with(
            &process,
            target(),
            ForeignProcessControlOperation::End,
        ),
        ForeignProcessControlOutcome::Failed {
            kind: ForeignProcessControlFailure::IdentityChanged,
            detail: "reused".to_owned(),
        }
    );
}

#[test]
fn malformed_or_denied_helper_output_never_becomes_applied() {
    for (status, stdout, reason) in [
        (Some(126), "", EscalationDenialReason::PermissionDenied),
        (
            Some(127),
            "",
            EscalationDenialReason::AuthorizationUnavailable,
        ),
        (
            Some(0),
            "garbage",
            EscalationDenialReason::HelperProtocolViolation,
        ),
    ] {
        let process = FixedProcess::output(status, stdout);
        assert!(matches!(
            invoke_foreign_process_control_with(
                &process,
                target(),
                ForeignProcessControlOperation::Kill,
            ),
            ForeignProcessControlOutcome::Unavailable { reason: actual, .. } if actual == reason
        ));
    }
}

#[test]
fn target_constructor_rejects_zero_identity_components() {
    assert!(ForeignProcessControlTarget::new(0, 9).is_none());
    assert!(ForeignProcessControlTarget::new(42, 0).is_none());
}

#[test]
fn runner_timeout_is_reported_as_an_abandoned_crossing() {
    // The bounded runner maps its deadline kill onto ErrorKind::TimedOut; the
    // generic layer must not mislabel it as a spawn failure (the "end
    // process" path must surface an abandoned dialog honestly).
    let process = FixedProcess {
        output: Mutex::new(Some(Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "did not finish within the bounded deadline and was killed",
        )))),
        seen: Mutex::new(Vec::new()),
    };
    match invoke_foreign_process_control_with(
        &process,
        target(),
        ForeignProcessControlOperation::Kill,
    ) {
        ForeignProcessControlOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("killed at its deadline"), "{detail}");
            assert!(!detail.contains("could not spawn"), "{detail}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn windows_polkit_lane_stays_linux_only_and_routes_to_the_uac_transport() {
    assert!(matches!(
        windows_foreign_process_control_unavailable(),
        ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::Unsupported,
            detail,
        } if detail.contains("UAC transport")
    ));
}
