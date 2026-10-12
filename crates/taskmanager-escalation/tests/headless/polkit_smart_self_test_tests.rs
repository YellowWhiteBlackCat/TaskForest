use std::io;

use crate::EscalationDenialReason;
use crate::polkit::{
    HelperOutput, SmartSelfTestHelperFailure, SmartSelfTestHelperOutcome,
    SmartSelfTestHelperProcess, SmartSelfTestHelperRequest, invoke_smart_self_test_helper_with,
};

struct FixedProcess {
    status_code: Option<i32>,
    stdout: &'static str,
}

impl SmartSelfTestHelperProcess for FixedProcess {
    fn run(&self, request: SmartSelfTestHelperRequest<'_>) -> io::Result<HelperOutput> {
        assert_eq!(request.device_id, "disk:wwid:fixture");
        assert_eq!(request.device_generation, 4);
        assert_eq!(request.locator, "nvme0n1");
        assert_eq!(request.kind, "short");
        Ok(HelperOutput {
            status_code: self.status_code,
            stdout: self.stdout.as_bytes().to_vec(),
        })
    }
}

fn request() -> SmartSelfTestHelperRequest<'static> {
    SmartSelfTestHelperRequest {
        device_id: "disk:wwid:fixture",
        device_generation: 4,
        locator: "nvme0n1",
        protocol: "nvme",
        interconnect: "pcie",
        kind: "short",
    }
}

#[test]
fn typed_start_and_helper_failures_are_separate_from_authorization_failures() {
    let started = invoke_smart_self_test_helper_with(
        &FixedProcess {
            status_code: Some(0),
            stdout: r#"{"schema":1,"status":"started"}"#,
        },
        request(),
    );
    assert_eq!(started, SmartSelfTestHelperOutcome::Started);

    let unsupported = invoke_smart_self_test_helper_with(
        &FixedProcess {
            status_code: Some(1),
            stdout: r#"{"schema":1,"status":"error","kind":"unsupported"}"#,
        },
        request(),
    );
    assert_eq!(
        unsupported,
        SmartSelfTestHelperOutcome::HelperError(SmartSelfTestHelperFailure::Unsupported)
    );
}

#[test]
fn pkexec_refusal_and_incomplete_authorization_remain_distinct() {
    let refused = invoke_smart_self_test_helper_with(
        &FixedProcess {
            status_code: Some(126),
            stdout: "",
        },
        request(),
    );
    assert!(matches!(
        refused,
        SmartSelfTestHelperOutcome::Unavailable {
            reason: EscalationDenialReason::PermissionDenied,
            ..
        }
    ));

    let incomplete = invoke_smart_self_test_helper_with(
        &FixedProcess {
            status_code: Some(127),
            stdout: "",
        },
        request(),
    );
    assert!(matches!(
        incomplete,
        SmartSelfTestHelperOutcome::Unavailable {
            reason: EscalationDenialReason::AuthorizationUnavailable,
            ..
        }
    ));
}

#[test]
fn malformed_helper_reply_fails_closed() {
    let result = invoke_smart_self_test_helper_with(
        &FixedProcess {
            status_code: Some(0),
            stdout: r#"{"schema":2,"status":"started"}"#,
        },
        request(),
    );
    assert!(matches!(
        result,
        SmartSelfTestHelperOutcome::Unavailable {
            reason: EscalationDenialReason::HelperProtocolViolation,
            ..
        }
    ));
}
