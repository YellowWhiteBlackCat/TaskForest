//! Contract tests for the MSR readout-helper crossing: parser fixtures,
//! fail-closed rejections, and the process-semantics mapping of
//! `invoke_msr_helper_with`. No test ever runs a real `pkexec`.

use super::*;
use crate::EscalationDenialReason;
use std::io;
use std::sync::Mutex;

/// A mock process that returns a canned reply or a synthetic spawn error.
struct FixedProcess {
    replies: Mutex<Vec<Reply>>,
}

enum Reply {
    Ok(HelperOutput),
    Err(io::ErrorKind, String),
}

impl FixedProcess {
    fn one_ok(stdout: &str, code: Option<i32>) -> Self {
        Self {
            replies: Mutex::new(vec![Reply::Ok(HelperOutput {
                status_code: code,
                stdout: stdout.as_bytes().to_vec(),
            })]),
        }
    }

    fn one_err(kind: io::ErrorKind, detail: &str) -> Self {
        Self {
            replies: Mutex::new(vec![Reply::Err(kind, detail.to_owned())]),
        }
    }
}

impl MsrHelperProcess for FixedProcess {
    fn run(&self) -> io::Result<HelperOutput> {
        let mut guard = self.replies.lock().expect("test reply mutex");
        match guard.pop() {
            Some(Reply::Ok(output)) => Ok(output),
            Some(Reply::Err(kind, detail)) => Err(io::Error::new(kind, detail)),
            None => panic!("FixedProcess exhausted its canned replies"),
        }
    }
}

const SUCCESS_FIXTURE: &str = concat!(
    r#"{"schema":1,"packages":["#,
    r#"{"cpu":0,"bclk_mhz":null,"temperature_c":58.0,"multiplier":45.0,"#,
    r#""multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":1.21875},"#,
    r#"{"cpu":2,"bclk_mhz":null,"temperature_c":null,"multiplier":null,"#,
    r#""multiplier_min":null,"multiplier_max":null,"vcore_v":null}]}"#
);

#[test]
fn parse_success_reads_every_typed_field() {
    match parse_helper_output(SUCCESS_FIXTURE) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.schema, 1);
            assert_eq!(success.packages.len(), 2);
            let first = &success.packages[0];
            assert_eq!(first.cpu, 0);
            assert_eq!(first.temperature_c, Some(58.0));
            assert_eq!(first.multiplier, Some(45.0));
            assert_eq!(first.multiplier_min, Some(8.0));
            assert_eq!(first.multiplier_max, Some(55.0));
            assert_eq!(first.vcore_v, Some(1.21875));
            assert_eq!(first.bclk_mhz, None, "bclk stays null (ADR-048)");
            // A node without implemented registers is honest nulls, never zeros.
            let second = &success.packages[1];
            assert_eq!(second.cpu, 2);
            assert_eq!(second.temperature_c, None);
            assert_eq!(second.multiplier, None);
            assert_eq!(second.vcore_v, None);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_accepts_the_honest_empty_package_list() {
    let stdout = r#"{"schema":1,"packages":[]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => assert!(success.packages.is_empty()),
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_error_reads_every_contract_kind() {
    for (kind, expected) in [
        ("permission_denied", MsrHelperErrorKind::PermissionDenied),
        ("no_msr", MsrHelperErrorKind::NoMsr),
        ("open_failed", MsrHelperErrorKind::OpenFailed),
        ("read_failed", MsrHelperErrorKind::ReadFailed),
    ] {
        let stdout = format!(r#"{{"status":"error","kind":"{kind}","detail":"/dev/cpu"}}"#);
        match parse_helper_output(&stdout) {
            ParsedOutput::HelperError(error) => {
                assert_eq!(error.kind, expected);
                assert_eq!(error.kind.as_contract_str(), kind);
                assert_eq!(error.detail, "/dev/cpu");
            }
            other => panic!("kind {kind}: expected HelperError, got {other:?}"),
        }
    }
}

#[test]
fn parse_rejects_non_contract_documents() {
    let bad_documents = [
        "not json",
        // Wrong schema.
        r#"{"schema":2,"packages":[]}"#,
        // Missing required keys.
        r#"{"schema":1}"#,
        r#"{"schema":1,"packages":{}}"#,
        // Row field violations: missing key, wrong type, non-finite or
        // non-physical magnitudes.
        r#"{"schema":1,"packages":[{"cpu":0}]}"#,
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":100,"temperature_c":58.0,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0}]}"#,
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":"100","temperature_c":58.0,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":null}]}"#,
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":null,"temperature_c":1e999,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":null}]}"#,
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":null,"temperature_c":-5.0,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":null}]}"#,
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":null,"temperature_c":58.0,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":9000.0}]}"#,
        r#"{"schema":1,"packages":[{"cpu":-1,"bclk_mhz":null,"temperature_c":58.0,"multiplier":45.0,"multiplier_min":8.0,"multiplier_max":55.0,"vcore_v":null}]}"#,
        // Unknown error kind / missing detail.
        r#"{"status":"error","kind":"fried","detail":"x"}"#,
        r#"{"status":"error","kind":"no_msr"}"#,
        "",
    ];
    for bad in bad_documents {
        assert!(
            matches!(parse_helper_output(bad), ParsedOutput::NotContract),
            "expected NotContract for: {bad}"
        );
    }
}

#[test]
fn parse_treats_null_and_present_values_as_distinct_honest_states() {
    // null vs a real number must BOTH parse; the distinction is the payload.
    let stdout = concat!(
        r#"{"schema":1,"packages":[{"cpu":0,"bclk_mhz":100.0,"temperature_c":null,"#,
        r#""multiplier":45.0,"multiplier_min":null,"multiplier_max":55.0,"vcore_v":null}]}"#
    );
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.packages[0].bclk_mhz, Some(100.0));
            assert_eq!(success.packages[0].temperature_c, None);
            assert_eq!(success.packages[0].multiplier_min, None);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn invoke_maps_a_spawn_failure_to_helper_unavailable() {
    let process = FixedProcess::one_err(io::ErrorKind::NotFound, "pkexec missing");
    match invoke_msr_helper_with(&process) {
        MsrHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("could not spawn"));
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_maps_helper_success_and_helper_error_verbatim() {
    let process = FixedProcess::one_ok(SUCCESS_FIXTURE, Some(0));
    match invoke_msr_helper_with(&process) {
        MsrHelperOutcome::Success(success) => assert_eq!(success.packages.len(), 2),
        other => panic!("expected Success, got {other:?}"),
    }
    let process = FixedProcess::one_ok(
        r#"{"status":"error","kind":"no_msr","detail":"no /dev/cpu tree"}"#,
        Some(3),
    );
    match invoke_msr_helper_with(&process) {
        MsrHelperOutcome::HelperError(error) => {
            assert_eq!(error.kind, MsrHelperErrorKind::NoMsr)
        }
        other => panic!("expected HelperError, got {other:?}"),
    }
}

#[test]
fn invoke_classifies_no_contract_replies_by_pkexec_exit_code() {
    let refusal = FixedProcess::one_ok("garbage", Some(126));
    match invoke_msr_helper_with(&refusal) {
        MsrHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::PermissionDenied);
        }
        other => panic!("expected Unavailable for exit 126, got {other:?}"),
    }
    let broker = FixedProcess::one_ok("garbage", Some(127));
    match invoke_msr_helper_with(&broker) {
        MsrHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::AuthorizationUnavailable);
        }
        other => panic!("expected Unavailable for exit 127, got {other:?}"),
    }
    let violation = FixedProcess::one_ok("", Some(9));
    match invoke_msr_helper_with(&violation) {
        MsrHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::HelperProtocolViolation);
        }
        other => panic!("expected Unavailable for exit 9, got {other:?}"),
    }
}

#[test]
fn the_production_driver_is_constructible_without_side_effects() {
    let _driver = PkexecMsrHelper::new();
}
