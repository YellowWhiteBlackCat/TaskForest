use super::*;
use crate::{EscalationAvailability, EscalationFeature, PrivilegeGate};
use std::io;
use std::sync::Mutex;

/// A mock process that returns a canned reply or a synthetic spawn error.
/// `Mutex` keeps the inner mutable while the trait method takes `&self`, so
/// a test can assert on call state without `&mut` plumbing.
struct FixedProcess {
    replies: Mutex<Vec<Reply>>,
}

enum Reply {
    Ok(HelperOutput),
    Err(io::ErrorKind, String),
}

impl FixedProcess {
    fn one_ok(stdout: impl Into<Vec<u8>>, code: Option<i32>) -> Self {
        Self {
            replies: Mutex::new(vec![Reply::Ok(HelperOutput {
                status_code: code,
                stdout: stdout.into(),
            })]),
        }
    }

    fn one_err(kind: io::ErrorKind, detail: &str) -> Self {
        Self {
            replies: Mutex::new(vec![Reply::Err(kind, detail.to_owned())]),
        }
    }
}

impl PerfHelperProcess for FixedProcess {
    fn run(&self) -> io::Result<HelperOutput> {
        let mut guard = self.replies.lock().expect("test reply mutex");
        match guard.pop() {
            Some(Reply::Ok(output)) => Ok(output),
            Some(Reply::Err(kind, detail)) => Err(io::Error::new(kind, detail)),
            None => panic!("FixedProcess exhausted its canned replies"),
        }
    }
}

// --- parse_helper_output: SUCCESS fixtures --------------------------------

#[test]
fn parse_success_fixture_produces_typed_engines() {
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":100,"engines":[
            {"name":"Render Ring","class":"rcs","busy_pct":42.5},
            {"name":"Blitter Ring","class":"bcs","busy_pct":0.0}
        ]}"#;
    let parsed = parse_helper_output(stdout);
    let PerfHelperSuccess {
        schema,
        driver,
        sample_ms,
        engines,
    } = match parsed {
        ParsedOutput::Success(s) => s,
        other => panic!("expected Success, got {other:?}"),
    };
    assert_eq!(schema, 1);
    assert_eq!(driver, "xe");
    assert_eq!(sample_ms, 100);
    assert_eq!(engines.len(), 2);
    assert_eq!(engines[0].name, "Render Ring");
    assert_eq!(engines[0].class, "rcs");
    assert_eq!(engines[0].busy_pct, 42.5);
    assert_eq!(engines[1].busy_pct, 0.0);
}

#[test]
fn parse_success_with_i915_driver_and_100_percent() {
    let stdout = r#"{"schema":1,"driver":"i915","sample_ms":50,"engines":[{"name":"VCS","class":"vcs","busy_pct":100.0}]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.driver, "i915");
            assert_eq!(success.engines[0].busy_pct, 100.0);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_with_empty_engines_array_is_valid() {
    // The contract permits a zero-engine SUCCESS object; that is honest
    // absence of engines, not a malformed document.
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":100,"engines":[]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => assert!(success.engines.is_empty()),
        other => panic!("expected Success, got {other:?}"),
    }
}

// --- parse_helper_output: ERROR fixtures (every kind) ---------------------

#[test]
fn parse_error_each_contract_kind_maps_to_typed_variant() {
    for (raw, expected) in [
        ("permission_denied", PerfHelperErrorKind::PermissionDenied),
        ("no_pmu", PerfHelperErrorKind::NoPmu),
        ("open_failed", PerfHelperErrorKind::OpenFailed),
        ("read_failed", PerfHelperErrorKind::ReadFailed),
    ] {
        let stdout = format!(r#"{{"status":"error","kind":"{raw}","detail":"boom"}}"#);
        match parse_helper_output(&stdout) {
            ParsedOutput::HelperError(error) => {
                assert_eq!(error.kind, expected, "kind {raw}");
                assert_eq!(error.kind.as_contract_str(), raw);
                assert_eq!(error.detail, "boom");
            }
            other => panic!("expected HelperError for {raw}, got {other:?}"),
        }
    }
}

#[test]
fn parse_error_unknown_kind_is_not_contract() {
    let stdout = r#"{"status":"error","kind":"mystery","detail":"x"}"#;
    assert!(matches!(
        parse_helper_output(stdout),
        ParsedOutput::NotContract
    ));
}

// --- parse_helper_output: malformed / out-of-contract inputs --------------

#[test]
fn parse_malformed_json_is_not_contract() {
    for stdout in [
        r#"{"schema":1,"driver":"xe","#, // truncated
        r#"not json at all"#,
        r#""a bare string""#,
        r#"42"#,
        r#"{"engines":}"#,
    ] {
        assert!(
            matches!(parse_helper_output(stdout), ParsedOutput::NotContract),
            "expected NotContract for {stdout:?}",
        );
    }
}

#[test]
fn parse_empty_stdout_is_not_contract() {
    assert!(matches!(parse_helper_output(""), ParsedOutput::NotContract));
    assert!(matches!(
        parse_helper_output("   \n\t  "),
        ParsedOutput::NotContract
    ));
}

#[test]
fn parse_success_wrong_schema_is_not_contract() {
    let stdout = r#"{"schema":2,"driver":"xe","sample_ms":100,"engines":[]}"#;
    assert!(matches!(
        parse_helper_output(stdout),
        ParsedOutput::NotContract
    ));
}

#[test]
fn parse_success_busy_pct_out_of_range_is_not_contract() {
    for bad in [
        r#"{"schema":1,"driver":"xe","sample_ms":1,"engines":[{"name":"R","class":"rcs","busy_pct":120.0}]}"#,
        r#"{"schema":1,"driver":"xe","sample_ms":1,"engines":[{"name":"R","class":"rcs","busy_pct":-1.0}]}"#,
    ] {
        assert!(
            matches!(parse_helper_output(bad), ParsedOutput::NotContract),
            "expected NotContract for {bad}",
        );
    }
}

#[test]
fn parse_success_missing_required_field_is_not_contract() {
    for bad in [
        r#"{"schema":1,"sample_ms":1,"engines":[]}"#, // no driver
        r#"{"schema":1,"driver":"xe","engines":[]}"#, // no sample_ms
        r#"{"schema":1,"driver":"xe","sample_ms":1}"#, // no engines
        r#"{"schema":1,"driver":"xe","sample_ms":1,"engines":[{"name":"R","class":"rcs"}]}"#, // engine missing busy_pct
    ] {
        assert!(
            matches!(parse_helper_output(bad), ParsedOutput::NotContract),
            "expected NotContract for {bad}",
        );
    }
}

#[test]
fn parse_success_sample_ms_must_be_integer() {
    let bad = r#"{"schema":1,"driver":"xe","sample_ms":1.5,"engines":[]}"#;
    assert!(matches!(
        parse_helper_output(bad),
        ParsedOutput::NotContract
    ));
}

#[test]
fn parse_object_with_engines_takes_precedence_over_status() {
    // The contract says a SUCCESS object has no "status"; the consumer
    // distinguishes by "engines". A document carrying BOTH is malformed in
    // spirit, but per the contract rule "engines" wins. Ensure that rule is
    // deterministic and that a SUCCESS parse still validates the engines.
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":1,"engines":[],"status":"error"}"#;
    assert!(matches!(
        parse_helper_output(stdout),
        ParsedOutput::Success(_)
    ));
}

#[test]
fn parse_neither_engines_nor_error_status_is_not_contract() {
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":1}"#;
    assert!(matches!(
        parse_helper_output(stdout),
        ParsedOutput::NotContract
    ));
}

#[test]
fn parse_handles_escape_sequences_and_unicode_in_strings() {
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":1,"engines":[{"name":"a\"b\\cé","class":"x","busy_pct":1.0}]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.engines[0].name, "a\"b\\cé");
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

// --- invoke_perf_helper_with: process-semantics mapping -------------------

#[test]
fn invoke_success_stdout_maps_to_success_outcome() {
    let stdout = r#"{"schema":1,"driver":"xe","sample_ms":100,"engines":[{"name":"R","class":"rcs","busy_pct":7.0}]}"#;
    let process = FixedProcess::one_ok(stdout, Some(0));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Success(success) => {
            assert_eq!(success.driver, "xe");
            assert_eq!(success.engines[0].busy_pct, 7.0);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn invoke_error_stdout_maps_to_helper_error_outcome() {
    let stdout = r#"{"status":"error","kind":"no_pmu","detail":"no i915 pmu"}"#;
    let process = FixedProcess::one_ok(stdout, Some(1));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::HelperError(error) => {
            assert_eq!(error.kind, PerfHelperErrorKind::NoPmu);
            assert_eq!(error.detail, "no i915 pmu");
        }
        other => panic!("expected HelperError, got {other:?}"),
    }
}

#[test]
fn invoke_exit_126_is_permission_denied() {
    // pkexec returns ~126 when the user dismisses the prompt; no contract
    // message arrives. The honest typed result is Denied/PermissionDenied.
    let process = FixedProcess::one_ok(Vec::new(), Some(126));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::PermissionDenied);
            assert!(
                detail.contains("126"),
                "detail should name the code: {detail}"
            );
        }
        other => panic!("expected Unavailable/PermissionDenied, got {other:?}"),
    }
}

#[test]
fn invoke_exit_127_is_authorization_unavailable() {
    let process = FixedProcess::one_ok(Vec::new(), Some(127));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::AuthorizationUnavailable);
            assert!(detail.contains("127"), "{detail}");
        }
        other => panic!("expected authorization unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_no_contract_is_helper_protocol_violation() {
    let process = FixedProcess::one_ok(b"garbage not json".to_vec(), Some(0));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperProtocolViolation);
            assert!(detail.contains("no valid contract message"), "{detail}");
        }
        other => panic!("expected helper protocol violation, got {other:?}"),
    }
}

#[test]
fn invoke_spawn_not_found_is_helper_unavailable() {
    let process = FixedProcess::one_err(io::ErrorKind::NotFound, "pkexec not found");
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("spawn"), "{detail}");
        }
        other => panic!("expected Unavailable/HelperUnavailable, got {other:?}"),
    }
}

#[test]
fn invoke_trailing_whitespace_in_stdout_is_tolerated() {
    // The helper writes one JSON object then a trailing newline; the parser
    // must accept that.
    let stdout = "{\"schema\":1,\"driver\":\"xe\",\"sample_ms\":1,\"engines\":[]}\n";
    let process = FixedProcess::one_ok(stdout.as_bytes().to_vec(), Some(0));
    assert!(matches!(
        invoke_perf_helper_with(&process),
        PerfHelperOutcome::Success(_)
    ));
}

// --- bounded-detail truncation on multibyte stdout ------------------------

#[test]
fn truncate_for_detail_cuts_multibyte_text_at_char_boundaries() {
    // Byte 160 of each input lands inside a multi-byte character; the old
    // `text[..LIMIT]` slice panicked there. The cut must land on the newest
    // boundary at or before 160 bytes and stay honest with one ellipsis.
    for (text, width) in [
        (format!("x{}", "中".repeat(100)), "3-byte char"),
        (format!("x{}", "é".repeat(200)), "2-byte char"),
        (format!("x{}", "😀".repeat(100)), "4-byte char"),
    ] {
        let cut = truncate_for_detail(&text);
        assert!(cut.ends_with('…'), "{width}: {cut:?}");
        let body = &cut[..cut.len() - '…'.len_utf8()];
        assert!(text.starts_with(body), "{width}: cut diverged: {cut:?}");
        assert!(text.is_char_boundary(body.len()), "{width}: mid-char cut");
        assert!(body.len() <= 160, "{width}: cut too long: {}", body.len());
    }

    // Short and boundary-aligned inputs pass through unchanged.
    assert_eq!(truncate_for_detail(" short "), "short");
    let aligned = "中".repeat(53); // exactly 159 bytes… still <= 160
    assert_eq!(truncate_for_detail(&aligned), aligned);
}

#[test]
fn invoke_multibyte_garbage_stdout_does_not_panic_and_stays_bounded() {
    // The real defect surface: raw helper stdout reaching the diagnostic
    // detail through the not-a-contract classification.
    let stdout = "中".repeat(400); // 1200 bytes, byte 160 mid-character
    let process = FixedProcess::one_ok(stdout.as_bytes().to_vec(), Some(0));
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperProtocolViolation);
            assert!(detail.contains("no valid contract message"), "{detail}");
            // The stdout part of the detail was cut at a char boundary and
            // flagged with one ellipsis instead of panicking.
            assert!(detail.ends_with('…'), "{detail}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_runner_timeout_is_reported_as_an_abandoned_crossing() {
    // The bounded runner maps its deadline kill onto ErrorKind::TimedOut;
    // the generic layer must not mislabel it as a spawn failure.
    let process = FixedProcess::one_err(
        io::ErrorKind::TimedOut,
        "did not finish within the bounded deadline and was killed",
    );
    match invoke_perf_helper_with(&process) {
        PerfHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("killed at its deadline"), "{detail}");
            assert!(!detail.contains("could not spawn"), "{detail}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

// --- PolkitGate::probe ----------------------------------------------------

#[test]
fn polkit_gate_probe_intel_pmu_never_claims_available_or_overstates() {
    // The host in the test lane may or may not have pkexec/polkit; either
    // way the gate must not fabricate Available access, and must not report
    // PermissionDenied for a mere probe (probing never asks the user).
    let gate = PolkitGate::new();
    match gate.probe(EscalationFeature::IntelPmu) {
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu) => {}
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        } => {}
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::Unsupported,
        } => {}
        other => panic!("probe(IntelPmu) returned an overclaiming {other:?}"),
    }
}

#[test]
fn polkit_gate_probe_process_control_is_typed_without_overclaiming() {
    let gate = PolkitGate::new();
    match gate.probe(EscalationFeature::ForeignProcessControl) {
        EscalationAvailability::RequiresEscalation(EscalationFeature::ForeignProcessControl)
        | EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        }
        | EscalationAvailability::Denied {
            reason: EscalationDenialReason::Unsupported,
        } => {}
        other => panic!("process-control probe over-claimed availability: {other:?}"),
    }
}

#[test]
fn polkit_gate_probe_unwired_features_defer_to_requires_escalation() {
    // PerProcessNet is NOT here: it has a real probe (`gate::
    // probe_net_launcher`, fixture-tested in `gate.rs`) that distinguishes
    // "prompt available" from HelperUnavailable, so its result depends on
    // host state instead of deferring to the default.
    let gate = PolkitGate::new();
    for feature in [
        EscalationFeature::AtaSmart,
        EscalationFeature::SystemServiceControl,
        EscalationFeature::MemorySmbios,
        EscalationFeature::PackagePowerRapl,
    ] {
        assert_eq!(
            gate.probe(feature),
            EscalationAvailability::RequiresEscalation(feature),
            "PolkitGate should not over-claim denial for an unwired feature",
        );
    }
}

// --- polkit .policy.in is well-formed and complete ------------------------

/// Locate the polkit policy template relative to this crate. It lives at the
/// repository root
/// (`polkit/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy.in`);
/// from this crate that is `../../polkit/...`.
fn policy_in_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../polkit/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy.in",
    )
}

#[test]
fn polkit_policy_template_is_well_formed_and_complete() {
    let path = policy_in_path();
    let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "polkit policy template should be readable at {}: {error}",
            path.display()
        )
    });

    // Required structural fragments (the polkit action contract).
    for fragment in [
        "<?xml version=\"1.0\"",
        "<policyconfig>",
        "<action id=\"io.github.YellowWhiteBlackCat.TaskForest.perf-helper\">",
        "<description>",
        "<message>",
        "auth_admin_keep",
        "/usr/libexec/taskforest-privilege-helper",
        "</policyconfig>",
    ] {
        assert!(
            content.contains(fragment),
            "polkit policy template lost required fragment: {fragment}",
        );
    }

    // Best-effort true well-formedness via xmllint when it is installed; if
    // it is absent the structural check above stands.
    let xmllint = std::process::Command::new("xmllint")
        .arg("--nonet")
        .arg("--noout")
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match xmllint {
        Ok(status) => assert!(
            status.success(),
            "xmllint reported the polkit policy template is not well-formed XML"
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // xmllint not installed in this lane; the structural check above
            // is the fallback the task specified.
        }
        Err(error) => panic!("could not run xmllint to validate the policy: {error}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn net_launcher_policy_template_is_well_formed_and_complete() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../polkit/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy.in",
    );
    let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "net-launcher policy template should be readable at {}: {error}",
            path.display()
        )
    });
    // The exec.path annotation MUST match NET_LAUNCHER_PATH byte-for-byte
    // (polkit resolves the action by that path).
    for fragment in [
        "<?xml version=\"1.0\"",
        "<policyconfig>",
        "<action id=\"io.github.YellowWhiteBlackCat.TaskForest.net-launcher\">",
        "<description>",
        "<message>",
        "auth_admin_keep",
        "/usr/libexec/taskforest-net-launcher",
        "</policyconfig>",
    ] {
        assert!(
            content.contains(fragment),
            "net-launcher policy lost required fragment: {fragment}",
        );
    }
    assert_eq!(
        net_launcher::NET_LAUNCHER_PATH,
        "/usr/libexec/taskforest-net-launcher",
        "NET_LAUNCHER_PATH must match the policy's exec.path annotation"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn process_control_policy_template_matches_the_fixed_helper_path() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../polkit/io.github.YellowWhiteBlackCat.TaskForest.process-control.policy.in",
    );
    let content = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "process-control policy template should be readable at {}: {error}",
            path.display()
        )
    });
    for fragment in [
        "<?xml version=\"1.0\"",
        "<policyconfig>",
        "<action id=\"io.github.YellowWhiteBlackCat.TaskForest.process-control\">",
        "auth_admin_keep",
        "/usr/libexec/taskforest-process-control-helper",
        "</policyconfig>",
    ] {
        assert!(
            content.contains(fragment),
            "process-control policy lost required fragment: {fragment}",
        );
    }
    assert_eq!(
        process_control::PROCESS_CONTROL_HELPER_PATH,
        "/usr/libexec/taskforest-process-control-helper",
        "process-control helper path must match the policy annotation"
    );
}

// --- net-launcher invocation (ADR-024/025) ---

#[cfg(target_os = "linux")]
/// A mock launcher returning a canned fd or a synthetic permission error.
struct CannedLauncher {
    ok: bool,
}

#[cfg(target_os = "linux")]
impl NetLauncherProcess for CannedLauncher {
    fn obtain_fd(&self, _iface_index: u32) -> io::Result<NetLaunchHandle> {
        if self.ok {
            // Any valid fd suffices — the outcome only carries it opaquely.
            Ok(NetLaunchHandle::from(std::fs::File::open("/dev/null")?))
        } else {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn invoke_net_launcher_success_carries_the_fd() {
    let outcome = invoke_net_launcher_with(&CannedLauncher { ok: true }, 2);
    assert!(
        matches!(outcome, NetLauncherOutcome::Success(_)),
        "expected the fd back, got {outcome:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn invoke_net_launcher_denial_is_typed_unavailable() {
    let outcome = invoke_net_launcher_with(&CannedLauncher { ok: false }, 2);
    match outcome {
        NetLauncherOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::PermissionDenied);
            assert!(detail.contains("net-launcher"));
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}
