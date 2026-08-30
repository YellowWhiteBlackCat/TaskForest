//! Tests for the `--msr` CLI mode's JSON rendering of the polkit MSR-helper
//! outcome: success readouts (values and honest nulls), contract error
//! round-trips, and the no-fabricated-readings honesty rule.

use super::*;
use serde_json::Value;
use taskmanager_escalation::polkit::{
    MsrHelperError, MsrHelperErrorKind, MsrPackageReading, MsrReadoutSuccess,
};

fn all_contract_error_kinds() -> [MsrHelperErrorKind; 4] {
    [
        MsrHelperErrorKind::PermissionDenied,
        MsrHelperErrorKind::NoMsr,
        MsrHelperErrorKind::OpenFailed,
        MsrHelperErrorKind::ReadFailed,
    ]
}

#[test]
fn render_success_lists_readouts_and_keeps_absent_fields_null() {
    let outcome = MsrHelperOutcome::Success(MsrReadoutSuccess {
        schema: 1,
        packages: vec![
            MsrPackageReading {
                cpu: 0,
                bclk_mhz: None,
                temperature_c: Some(58.0),
                multiplier: Some(45.0),
                multiplier_min: Some(8.0),
                multiplier_max: Some(55.0),
                vcore_v: Some(1.21875),
            },
            MsrPackageReading {
                cpu: 1,
                bclk_mhz: None,
                temperature_c: None,
                multiplier: None,
                multiplier_min: None,
                multiplier_max: None,
                vcore_v: None,
            },
        ],
    });
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["packages"][0]["cpu"], 0);
    assert_eq!(value["packages"][0]["temperature_c"], 58.0);
    assert_eq!(value["packages"][0]["multiplier"], 45.0);
    assert_eq!(value["packages"][0]["multiplier_min"], 8.0);
    assert_eq!(value["packages"][0]["multiplier_max"], 55.0);
    // The document rounds each quantity to its display resolution instead of
    // echoing the f32 artifact (1.21875 → 1.219).
    assert_eq!(value["packages"][0]["vcore_v"], 1.219);
    // bclk is excluded by ADR-048: it must render null, never a guess.
    assert!(value["packages"][0]["bclk_mhz"].is_null());
    // A node without implemented registers renders all-null fields.
    assert!(value["packages"][1]["temperature_c"].is_null());
    assert!(value["packages"][1]["multiplier"].is_null());
}

#[test]
fn render_success_with_no_packages_is_honest_empty_array() {
    let outcome = MsrHelperOutcome::Success(MsrReadoutSuccess {
        schema: 1,
        packages: Vec::new(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&outcome)).expect("rendered document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["packages"].as_array().map(Vec::len), Some(0));
}

#[test]
fn render_helper_error_each_kind_round_trips_contract_string() {
    for kind in all_contract_error_kinds() {
        let outcome = MsrHelperOutcome::HelperError(MsrHelperError {
            kind,
            detail: "detail text".to_owned(),
        });
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered error document must parse");
        assert_eq!(value["status"], "error");
        assert_eq!(value["kind"], kind.as_contract_str());
        assert_eq!(value["detail"], "detail text");
    }
}

#[test]
fn render_unavailable_carries_typed_reason_and_feature() {
    for (reason, expected) in [
        (EscalationDenialReason::Unsupported, "unsupported"),
        (
            EscalationDenialReason::PermissionDenied,
            "permission_denied",
        ),
        (
            EscalationDenialReason::AuthorizationUnavailable,
            "authorization_unavailable",
        ),
        (
            EscalationDenialReason::HelperUnavailable,
            "helper_unavailable",
        ),
        (
            EscalationDenialReason::HelperProtocolViolation,
            "helper_protocol_violation",
        ),
    ] {
        let outcome = MsrHelperOutcome::Unavailable {
            reason,
            detail: "why".to_owned(),
        };
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered unavailable document must parse");
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["reason"], expected);
        assert_eq!(value["feature"], "cpu_msr");
        assert_eq!(value["detail"], "why");
    }
}

#[test]
fn render_never_emits_a_fabricated_readout() {
    // Every non-success outcome MUST omit "packages" entirely; a fabricated
    // zero-temperature row here would violate the honesty red line.
    let denied = MsrHelperOutcome::Unavailable {
        reason: EscalationDenialReason::PermissionDenied,
        detail: "declined".to_owned(),
    };
    let value: Value =
        serde_json::from_str(&render_outcome(&denied)).expect("rendered document must parse");
    assert!(
        value.get("packages").is_none(),
        "a denial must never carry a fabricated packages array",
    );

    let helper_err = MsrHelperOutcome::HelperError(MsrHelperError {
        kind: MsrHelperErrorKind::NoMsr,
        detail: "none".to_owned(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&helper_err)).expect("rendered document must parse");
    assert!(
        value.get("packages").is_none(),
        "a helper error must never carry a fabricated packages array",
    );
}
