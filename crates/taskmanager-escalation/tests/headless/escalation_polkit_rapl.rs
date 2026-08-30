//! Contract tests for the RAPL package-power-helper crossing: parser
//! fixtures, fail-closed rejections, and the process-semantics mapping of
//! `invoke_rapl_helper_with`. No test ever runs a real `pkexec`.

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

impl RaplHelperProcess for FixedProcess {
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
    r#"{"schema":1,"sample_ms":1000,"packages":["#,
    r#"{"name":"package-0","power_w":12.5,"energy_delta_uj":12500000},"#,
    r#"{"name":"package-1","power_w":7.25,"energy_delta_uj":7250000}]}"#
);

#[test]
fn parse_success_reads_every_typed_field() {
    match parse_helper_output(SUCCESS_FIXTURE) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.schema, 1);
            assert_eq!(success.sample_ms, 1000);
            assert_eq!(success.packages.len(), 2);
            assert_eq!(success.packages[0].name, "package-0");
            assert_eq!(success.packages[0].power_w, 12.5);
            assert_eq!(success.packages[0].energy_delta_uj, 12_500_000);
            assert_eq!(success.packages[1].name, "package-1");
            assert_eq!(success.packages[1].power_w, 7.25);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_accepts_the_honest_empty_package_list() {
    let stdout = r#"{"schema":1,"sample_ms":1000,"packages":[]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => assert!(success.packages.is_empty()),
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_error_reads_every_contract_kind() {
    for (kind, expected) in [
        ("permission_denied", RaplHelperErrorKind::PermissionDenied),
        ("no_rapl", RaplHelperErrorKind::NoRapl),
        ("open_failed", RaplHelperErrorKind::OpenFailed),
        ("read_failed", RaplHelperErrorKind::ReadFailed),
    ] {
        let stdout = format!(r#"{{"status":"error","kind":"{kind}","detail":"powercap"}}"#);
        match parse_helper_output(&stdout) {
            ParsedOutput::HelperError(error) => {
                assert_eq!(error.kind, expected);
                assert_eq!(error.detail, "powercap");
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
        r#"{"schema":2,"sample_ms":1000,"packages":[]}"#,
        // Missing required fields.
        r#"{"schema":1,"packages":[]}"#,
        r#"{"schema":1,"sample_ms":1000}"#,
        // Package field violations.
        r#"{"schema":1,"sample_ms":1000,"packages":[{"name":"package-0"}]}"#,
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"package-0","power_w":12.5}]}"#,
        // Empty name.
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"","power_w":1.0,"energy_delta_uj":1}]}"#,
        // Negative power.
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"p","power_w":-1.0,"energy_delta_uj":1}]}"#,
        // Infinite power (f64 parses 1e999 as infinity).
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"p","power_w":1e999,"energy_delta_uj":1}]}"#,
        // Non-physical magnitude.
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"p","power_w":1e7,"energy_delta_uj":1}]}"#,
        // Fractional / negative energy delta.
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"p","power_w":1.0,"energy_delta_uj":1.5}]}"#,
        r#"{"schema":1,"sample_ms":1000,
            "packages":[{"name":"p","power_w":1.0,"energy_delta_uj":-4}]}"#,
        // Unknown error kind / missing detail.
        r#"{"status":"error","kind":"melted","detail":"x"}"#,
        r#"{"status":"error","kind":"no_rapl"}"#,
        r#"{"schema":1}"#,
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
fn energy_delta_accepts_the_full_u64_range() {
    // RAPL counters count microjoules; a long window on a many-socket host
    // must still round-trip through the f64-based std-only reader.
    let stdout = concat!(
        r#"{"schema":1,"sample_ms":60000,"#,
        r#""packages":[{"name":"package-0","power_w":250.0,"#,
        r#""energy_delta_uj":18000000000000}]}"#
    );
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.packages[0].energy_delta_uj, 18_000_000_000_000);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn invoke_maps_a_spawn_failure_to_helper_unavailable() {
    let process = FixedProcess::one_err(io::ErrorKind::NotFound, "pkexec missing");
    match invoke_rapl_helper_with(&process) {
        RaplHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("could not spawn"));
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_maps_helper_success_and_helper_error_verbatim() {
    let process = FixedProcess::one_ok(SUCCESS_FIXTURE, Some(0));
    match invoke_rapl_helper_with(&process) {
        RaplHelperOutcome::Success(success) => assert_eq!(success.packages.len(), 2),
        other => panic!("expected Success, got {other:?}"),
    }
    let process = FixedProcess::one_ok(
        r#"{"status":"error","kind":"no_rapl","detail":"no intel-rapl nodes"}"#,
        Some(3),
    );
    match invoke_rapl_helper_with(&process) {
        RaplHelperOutcome::HelperError(error) => {
            assert_eq!(error.kind, RaplHelperErrorKind::NoRapl)
        }
        other => panic!("expected HelperError, got {other:?}"),
    }
}

#[test]
fn invoke_classifies_no_contract_replies_by_pkexec_exit_code() {
    let refusal = FixedProcess::one_ok("garbage", Some(126));
    match invoke_rapl_helper_with(&refusal) {
        RaplHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::PermissionDenied);
        }
        other => panic!("expected Unavailable for exit 126, got {other:?}"),
    }
    let broker = FixedProcess::one_ok("garbage", Some(127));
    match invoke_rapl_helper_with(&broker) {
        RaplHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::AuthorizationUnavailable);
        }
        other => panic!("expected Unavailable for exit 127, got {other:?}"),
    }
    let violation = FixedProcess::one_ok("", Some(9));
    match invoke_rapl_helper_with(&violation) {
        RaplHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::HelperProtocolViolation);
        }
        other => panic!("expected Unavailable for exit 9, got {other:?}"),
    }
}

#[test]
fn the_production_driver_is_constructible_without_side_effects() {
    let _driver = PkexecRaplHelper::new();
}
