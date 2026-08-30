//! Contract tests for the SMBIOS memory-helper crossing: parser fixtures,
//! fail-closed rejections, and the process-semantics mapping of
//! `invoke_smbios_helper_with`. No test ever runs a real `pkexec`.

use super::*;
use crate::EscalationDenialReason;
use std::io;
use std::sync::Mutex;

/// A mock process that returns a canned reply or a synthetic spawn error
/// (mirrors the perf seam's `FixedProcess`).
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

impl SmbiosHelperProcess for FixedProcess {
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
    r#"{"schema":1,"slots_total":4,"slots_used":2,"modules":["#,
    r#"{"slot":0,"size_mb":16384,"speed_mts":5600,"configured_speed_mts":5200,"#,
    r#""manufacturer":"Samsung","serial_number":"37A31B2C","part_number":"M425R4GA3PB0","#,
    r#""form_factor":"SODIMM","memory_type":"DDR5","locator":"ChannelA-DIMM0"},"#,
    r#"{"slot":2,"size_mb":16384,"speed_mts":5600,"configured_speed_mts":null,"#,
    r#""manufacturer":null,"serial_number":null,"part_number":null,"#,
    r#""form_factor":null,"memory_type":null,"locator":"ChannelB-DIMM0"}],"#,
    r#""identity":{"bios_vendor":"AMI","bios_version":"P1.27","#,
    r#""bios_date":"04/17/2024","board_manufacturer":"ASUSTeK","#,
    r#""board_product":"X670E","board_serial":"MB-SN-1","#,
    r#""board_asset_tag":"ASSET-42","system_manufacturer":"LENOVO","#,
    r#""system_product":"21JX","system_serial":"PF3XYZ42","#,
    r#""system_uuid":"4c4c4544-0042-3510-8054-b7c04f4d3532","#,
    r#""system_sku":"SKU-AB","system_family":"ThinkPad"}}"#
);

/// A SUCCESS document from a helper predating the additive `identity` field.
const LEGACY_SUCCESS_FIXTURE: &str = concat!(
    r#"{"schema":1,"slots_total":4,"slots_used":2,"modules":["#,
    r#"{"slot":0,"size_mb":16384,"speed_mts":5600,"configured_speed_mts":5200,"#,
    r#""manufacturer":"Samsung","serial_number":"37A31B2C","part_number":"M425R4GA3PB0","#,
    r#""form_factor":"SODIMM","memory_type":"DDR5","locator":"ChannelA-DIMM0"},"#,
    r#"{"slot":2,"size_mb":16384,"speed_mts":5600,"configured_speed_mts":null,"#,
    r#""manufacturer":null,"serial_number":null,"part_number":null,"#,
    r#""form_factor":null,"memory_type":null,"locator":"ChannelB-DIMM0"}]}"#
);

#[test]
fn parse_success_reads_every_typed_field() {
    match parse_helper_output(SUCCESS_FIXTURE) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.schema, 1);
            assert_eq!(success.slots_total, 4);
            assert_eq!(success.slots_used, 2);
            assert_eq!(success.modules.len(), 2);
            let first = &success.modules[0];
            assert_eq!(first.slot, 0);
            assert_eq!(first.size_mb, Some(16_384));
            assert_eq!(first.speed_mts, Some(5_600));
            assert_eq!(first.configured_speed_mts, Some(5_200));
            assert_eq!(first.manufacturer.as_deref(), Some("Samsung"));
            assert_eq!(first.serial_number.as_deref(), Some("37A31B2C"));
            assert_eq!(first.part_number.as_deref(), Some("M425R4GA3PB0"));
            assert_eq!(first.form_factor.as_deref(), Some("SODIMM"));
            assert_eq!(first.memory_type.as_deref(), Some("DDR5"));
            assert_eq!(first.locator.as_deref(), Some("ChannelA-DIMM0"));
            let second = &success.modules[1];
            assert_eq!(second.slot, 2);
            assert_eq!(second.configured_speed_mts, None);
            assert_eq!(second.manufacturer, None);
            assert_eq!(second.form_factor, None);
            let identity = success.identity.as_ref().expect("identity present");
            assert_eq!(identity.bios_vendor.as_deref(), Some("AMI"));
            assert_eq!(identity.bios_version.as_deref(), Some("P1.27"));
            assert_eq!(identity.bios_date.as_deref(), Some("04/17/2024"));
            assert_eq!(identity.board_manufacturer.as_deref(), Some("ASUSTeK"));
            assert_eq!(identity.board_product.as_deref(), Some("X670E"));
            assert_eq!(identity.board_serial.as_deref(), Some("MB-SN-1"));
            assert_eq!(identity.board_asset_tag.as_deref(), Some("ASSET-42"));
            assert_eq!(identity.system_manufacturer.as_deref(), Some("LENOVO"));
            assert_eq!(identity.system_product.as_deref(), Some("21JX"));
            assert_eq!(identity.system_serial.as_deref(), Some("PF3XYZ42"));
            assert_eq!(
                identity.system_uuid.as_deref(),
                Some("4c4c4544-0042-3510-8054-b7c04f4d3532")
            );
            assert_eq!(identity.system_sku.as_deref(), Some("SKU-AB"));
            assert_eq!(identity.system_family.as_deref(), Some("ThinkPad"));
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_accepts_a_legacy_helper_without_the_identity_field() {
    match parse_helper_output(LEGACY_SUCCESS_FIXTURE) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.slots_used, 2, "the module rows still parse");
            assert_eq!(success.identity, None, "a missing key is an old helper");
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_accepts_an_explicit_null_identity() {
    let stdout = r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],"identity":null}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => assert_eq!(success.identity, None),
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_keeps_null_identity_fields_as_none() {
    let stdout = concat!(
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],"identity":"#,
        r#"{"bios_vendor":null,"bios_version":null,"bios_date":null,"#,
        r#""board_manufacturer":null,"board_product":null,"board_serial":null,"#,
        r#""board_asset_tag":"ASSET-42","system_manufacturer":null,"#,
        r#""system_product":null,"system_serial":null,"system_uuid":null,"#,
        r#""system_sku":null,"system_family":null}}"#
    );
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            let identity = success.identity.expect("identity object present");
            assert_eq!(identity.board_asset_tag.as_deref(), Some("ASSET-42"));
            assert_eq!(identity.system_serial, None);
            assert_eq!(identity.system_uuid, None);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_success_accepts_the_honest_empty_module_list() {
    let stdout = r#"{"schema":1,"slots_total":2,"slots_used":0,"modules":[]}"#;
    match parse_helper_output(stdout) {
        ParsedOutput::Success(success) => {
            assert_eq!(success.slots_total, 2);
            assert!(success.modules.is_empty());
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[test]
fn parse_error_reads_every_contract_kind() {
    for (kind, expected) in [
        ("permission_denied", SmbiosHelperErrorKind::PermissionDenied),
        ("no_dmi", SmbiosHelperErrorKind::NoDmi),
        ("open_failed", SmbiosHelperErrorKind::OpenFailed),
        ("read_failed", SmbiosHelperErrorKind::ReadFailed),
    ] {
        let stdout = format!(r#"{{"status":"error","kind":"{kind}","detail":"dmi walk failed"}}"#);
        match parse_helper_output(&stdout) {
            ParsedOutput::HelperError(error) => {
                assert_eq!(error.kind, expected);
                assert_eq!(error.detail, "dmi walk failed");
            }
            other => panic!("kind {kind}: expected HelperError, got {other:?}"),
        }
    }
}

#[test]
fn parse_rejects_non_contract_documents() {
    let bad_documents = [
        // Not JSON.
        "not json",
        // Wrong schema version.
        r#"{"schema":2,"slots_total":1,"slots_used":1,"modules":[]}"#,
        // Missing required top-level fields.
        r#"{"schema":1,"slots_used":1,"modules":[]}"#,
        r#"{"schema":1,"slots_total":1,"modules":[]}"#,
        r#"{"schema":1,"slots_total":1,"slots_used":1}"#,
        // Non-integer integer fields.
        r#"{"schema":1,"slots_total":"4","slots_used":0,"modules":[]}"#,
        r#"{"schema":1.5,"slots_total":1,"slots_used":0,"modules":[]}"#,
        // Module field violations: missing key, wrong type, fractional,
        // negative.
        r#"{"schema":1,"slots_total":1,"slots_used":1,"modules":[{"slot":0}]}"#,
        r#"{"schema":1,"slots_total":1,"slots_used":1,"modules":[{"slot":0,"size_mb":"16"}]}"#,
        r#"{"schema":1,"slots_total":1,"slots_used":1,
            "modules":[{"slot":0,"size_mb":16.5}]}"#,
        r#"{"schema":1,"slots_total":1,"slots_used":1,
            "modules":[{"slot":-1,"size_mb":16}]}"#,
        // A non-string, non-null optional string field.
        r#"{"schema":1,"slots_total":1,"slots_used":1,
            "modules":[{"slot":0,"manufacturer":17}]}"#,
        // Identity violations: non-object shape, non-object field value,
        // missing field inside the object, wrong inner type.
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],"identity":"none"}"#,
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],"identity":7}"#,
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],"identity":[]}"#,
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],
            "identity":{"bios_vendor":null,"bios_version":null,"bios_date":null,
            "board_manufacturer":null,"board_product":null,"board_serial":null,
            "board_asset_tag":null,"system_manufacturer":null,
            "system_product":null,"system_serial":null,"system_uuid":null,
            "system_sku":null}}"#,
        r#"{"schema":1,"slots_total":0,"slots_used":0,"modules":[],
            "identity":{"bios_vendor":null,"bios_version":null,"bios_date":null,
            "board_manufacturer":null,"board_product":null,"board_serial":null,
            "board_asset_tag":null,"system_manufacturer":null,
            "system_product":null,"system_serial":null,"system_uuid":null,
            "system_sku":null,"system_family":19}}"#,
        // Unknown error kind.
        r#"{"status":"error","kind":"exploded","detail":"x"}"#,
        // ERROR missing its detail.
        r#"{"status":"error","kind":"no_dmi"}"#,
        // Neither branch discriminator present.
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
fn invoke_maps_a_spawn_failure_to_helper_unavailable() {
    let process = FixedProcess::one_err(io::ErrorKind::NotFound, "pkexec missing");
    match invoke_smbios_helper_with(&process) {
        SmbiosHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("could not spawn"));
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_maps_a_deadline_kill_to_helper_unavailable() {
    let process = FixedProcess::one_err(io::ErrorKind::TimedOut, "deadline");
    match invoke_smbios_helper_with(&process) {
        SmbiosHelperOutcome::Unavailable { reason, detail } => {
            assert_eq!(reason, EscalationDenialReason::HelperUnavailable);
            assert!(detail.contains("deadline"));
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn invoke_maps_helper_success_and_helper_error_verbatim() {
    let process = FixedProcess::one_ok(SUCCESS_FIXTURE, Some(0));
    match invoke_smbios_helper_with(&process) {
        SmbiosHelperOutcome::Success(success) => assert_eq!(success.slots_used, 2),
        other => panic!("expected Success, got {other:?}"),
    }
    let process = FixedProcess::one_ok(
        r#"{"status":"error","kind":"no_dmi","detail":"entries dir missing"}"#,
        Some(3),
    );
    match invoke_smbios_helper_with(&process) {
        SmbiosHelperOutcome::HelperError(error) => {
            assert_eq!(error.kind, SmbiosHelperErrorKind::NoDmi)
        }
        other => panic!("expected HelperError, got {other:?}"),
    }
}

#[test]
fn invoke_classifies_no_contract_replies_by_pkexec_exit_code() {
    let refusal = FixedProcess::one_ok("garbage", Some(126));
    match invoke_smbios_helper_with(&refusal) {
        SmbiosHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::PermissionDenied);
        }
        other => panic!("expected Unavailable for exit 126, got {other:?}"),
    }
    let broker = FixedProcess::one_ok("garbage", Some(127));
    match invoke_smbios_helper_with(&broker) {
        SmbiosHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::AuthorizationUnavailable);
        }
        other => panic!("expected Unavailable for exit 127, got {other:?}"),
    }
    let violation = FixedProcess::one_ok("", Some(9));
    match invoke_smbios_helper_with(&violation) {
        SmbiosHelperOutcome::Unavailable { reason, .. } => {
            assert_eq!(reason, EscalationDenialReason::HelperProtocolViolation);
        }
        other => panic!("expected Unavailable for exit 9, got {other:?}"),
    }
}

#[test]
fn the_production_driver_is_constructible_without_side_effects() {
    // Constructing the pkexec driver must never prompt; only `run` crosses.
    let _driver = PkexecSmbiosHelper::new();
}
