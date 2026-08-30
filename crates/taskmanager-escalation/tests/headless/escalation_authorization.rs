//! Fixture-driven contract tests for the macOS authorization transport seam.
//! Everything here compiles and runs on ANY host: the transport facts and
//! mappings are pure data, and the fixture transport returns canned
//! [`MacAuthorizationObservation`]s — no real authorization dialog, daemon,
//! or reply channel is ever touched, and no macOS runtime behavior is
//! invented (that is on-box receipt territory behind a future signed-helper
//! ADR).

use super::*;
use crate::polkit::ForeignProcessControlFailure;

/// Fixture transport: answers every call with one canned observation.
struct FixedTransport {
    observation: MacAuthorizationObservation,
}

impl MacAuthorizationForeignProcessControlTransport for FixedTransport {
    fn cross(
        &self,
        _target: ForeignProcessControlTarget,
        _operation: &ForeignProcessControlOperation,
    ) -> MacAuthorizationObservation {
        self.observation.clone()
    }
}

fn target() -> ForeignProcessControlTarget {
    ForeignProcessControlTarget::new(42, 9_000).expect("valid target")
}

fn denial_reason(observation: MacAuthorizationObservation) -> EscalationDenialReason {
    let transport = FixedTransport { observation };
    match invoke_mac_foreign_process_control_with(
        &transport,
        target(),
        ForeignProcessControlOperation::Kill,
    ) {
        ForeignProcessControlOutcome::Unavailable { reason, .. } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn transport_failure_facts_keep_their_distinct_denial_reasons() {
    // User refusal (canceled AND denied), no-dialog environment, broker
    // failure, missing daemon, deadline, non-contract reply, and the unwired
    // transport are SEVEN distinct facts that must never collapse into one
    // PermissionDenied or each other.
    for (observation, reason) in [
        (
            // 用户明确取消/拒绝 → PermissionDenied
            MacAuthorizationObservation::AuthorizationFailed { osstatus: -60006 },
            EscalationDenialReason::PermissionDenied,
        ),
        (
            MacAuthorizationObservation::AuthorizationFailed { osstatus: -60007 },
            EscalationDenialReason::PermissionDenied,
        ),
        (
            // 无对话框环境 → AuthorizationUnavailable
            MacAuthorizationObservation::ConsentUnavailable,
            EscalationDenialReason::AuthorizationUnavailable,
        ),
        (
            // 授权服务不可达（errAuthorizationNotAvailable）→ 中性不可用
            MacAuthorizationObservation::AuthorizationFailed { osstatus: -60022 },
            EscalationDenialReason::AuthorizationUnavailable,
        ),
        (
            // 未归因失败 → 中性，不得臆测是用户拒绝
            MacAuthorizationObservation::AuthorizationFailed { osstatus: -25244 },
            EscalationDenialReason::AuthorizationUnavailable,
        ),
        (
            // 已签名 helper 未安装 → HelperUnavailable
            MacAuthorizationObservation::HelperNotInstalled,
            EscalationDenialReason::HelperUnavailable,
        ),
        (
            // 跨界死线 → HelperUnavailable
            MacAuthorizationObservation::DeadlineExceeded,
            EscalationDenialReason::HelperUnavailable,
        ),
        (
            // 回传非契约 → HelperProtocolViolation
            MacAuthorizationObservation::HelperReply {
                payload: b"not a contract".to_vec(),
            },
            EscalationDenialReason::HelperProtocolViolation,
        ),
        (
            // transport 未接线（现状） → Unsupported
            MacAuthorizationObservation::TransportUnwired,
            EscalationDenialReason::Unsupported,
        ),
    ] {
        assert_eq!(
            denial_reason(observation),
            reason,
            "observation must map to {reason:?}"
        );
    }
}

#[test]
fn helper_contract_reply_maps_applied_and_typed_helper_failures() {
    // The reply channel carries the SAME contract as the Linux helper stdout;
    // valid messages keep the helper's own typed vocabulary, including the
    // identity discipline (a token mismatch is Failed{IdentityChanged}, never
    // success).
    for (payload, expected) in [
        (
            r#"{"schema":1,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#
                .as_bytes()
                .to_vec(),
            ForeignProcessControlOutcome::Applied,
        ),
        (
            br#"{"schema":1,"status":"error","kind":"identity_changed","detail":"reused"}"#
                .to_vec(),
            ForeignProcessControlOutcome::Failed {
                kind: ForeignProcessControlFailure::IdentityChanged,
                detail: "reused".to_owned(),
            },
        ),
    ] {
        let transport = FixedTransport {
            observation: MacAuthorizationObservation::HelperReply { payload },
        };
        assert_eq!(
            invoke_mac_foreign_process_control_with(
                &transport,
                target(),
                ForeignProcessControlOperation::Kill
            ),
            expected
        );
    }
}

#[test]
fn empty_and_schema_drifted_replies_are_protocol_violations_never_success() {
    // A crashed helper delivers an empty reply; a future schema must not be
    // silently coerced into Applied.
    for payload in [
        Vec::new(),
        br#"{"schema":2,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#
            .to_vec(),
    ] {
        assert_eq!(
            denial_reason(MacAuthorizationObservation::HelperReply { payload }),
            EscalationDenialReason::HelperProtocolViolation
        );
    }
}

#[test]
fn install_probe_reports_requires_escalation_only_when_fully_installed() {
    assert_eq!(
        probe_foreign_process_control_install(MacHelperInstallFacts {
            helper_installed: true,
            registration_consistent: true,
        }),
        EscalationAvailability::RequiresEscalation(EscalationFeature::ForeignProcessControl)
    );
    for facts in [
        MacHelperInstallFacts {
            helper_installed: false,
            registration_consistent: true,
        },
        MacHelperInstallFacts {
            helper_installed: true,
            registration_consistent: false,
        },
        MacHelperInstallFacts::default(),
    ] {
        assert_eq!(
            probe_foreign_process_control_install(facts),
            EscalationAvailability::Denied {
                reason: EscalationDenialReason::HelperUnavailable
            },
            "facts: {facts:?}"
        );
    }
}

#[test]
fn the_production_entry_fails_closed_as_unsupported() {
    // No signed-helper ADR exists, so the honest production answer is typed
    // Unsupported without touching a dialog, daemon, or child process.
    assert!(matches!(
        invoke_mac_foreign_process_control(target(), ForeignProcessControlOperation::Kill),
        ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::Unsupported,
            ..
        }
    ));
}
