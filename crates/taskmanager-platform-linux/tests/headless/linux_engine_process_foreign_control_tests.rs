use super::*;
use taskmanager_core::PriorityTier;

fn target() -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "worker", 1, 9_000).expect("valid identity")
}

#[test]
fn batch_and_signal_operations_preserve_typed_action() {
    assert_eq!(
        batch_operation(ProcessBatchAction::SetPriority(PriorityTier::High)),
        ForeignProcessControlOperation::SetPriority(-10)
    );
    assert_eq!(
        signal_operation(ProcessSignal::User2),
        ForeignProcessControlOperation::Signal(
            taskmanager_escalation::polkit::ForeignProcessSignal::User2
        )
    );
    assert_eq!(
        affinity_operation(&[1, 3]),
        ForeignProcessControlOperation::SetAffinity(vec![1, 3])
    );
}

#[test]
fn successful_direct_control_never_invokes_escalation() {
    assert_eq!(
        finish_with_escalation(&target(), ForeignProcessControlOperation::Kill, Ok(())),
        Ok(())
    );
}

#[test]
fn invalid_frozen_identity_blocks_the_escalation_attempt() {
    let invalid = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 1, 0);
    assert!(invalid.is_none());
    let decoded: FrozenProcessIdentity = serde_json::from_str(
        r#"{"pid":42,"name":"worker","start_time_secs":1,"start_token":null}"#,
    )
    .expect("legacy identity decodes");
    assert_eq!(
        finish_with_escalation(
            &decoded,
            ForeignProcessControlOperation::Kill,
            Err(FailureKind::PermissionDenied)
        ),
        Err(FailureKind::IdentityChanged)
    );
}
