use super::*;

pub(super) fn test_event_context(
    request_id: RequestId,
    capability: CapabilityId,
) -> PlatformEventContext {
    PlatformEventContext {
        request_id,
        capability,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1,
    }
}
