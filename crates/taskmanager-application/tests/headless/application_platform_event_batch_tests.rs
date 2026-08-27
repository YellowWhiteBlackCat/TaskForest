use taskmanager_core::FailureKind;
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, OperationFailure, PartialSourceSnapshot, RequestId,
    RetryDisposition,
};

use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext, ServiceEvent, ShellEvent};

#[test]
fn default_batch_is_empty() {
    assert!(PlatformEventBatch::default().is_empty());
}

fn context(sequence: u64, capability: CapabilityId) -> PlatformEventContext {
    PlatformEventContext {
        request_id: RequestId::new(sequence).expect("non-zero sequence fixture"),
        capability,
        provider: None,
        sequence: EventSequence::new(sequence),
        observed_at_ms: sequence,
    }
}

fn failure(sequence: u64) -> OperationFailure {
    OperationFailure {
        request_id: RequestId::new(sequence).expect("non-zero sequence fixture"),
        capability: CapabilityId::PROCESS_CONTROL,
        sequence: EventSequence::new(sequence),
        kind: FailureKind::TimedOut,
        retry: RetryDisposition::RetryLater,
        provider: None,
        observed_at_ms: sequence,
    }
}

#[test]
fn domain_ordering_uses_sequence_without_inventing_cross_domain_precedence() {
    let mut batch = PlatformEventBatch::default();
    for sequence in [4, 2] {
        batch.shell_events.push(CorrelatedEvent::new(
            context(sequence, CapabilityId::COMMAND_LAUNCH),
            ShellEvent::CommandLaunched {
                pid: sequence as u32,
            },
        ));
    }
    for sequence in [3, 1] {
        batch.service_events.push(CorrelatedEvent::new(
            context(sequence, CapabilityId::SERVICES),
            ServiceEvent::Snapshot(PartialSourceSnapshot::new(Vec::new(), Vec::new())),
        ));
    }
    batch.failures.extend([failure(6), failure(5)]);

    let ordered = batch.into_domain_ordered();

    assert_eq!(
        ordered
            .shell_events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [EventSequence::new(2), EventSequence::new(4)]
    );
    assert_eq!(
        ordered
            .service_events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [EventSequence::new(1), EventSequence::new(3)]
    );
    assert_eq!(
        ordered
            .failures
            .iter()
            .map(|failure| failure.sequence)
            .collect::<Vec<_>>(),
        [EventSequence::new(5), EventSequence::new(6)]
    );
}
