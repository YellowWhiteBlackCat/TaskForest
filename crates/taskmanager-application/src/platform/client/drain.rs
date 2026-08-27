//! Bounded event draining and correlated projection updates.

use taskmanager_platform_contract::{
    CapabilityId, EventPortError, OperationFailure, ProviderFailure, RequestId,
};

use crate::platform::{
    CorrelatedEvent, PlatformEvent, PlatformEventBatch, PlatformEventContext, ProcessInsightFacet,
    ProcessInsightFacetEvent, ProcessInsightUnavailable, ProcessInsightsProjectionApplyResult,
    ProcessInsightsProjectionRejection, StartupEvidenceUnavailable, SystemTelemetryDomainOutcome,
    SystemTelemetryUnavailable,
};

use super::PlatformClient;
use super::startup_projection::append_startup_evidence_projection;
use super::system_projection::{append_system_projection, system_telemetry_domain};

const MAX_EVENTS_PER_DRAIN: usize = 64;

struct ProcessInsightDrainOutcome {
    projection: Option<ProcessInsightsProjectionApplyResult>,
    diagnostic: Option<taskmanager_core::FailureKind>,
}

impl PlatformClient {
    pub fn try_drain(&mut self) -> Result<PlatformEventBatch, EventPortError> {
        let mut batch = PlatformEventBatch::default();
        for _ in 0..MAX_EVENTS_PER_DRAIN {
            let Some(event) = self.handle.events().try_recv()? else {
                break;
            };
            let context = PlatformEventContext::from_envelope(&event);
            if !event.has_consistent_failure_metadata() {
                batch.failures.push(operation_failure(
                    &context,
                    taskmanager_core::FailureKind::ProviderFault,
                ));
                continue;
            }
            match event.outcome {
                Ok(payload) => {
                    let application_reduced = matches!(
                        &payload,
                        PlatformEvent::SystemTelemetry(_)
                            | PlatformEvent::ProcessInsightFacet(_)
                            | PlatformEvent::StartupEvidence(_)
                    );
                    if !application_reduced && !payload.accepts_capability(&context.capability) {
                        batch.failures.push(operation_failure(
                            &context,
                            taskmanager_core::FailureKind::ProviderFault,
                        ));
                        continue;
                    }
                    let process_outcome = match &payload {
                        PlatformEvent::ProcessInsightFacet(event) => self
                            .apply_process_insight_event(
                                context.request_id,
                                &context.capability,
                                event,
                            ),
                        _ => ProcessInsightDrainOutcome {
                            projection: None,
                            diagnostic: None,
                        },
                    };
                    let is_raw_process_facet =
                        matches!(&payload, PlatformEvent::ProcessInsightFacet(_));
                    let startup_outcome = match &payload {
                        PlatformEvent::StartupEvidence(event) => {
                            Some(self.apply_startup_evidence_event(
                                context.request_id,
                                &context.capability,
                                event,
                                context.observed_at_ms,
                            ))
                        }
                        _ => None,
                    };
                    let is_raw_startup_evidence =
                        matches!(&payload, PlatformEvent::StartupEvidence(_));
                    let mut system_projection = match &payload {
                        PlatformEvent::SystemTelemetry(event) => {
                            Some(self.apply_system_telemetry_event(
                                context.request_id,
                                &context.capability,
                                event,
                            ))
                        }
                        _ => None,
                    };
                    if let Some(outcome) = system_projection
                        .as_mut()
                        .and_then(|result| result.outcome.take())
                    {
                        batch
                            .system_telemetry_outcomes
                            .push(CorrelatedEvent::new(context.clone(), outcome));
                    }
                    let accept_raw_event = !is_raw_process_facet
                        && !is_raw_startup_evidence
                        && system_projection
                            .as_ref()
                            .is_none_or(|outcome| outcome.rejection.is_none());
                    if accept_raw_event {
                        batch.merge(context.clone(), payload);
                    } else if let Some(kind) = system_projection
                        .as_ref()
                        .and_then(|outcome| outcome.rejection)
                    {
                        batch.failures.push(operation_failure(&context, kind));
                    }
                    if let Some(kind) = process_outcome.diagnostic {
                        batch.failures.push(operation_failure(&context, kind));
                    }
                    if let Some(kind) = startup_outcome
                        .as_ref()
                        .and_then(|outcome| outcome.diagnostic)
                    {
                        batch.failures.push(operation_failure(&context, kind));
                    }
                    match process_outcome.projection {
                        Some(ProcessInsightsProjectionApplyResult::AppliedPartial(projection)) => {
                            batch.process_insight_projections.push(*projection);
                        }
                        Some(ProcessInsightsProjectionApplyResult::AppliedComplete {
                            projection,
                            ..
                        }) => {
                            batch.process_insight_projections.push(*projection);
                        }
                        Some(ProcessInsightsProjectionApplyResult::Ignored(_)) | None => {}
                    }
                    append_system_projection(
                        &mut batch,
                        system_projection.and_then(|outcome| outcome.projection),
                    );
                    append_startup_evidence_projection(
                        &mut batch,
                        startup_outcome.and_then(|outcome| outcome.projection),
                    );
                }
                Err(failure) => {
                    if failure.request_id == context.request_id
                        && let Some(pending) =
                            self.process_insight_requests.remove(&context.request_id)
                    {
                        let reason = if process_insight_facet(&context.capability)
                            == Some(pending.facet)
                            && process_insight_facet(&failure.capability) == Some(pending.facet)
                        {
                            failure.kind
                        } else {
                            taskmanager_core::FailureKind::ProviderFault
                        };
                        let applied = self.process_insights_projection.apply_failure(
                            &pending.target,
                            pending.revision,
                            pending.facet,
                            ProcessInsightUnavailable::Provider(reason),
                        );
                        match applied {
                            ProcessInsightsProjectionApplyResult::AppliedPartial(projection)
                            | ProcessInsightsProjectionApplyResult::AppliedComplete {
                                projection,
                                ..
                            } => batch.process_insight_projections.push(*projection),
                            ProcessInsightsProjectionApplyResult::Ignored(_) => {}
                        }
                    }
                    if failure.request_id == context.request_id
                        && let Some(pending) =
                            self.system_telemetry_requests.remove(&context.request_id)
                    {
                        let reason = if system_telemetry_domain(&context.capability)
                            == Some(pending.domain)
                            && system_telemetry_domain(&failure.capability) == Some(pending.domain)
                        {
                            failure.kind
                        } else {
                            taskmanager_core::FailureKind::ProviderFault
                        };
                        let applied = self.system_telemetry_projection.apply_failure(
                            pending.revision,
                            pending.domain,
                            SystemTelemetryUnavailable::Provider(reason),
                        );
                        batch.system_telemetry_outcomes.push(CorrelatedEvent::new(
                            context.clone(),
                            SystemTelemetryDomainOutcome::Unavailable {
                                revision: pending.revision,
                                domain: pending.domain,
                                reason: SystemTelemetryUnavailable::Provider(reason),
                            },
                        ));
                        append_system_projection(&mut batch, Some(applied));
                    }
                    if failure.request_id == context.request_id
                        && let Some(revision) =
                            self.startup_evidence_requests.remove(&context.request_id)
                    {
                        let reason = if context.capability == CapabilityId::STARTUP_EVIDENCE
                            && failure.capability == CapabilityId::STARTUP_EVIDENCE
                        {
                            failure.kind
                        } else {
                            taskmanager_core::FailureKind::ProviderFault
                        };
                        let applied = self.startup_evidence_projection.apply_failure(
                            revision,
                            StartupEvidenceUnavailable::Provider(reason),
                            context.observed_at_ms,
                        );
                        append_startup_evidence_projection(&mut batch, Some(applied));
                    }
                    batch.failures.push(failure);
                }
            }
        }
        Ok(batch.into_domain_ordered())
    }

    fn apply_process_insight_event(
        &mut self,
        request_id: RequestId,
        capability: &CapabilityId,
        event: &ProcessInsightFacetEvent,
    ) -> ProcessInsightDrainOutcome {
        let Some(pending) = self.process_insight_requests.remove(&request_id) else {
            return ProcessInsightDrainOutcome {
                projection: None,
                diagnostic: Some(taskmanager_core::FailureKind::Rejected),
            };
        };
        if process_insight_facet(capability) != Some(pending.facet)
            || pending.facet != event.facet()
        {
            return ProcessInsightDrainOutcome {
                projection: Some(self.process_insights_projection.apply_failure(
                    &pending.target,
                    pending.revision,
                    pending.facet,
                    ProcessInsightUnavailable::Provider(
                        taskmanager_core::FailureKind::ProviderFault,
                    ),
                )),
                diagnostic: Some(taskmanager_core::FailureKind::ProviderFault),
            };
        }
        let applied = self.process_insights_projection.apply(event);
        let ProcessInsightsProjectionApplyResult::Ignored(rejection) = applied else {
            return ProcessInsightDrainOutcome {
                projection: Some(applied),
                diagnostic: None,
            };
        };
        let failure = match rejection {
            ProcessInsightsProjectionRejection::DifferentFrozenTarget
            | ProcessInsightsProjectionRejection::StaleOrUnexpectedRevision
            | ProcessInsightsProjectionRejection::ConflictingRawIdentity => {
                taskmanager_core::FailureKind::IdentityChanged
            }
            ProcessInsightsProjectionRejection::NoActiveRequest
            | ProcessInsightsProjectionRejection::DuplicateFacet => {
                taskmanager_core::FailureKind::Rejected
            }
        };
        ProcessInsightDrainOutcome {
            projection: Some(self.process_insights_projection.apply_failure(
                &pending.target,
                pending.revision,
                pending.facet,
                ProcessInsightUnavailable::Provider(failure),
            )),
            diagnostic: Some(failure),
        }
    }
}

fn operation_failure(
    context: &PlatformEventContext,
    kind: taskmanager_core::FailureKind,
) -> OperationFailure {
    OperationFailure {
        request_id: context.request_id,
        capability: context.capability.clone(),
        sequence: context.sequence,
        kind,
        retry: ProviderFailure::from_kind(kind).retry(),
        provider: context.provider.clone(),
        observed_at_ms: context.observed_at_ms,
    }
}

fn process_insight_facet(capability: &CapabilityId) -> Option<ProcessInsightFacet> {
    if capability == &CapabilityId::PROCESS_INSIGHTS_NETWORK {
        Some(ProcessInsightFacet::Network)
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_GPU {
        Some(ProcessInsightFacet::Gpu)
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_RESOURCES {
        Some(ProcessInsightFacet::Resources)
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_ISOLATION {
        Some(ProcessInsightFacet::Isolation)
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_THREADS {
        Some(ProcessInsightFacet::Threads)
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_OPEN_FILES {
        Some(ProcessInsightFacet::OpenFiles)
    } else {
        None
    }
}
