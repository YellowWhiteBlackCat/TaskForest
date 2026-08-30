//! Tests for the `--package-power` CLI mode's JSON rendering of the polkit
//! RAPL-helper outcome: success readings, contract error round-trips, and the
//! no-fabricated-watts honesty rule.

use super::*;
use serde_json::Value;
use taskmanager_escalation::polkit::{
    RaplHelperError, RaplHelperErrorKind, RaplPackageReading, RaplPowerSuccess,
};

fn all_contract_error_kinds() -> [RaplHelperErrorKind; 4] {
    [
        RaplHelperErrorKind::PermissionDenied,
        RaplHelperErrorKind::NoRapl,
        RaplHelperErrorKind::OpenFailed,
        RaplHelperErrorKind::ReadFailed,
    ]
}

#[test]
fn render_success_lists_packages() {
    let outcome = RaplHelperOutcome::Success(RaplPowerSuccess {
        schema: 1,
        sample_ms: 1000,
        packages: vec![
            RaplPackageReading {
                name: "package-0".to_owned(),
                power_w: 12.5,
                energy_delta_uj: 12_500_000,
            },
            RaplPackageReading {
                name: "package-1".to_owned(),
                power_w: 0.0,
                energy_delta_uj: 0,
            },
        ],
    });
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["sample_ms"], 1000);
    assert_eq!(value["packages"][0]["name"], "package-0");
    assert_eq!(value["packages"][0]["power_w"], 12.5);
    assert_eq!(value["packages"][0]["energy_delta_uj"], 12_500_000);
    assert_eq!(value["packages"][1]["power_w"], 0.0);
}

#[test]
fn render_success_with_no_packages_is_honest_empty_array() {
    let outcome = RaplHelperOutcome::Success(RaplPowerSuccess {
        schema: 1,
        sample_ms: 1000,
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
        let outcome = RaplHelperOutcome::HelperError(RaplHelperError {
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
        let outcome = RaplHelperOutcome::Unavailable {
            reason,
            detail: "why".to_owned(),
        };
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered unavailable document must parse");
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["reason"], expected);
        assert_eq!(value["feature"], "package_power_rapl");
        assert_eq!(value["detail"], "why");
    }
}

#[test]
fn render_never_emits_a_fabricated_watt_figure() {
    // Every non-success outcome MUST omit "packages" entirely; a fabricated
    // zero-watt row here would violate the honesty red line.
    let denied = RaplHelperOutcome::Unavailable {
        reason: EscalationDenialReason::PermissionDenied,
        detail: "declined".to_owned(),
    };
    let value: Value =
        serde_json::from_str(&render_outcome(&denied)).expect("rendered document must parse");
    assert!(
        value.get("packages").is_none(),
        "a denial must never carry a fabricated packages array",
    );

    let helper_err = RaplHelperOutcome::HelperError(RaplHelperError {
        kind: RaplHelperErrorKind::NoRapl,
        detail: "none".to_owned(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&helper_err)).expect("rendered document must parse");
    assert!(
        value.get("packages").is_none(),
        "a helper error must never carry a fabricated packages array",
    );
}
