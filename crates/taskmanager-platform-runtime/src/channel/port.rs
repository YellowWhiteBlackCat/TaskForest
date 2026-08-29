//! Bounded `RequestPort` adapter and typed lane factory.
//!
//! `ChannelRequestPort` validates each request's capability id against the
//! lane's request type and maps `Full`/`Disconnected` send errors to `Busy`/
//! `RuntimeStopped` submission errors; `request_lane` builds the bounded
//! `(Sender, Receiver)` pair for a present provider.

use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityRequest, RequestEnvelope, RequestPort, RequestTracking, SubmissionError,
    SubmissionErrorKind,
};

use super::lanes::Queued;

use crate::delivery::LaneStartRegistry;
use crate::ecs::{EcsAdmissionError, RuntimeEcsSchedulerHandle};

type EcsSchedulerHandle = RuntimeEcsSchedulerHandle;

pub(super) struct ChannelRequestPort<R> {
    sender: Sender<Queued<R>>,
    provider: ProviderId,
    scheduler: EcsSchedulerHandle,
    lane_starters: Arc<LaneStartRegistry>,
}

type OptionalRequestLane<R> = (
    Option<Arc<ChannelRequestPort<R>>>,
    Option<Receiver<Queued<R>>>,
);

impl<R> RequestPort for ChannelRequestPort<R>
where
    R: CapabilityRequest,
{
    type Request = R;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        let capability = R::CAPABILITY.clone();
        if request.capability != capability {
            return Err(SubmissionError {
                capability: request.capability,
                kind: SubmissionErrorKind::InvalidRequest,
            });
        }
        self.lane_starters
            .ensure_started(&capability)
            .map_err(|_| SubmissionError {
                capability: capability.clone(),
                kind: SubmissionErrorKind::RuntimeStopped,
            })?;
        let request_id = request.id;
        let tracking = request
            .payload
            .runtime_tracking()
            .map_err(|_| SubmissionError {
                capability: capability.clone(),
                kind: SubmissionErrorKind::InvalidRequest,
            })?;
        let owns_lifecycle = !matches!(&tracking, RequestTracking::Sideband);
        let scheduler_capability = capability.clone();
        let submitted_at_monotonic_ms = self.scheduler.now_ms();
        let mut scheduler = self.scheduler.lock().map_err(|_| SubmissionError {
            capability: capability.clone(),
            kind: SubmissionErrorKind::RuntimeStopped,
        })?;
        scheduler
            .admit_submission_with_tracking(
                &scheduler_capability,
                request_id,
                submitted_at_monotonic_ms,
                tracking,
            )
            .map_err(|cause| SubmissionError {
                capability: capability.clone(),
                kind: admission_error_kind(cause),
            })?;
        drop(scheduler);
        let result = self
            .sender
            .try_send(Queued {
                request_id: request.id,
                capability: capability.clone(),
                provider: self.provider.clone(),
                payload: request.payload,
            })
            .map_err(|error| SubmissionError {
                capability,
                kind: match error {
                    TrySendError::Full(_) => SubmissionErrorKind::Busy,
                    TrySendError::Disconnected(_) => SubmissionErrorKind::RuntimeStopped,
                },
            });
        if result.is_err()
            && owns_lifecycle
            && let Ok(mut scheduler) = self.scheduler.lock()
        {
            let failed_at_monotonic_ms = self.scheduler.now_ms();
            let _ = scheduler.cancel_submission(
                &scheduler_capability,
                request_id,
                failed_at_monotonic_ms,
            );
        }
        result
    }
}

const fn admission_error_kind(error: EcsAdmissionError) -> SubmissionErrorKind {
    match error {
        EcsAdmissionError::CapabilityInFlight
        | EcsAdmissionError::CapabilityStalled
        | EcsAdmissionError::CapabilityBlocked
        | EcsAdmissionError::DuplicateRequest
        | EcsAdmissionError::TargetInFlight
        | EcsAdmissionError::TargetCapacity
        | EcsAdmissionError::GlobalTargetCapacity
        | EcsAdmissionError::DomainTargetCapacity
        | EcsAdmissionError::TargetScopeByteCapacity
        | EcsAdmissionError::ControlDeliveryCapacity
        | EcsAdmissionError::ObservationDeliveryCapacity => SubmissionErrorKind::Busy,
        EcsAdmissionError::SidebandNotAllowed => SubmissionErrorKind::InvalidRequest,
        EcsAdmissionError::UnknownCapability | EcsAdmissionError::InvariantViolation => {
            SubmissionErrorKind::RuntimeStopped
        }
    }
}

pub(super) fn request_lane<R>(
    capacity: usize,
    provider: Option<&ProviderId>,
    scheduler: EcsSchedulerHandle,
    lane_starters: Arc<LaneStartRegistry>,
) -> OptionalRequestLane<R>
where
    R: CapabilityRequest,
{
    let Some(provider) = provider else {
        return (None, None);
    };
    let (sender, receiver) = bounded(capacity);
    (
        Some(Arc::new(ChannelRequestPort {
            sender,
            provider: provider.clone(),
            scheduler,
            lane_starters,
        })),
        Some(receiver),
    )
}

#[cfg(test)]
#[path = "../../tests/headless/runtime_channel_port_tests.rs"]
mod tests;
