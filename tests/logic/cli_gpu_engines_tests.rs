//! Tests for the `--gpu-engines` CLI mode's JSON rendering of the polkit
//! perf-helper outcome: success listings, contract error round-trips, and the
//! no-fabricated-rows honesty rule.

use super::*;
use serde_json::Value;
use taskmanager_escalation::polkit::{
    EngineReading, PerfHelperError, PerfHelperErrorKind, PerfHelperSuccess,
};

/// The set of contract error kinds the helper may emit, used by the
/// round-trip tests below to keep the CLI rendering in lockstep with the
/// escalation parser.
fn all_contract_error_kinds() -> [PerfHelperErrorKind; 4] {
    [
        PerfHelperErrorKind::PermissionDenied,
        PerfHelperErrorKind::NoPmu,
        PerfHelperErrorKind::OpenFailed,
        PerfHelperErrorKind::ReadFailed,
    ]
}

#[test]
fn render_success_lists_engines() {
    let outcome = PerfHelperOutcome::Success(PerfHelperSuccess {
        schema: 1,
        driver: "xe".to_owned(),
        sample_ms: 100,
        engines: vec![
            EngineReading {
                name: "Render Ring".to_owned(),
                class: "rcs".to_owned(),
                busy_pct: 42.5,
            },
            EngineReading {
                name: "Blitter".to_owned(),
                class: "bcs".to_owned(),
                busy_pct: 0.0,
            },
        ],
    });
    let value: Value = serde_json::from_str(&render_outcome(&outcome))
        .expect("rendered success document must parse");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["driver"], "xe");
    assert_eq!(value["sample_ms"], 100);
    assert_eq!(value["engines"][0]["name"], "Render Ring");
    assert_eq!(value["engines"][0]["busy_pct"], 42.5);
    assert_eq!(value["engines"][1]["busy_pct"], 0.0);
}

#[test]
fn render_success_with_no_engines_is_honest_empty_array() {
    let outcome = PerfHelperOutcome::Success(PerfHelperSuccess {
        schema: 1,
        driver: "i915".to_owned(),
        sample_ms: 1,
        engines: Vec::new(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&outcome)).expect("rendered document must parse");
    assert_eq!(value["status"], "ok");
    assert!(value["engines"].is_array());
    assert_eq!(value["engines"].as_array().map(Vec::len), Some(0));
}

#[test]
fn render_helper_error_each_kind_round_trips_contract_string() {
    for kind in all_contract_error_kinds() {
        let outcome = PerfHelperOutcome::HelperError(PerfHelperError {
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
        let outcome = PerfHelperOutcome::Unavailable {
            reason,
            detail: "why".to_owned(),
        };
        let value: Value = serde_json::from_str(&render_outcome(&outcome))
            .expect("rendered unavailable document must parse");
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["reason"], expected);
        assert_eq!(value["feature"], "intel_pmu");
        assert_eq!(value["detail"], "why");
    }
}

#[test]
fn render_never_emits_a_fabricated_engine_row() {
    // Every non-success outcome MUST omit "engines" entirely; a fabricated
    // zero-valued row here would violate the honesty red line.
    let denied = PerfHelperOutcome::Unavailable {
        reason: EscalationDenialReason::PermissionDenied,
        detail: "declined".to_owned(),
    };
    let value: Value =
        serde_json::from_str(&render_outcome(&denied)).expect("rendered document must parse");
    assert!(
        value.get("engines").is_none(),
        "a denial must never carry a fabricated engines array",
    );

    let helper_err = PerfHelperOutcome::HelperError(PerfHelperError {
        kind: PerfHelperErrorKind::NoPmu,
        detail: "none".to_owned(),
    });
    let value: Value =
        serde_json::from_str(&render_outcome(&helper_err)).expect("rendered document must parse");
    assert!(
        value.get("engines").is_none(),
        "a helper error must never carry a fabricated engines array",
    );
}
