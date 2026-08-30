//! Fixture-driven contract tests for the Windows UAC transport seam
//! (ADR-035 stage 1). Everything here compiles and runs on ANY host: the
//! transport facts and mappings are pure data, and the fixture transport
//! returns canned [`UacCrossingObservation`]s — no real consent box, child
//! process, or reply channel is ever touched, and no Windows runtime behavior
//! is invented (that is stage-3 on-box receipt territory).

use super::*;
use crate::polkit::ForeignProcessControlFailure;
use std::sync::Mutex;

/// Fixture transport: records every request it crossed with, then answers
/// each call with the next canned observation.
struct FixedTransport {
    observations: Mutex<Vec<UacCrossingObservation>>,
    seen: Mutex<Vec<(u32, u64, String)>>,
}

impl FixedTransport {
    fn single(observation: UacCrossingObservation) -> Self {
        Self {
            observations: Mutex::new(vec![observation]),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<(u32, u64, String)> {
        self.seen.lock().expect("seen mutex").clone()
    }
}

impl UacForeignProcessControlTransport for FixedTransport {
    fn cross(
        &self,
        target: ForeignProcessControlTarget,
        operation: &ForeignProcessControlOperation,
    ) -> UacCrossingObservation {
        self.seen.lock().expect("seen mutex").push((
            target.pid(),
            target.start_token(),
            format!("{operation:?}"),
        ));
        self.observations
            .lock()
            .expect("observations mutex")
            .remove(0)
    }
}

fn target() -> ForeignProcessControlTarget {
    ForeignProcessControlTarget::new(42, 9_000).expect("valid target")
}

fn unavailable_reason(
    transport: &FixedTransport,
    operation: ForeignProcessControlOperation,
) -> EscalationDenialReason {
    match invoke_uac_foreign_process_control_with(transport, target(), operation) {
        ForeignProcessControlOutcome::Unavailable { reason, .. } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn transport_failure_facts_keep_their_distinct_denial_reasons() {
    // ADR-035's transport-fact table, row by row. Refusal (ERROR_CANCELLED),
    // no-consent environment, deadline, missing install
    // (ERROR_FILE_NOT_FOUND), non-contract reply, and the unwired transport
    // are SIX distinct facts that must never collapse into one
    // PermissionDenied or each other.
    for (observation, reason) in [
        (
            // UAC 明确拒绝（ERROR_CANCELLED）→ PermissionDenied
            UacCrossingObservation::LaunchFailed { win32_error: 1223 },
            EscalationDenialReason::PermissionDenied,
        ),
        (
            // 无提示环境 → AuthorizationUnavailable
            UacCrossingObservation::ConsentUnavailable,
            EscalationDenialReason::AuthorizationUnavailable,
        ),
        (
            // 跨界死线 → HelperUnavailable
            UacCrossingObservation::DeadlineExceeded,
            EscalationDenialReason::HelperUnavailable,
        ),
        (
            // helper 未安装 → HelperUnavailable
            UacCrossingObservation::LaunchFailed { win32_error: 2 },
            EscalationDenialReason::HelperUnavailable,
        ),
        (
            // 回传非契约 → HelperProtocolViolation
            UacCrossingObservation::HelperReply {
                payload: b"not a contract".to_vec(),
            },
            EscalationDenialReason::HelperProtocolViolation,
        ),
        (
            // transport 未接线（现状） → Unsupported
            UacCrossingObservation::TransportUnwired,
            EscalationDenialReason::Unsupported,
        ),
    ] {
        assert_eq!(
            unavailable_reason(
                &FixedTransport::single(observation),
                ForeignProcessControlOperation::Kill
            ),
            reason,
            "ADR-035 transport-fact row: {reason:?}"
        );
    }
}

#[test]
fn unclassified_launch_errors_stay_neutral_authorization_unavailable() {
    // pkexec-127 discipline: a launch error that is neither the user's
    // explicit refusal (1223) nor the missing helper (2) must not be
    // attributed to either side.
    for win32_error in [5, 1224, u32::MAX] {
        assert_eq!(
            launch_failure_reason(win32_error),
            EscalationDenialReason::AuthorizationUnavailable
        );
    }
    assert_eq!(
        launch_failure_reason(1223),
        EscalationDenialReason::PermissionDenied
    );
    assert_eq!(
        launch_failure_reason(2),
        EscalationDenialReason::HelperUnavailable
    );
}

#[test]
fn helper_contract_reply_maps_applied_and_typed_helper_failures() {
    // The reply channel carries the SAME contract as the Linux helper stdout;
    // valid messages keep the helper's own typed vocabulary.
    for (payload, expected) in [
        (
            r#"{"schema":1,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#,
            ForeignProcessControlOutcome::Applied,
        ),
        (
            r#"{"schema":1,"status":"error","kind":"permission_denied","detail":"protected process"}"#,
            ForeignProcessControlOutcome::Failed {
                kind: ForeignProcessControlFailure::PermissionDenied,
                detail: "protected process".to_owned(),
            },
        ),
    ] {
        let transport = FixedTransport::single(UacCrossingObservation::HelperReply {
            payload: payload.as_bytes().to_vec(),
        });
        assert_eq!(
            invoke_uac_foreign_process_control_with(
                &transport,
                target(),
                ForeignProcessControlOperation::Kill
            ),
            expected
        );
    }
}

#[test]
fn schema_drift_and_empty_replies_are_protocol_violations_never_success() {
    // A crashed helper delivers an empty reply; a future schema must not be
    // silently coerced into Applied. Both are "installed helper returned
    // without one valid contract message".
    for payload in [
        Vec::new(),
        br#"{"schema":2,"status":"applied","pid":42,"start_token":9000,"operation":"kill"}"#
            .to_vec(),
    ] {
        let transport = FixedTransport::single(UacCrossingObservation::HelperReply { payload });
        assert_eq!(
            unavailable_reason(&transport, ForeignProcessControlOperation::End),
            EscalationDenialReason::HelperProtocolViolation
        );
    }
}

#[test]
fn install_probe_reports_requires_escalation_only_when_fully_installed() {
    assert_eq!(
        probe_foreign_process_control_install(UacHelperInstallFacts {
            helper_present: true,
            manifest_consistent: true,
        }),
        EscalationAvailability::RequiresEscalation(EscalationFeature::ForeignProcessControl)
    );
}

#[test]
fn missing_install_is_helper_unavailable_not_unsupported() {
    // ADR-035: the transport EXISTS (stage 2 will wire it); a missing install
    // is the honest "this host cannot offer the crossing" answer — never
    // Unsupported, which would hide the install fix behind a permanent one.
    for facts in [
        UacHelperInstallFacts {
            helper_present: false,
            manifest_consistent: true,
        },
        UacHelperInstallFacts {
            helper_present: true,
            manifest_consistent: false,
        },
        UacHelperInstallFacts::default(),
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
fn crossing_hands_the_full_pid_and_creation_token_to_the_transport() {
    // ADR-035 identity discipline: the request is expressible on the seam as
    // PID + frozen creation token (Windows: the GetProcessTimes 100ns value),
    // both components cross unchanged, and the helper-side mismatch report
    // maps onto Failed{IdentityChanged} — the before/after identity path
    // without touching a real process.
    let creation_token_100ns: u64 = 1_335_832_810_000_000;
    let transport = FixedTransport::single(UacCrossingObservation::HelperReply {
        payload: br#"{"schema":1,"status":"error","kind":"identity_changed","detail":"reused"}"#
            .to_vec(),
    });
    let identity =
        ForeignProcessControlTarget::new(4242, creation_token_100ns).expect("valid target");
    assert_eq!(
        invoke_uac_foreign_process_control_with(
            &transport,
            identity,
            ForeignProcessControlOperation::End
        ),
        ForeignProcessControlOutcome::Failed {
            kind: ForeignProcessControlFailure::IdentityChanged,
            detail: "reused".to_owned()
        }
    );
    assert_eq!(
        transport.seen(),
        vec![(
            4242,
            creation_token_100ns,
            format!("{:?}", ForeignProcessControlOperation::End)
        )]
    );
}

#[test]
fn reply_channel_names_are_per_nonce_and_never_reuse_a_scheme() {
    // ADR-035 decision 4: the reply channel is per-call and randomly named.
    // The name is pure over the nonce; two distinct nonces must never collide
    // and the name must stay a single path component (no separators, no
    // traversal) so it can only ever land inside the caller's temp directory.
    let first = reply_channel_file_name(0);
    let second = reply_channel_file_name(1);
    assert_ne!(first, second);
    assert_eq!(
        first,
        "taskforest-uac-reply-0000000000000000.json".to_owned()
    );
    for nonce in [0_u64, 1, u64::MAX, 0x00ff_00ff_00ff_00ff] {
        let name = reply_channel_file_name(nonce);
        assert!(!name.contains('/'), "name must be one component: {name}");
        assert!(!name.contains('\\'), "name must be one component: {name}");
        assert!(!name.contains(".."), "no traversal: {name}");
        assert!(name.starts_with("taskforest-uac-reply-"));
        assert!(name.ends_with(".json"));
    }
}

#[test]
fn command_line_quotes_only_what_needs_quoting() {
    // Fixed numeric/operation arguments pass through verbatim; the
    // reply-channel path (which may contain spaces in a user's temp dir) is
    // quoted; embedded quotes escape with backslash-run doubling per the
    // documented Windows command-line rule.
    assert_eq!(quote_windows_argument("42"), "42");
    assert_eq!(quote_windows_argument("priority:-5"), "priority:-5");
    assert_eq!(
        quote_windows_argument(r"C:\Temp Reply Dir\r.json"),
        r#""C:\Temp Reply Dir\r.json""#
    );
    let embedded_quote = "a\"b";
    assert_eq!(quote_windows_argument(embedded_quote), "\"a\\\"b\"");
    // A backslash run at the END of a quoted argument is doubled so it cannot
    // escape the closing quote; an unquoted trailing backslash passes through
    // verbatim because nothing delimits it.
    let quoted_trailing_backslash = "My Dir\\";
    assert_eq!(
        quote_windows_argument(quoted_trailing_backslash),
        "\"My Dir\\\\\""
    );
    let bare_trailing_backslash = "trailing\\";
    assert_eq!(
        quote_windows_argument(bare_trailing_backslash),
        "trailing\\"
    );
    assert_eq!(quote_windows_argument(""), "\"\"");
}

#[test]
fn runas_command_line_carries_the_fixed_helper_contract_order() {
    // The helper's fixed argument order is pid, start token, operation wire
    // form, reply channel — identical to the pkexec crossing plus the
    // one-shot reply path, so one helper vocabulary serves both transports.
    let target = ForeignProcessControlTarget::new(4242, 9_000).expect("valid target");
    let reply = std::path::Path::new("/tmp/taskforest-uac-reply-0000000000000001.json");
    assert_eq!(
        runas_command_line(
            target,
            &ForeignProcessControlOperation::SetPriority(-5),
            reply
        ),
        "4242 9000 priority:-5 /tmp/taskforest-uac-reply-0000000000000001.json".to_owned()
    );
    // A reply path with whitespace stays ONE argument after quoting.
    let spaced = std::path::Path::new("/tmp/My Dir/r.json");
    let line = runas_command_line(target, &ForeignProcessControlOperation::Kill, spaced);
    assert_eq!(line, r#"4242 9000 kill "/tmp/My Dir/r.json""#);
}

#[test]
fn a_reply_channel_setup_failure_is_helper_unavailable_not_a_user_fact() {
    // The channel dying before launch is neither a refusal, nor a protocol
    // violation, nor a deadline: the crossing never started, and the honest
    // typed answer is that the crossing infrastructure is unreachable.
    let transport = FixedTransport::single(UacCrossingObservation::ReplyChannelUnavailable);
    assert_eq!(
        unavailable_reason(&transport, ForeignProcessControlOperation::Kill),
        EscalationDenialReason::HelperUnavailable
    );
}

#[test]
fn an_unwired_transport_composition_fails_closed_as_unsupported() {
    // Any adapter set that never registered the stage-2 runas driver keeps
    // the fail-closed default: typed Unsupported, no fabricated crossing.
    struct UnwiredTransport;
    impl UacForeignProcessControlTransport for UnwiredTransport {
        fn cross(
            &self,
            _target: ForeignProcessControlTarget,
            _operation: &ForeignProcessControlOperation,
        ) -> UacCrossingObservation {
            UacCrossingObservation::TransportUnwired
        }
    }
    assert!(matches!(
        invoke_uac_foreign_process_control_with(
            &UnwiredTransport,
            target(),
            ForeignProcessControlOperation::Kill
        ),
        ForeignProcessControlOutcome::Unavailable {
            reason: EscalationDenialReason::Unsupported,
            ..
        }
    ));
}
