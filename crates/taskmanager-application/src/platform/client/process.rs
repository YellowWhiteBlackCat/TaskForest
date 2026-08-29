//! Process-axis request submission on `PlatformClient`: list, control,
//! five-facet insights under one revision, and affinity/resource control.

use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_platform_contract::{
    CapabilityId, RequestId, SubmissionError, SubmissionErrorKind,
};

use crate::platform::{
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessGpuRequest, ProcessInsightFacet, ProcessInsightUnavailable, ProcessInsightsSubmission,
    ProcessInsightsSubmissionError, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessResourceControlRequest,
    ProcessResourcesRequest,
};

use super::{PendingProcessInsightRequest, PlatformClient, submit_request};

mod environment;
mod open_files;
mod threads;

impl PlatformClient {
    pub fn submit_process_list(
        &mut self,
        request: ProcessListRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().list(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_process_control(
        &mut self,
        request: ProcessControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        if !control_request_has_exact_identity(&request) {
            return Err(invalid_process_request(CapabilityId::PROCESS_CONTROL));
        }
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    /// Schedule the five process-insight domains independently under one
    /// application-owned revision. Partial queue failures are returned and
    /// projected immediately; already accepted facets remain valid work.
    pub fn submit_process_insights(
        &mut self,
        target: FrozenProcessIdentity,
        submitted_at_ms: u64,
    ) -> Result<ProcessInsightsSubmission, ProcessInsightsSubmissionError> {
        if target.authoritative_start_token().is_none() {
            return Err(ProcessInsightsSubmissionError::IdentityUnavailable);
        }
        let revision = self.next_process_insights_revision()?;
        // Only one process-details selection is current. Retire correlations
        // for an older or hung selection before admitting the new revision so
        // late provider outcomes cannot make this map grow without bound.
        self.process_insight_requests.clear();
        self.process_insights_projection
            .begin(target.clone(), revision);
        let network = self.submit_process_network(&target, revision, submitted_at_ms);
        let gpu = self.submit_process_gpu(&target, revision, submitted_at_ms);
        let resources = self.submit_process_resources(&target, revision, submitted_at_ms);
        let isolation = self.submit_process_isolation(&target, revision, submitted_at_ms);
        let threads = self.submit_process_threads(&target, revision, submitted_at_ms);
        let open_files = self.submit_process_open_files(&target, revision, submitted_at_ms);
        let environment = self.submit_process_environment(&target, revision, submitted_at_ms);
        let projection = match self.process_insights_projection.snapshot() {
            Some(projection) => projection,
            None => crate::ProjectedProcessInsights::pending(target.clone(), revision),
        };
        Ok(ProcessInsightsSubmission {
            target,
            revision,
            network,
            gpu,
            resources,
            isolation,
            threads,
            open_files,
            environment,
            projection,
        })
    }

    fn submit_process_network(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().network(),
            submitted_at_ms,
            ProcessNetworkRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Network,
            result,
        )
    }

    fn submit_process_gpu(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().gpu(),
            submitted_at_ms,
            ProcessGpuRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Gpu,
            result,
        )
    }

    fn submit_process_resources(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().resources(),
            submitted_at_ms,
            ProcessResourcesRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Resources,
            result,
        )
    }

    fn submit_process_isolation(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().isolation(),
            submitted_at_ms,
            ProcessIsolationRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Isolation,
            result,
        )
    }

    pub fn submit_process_affinity(
        &mut self,
        request: ProcessAffinityRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        if request.target.authoritative_start_token().is_none() {
            return Err(invalid_process_request(CapabilityId::PROCESS_AFFINITY));
        }
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().affinity(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_process_affinity_control(
        &mut self,
        request: ProcessAffinityControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        if request.target.authoritative_start_token().is_none() {
            return Err(invalid_process_request(
                CapabilityId::PROCESS_AFFINITY_CONTROL,
            ));
        }
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().affinity_control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_process_resource_control(
        &mut self,
        request: ProcessResourceControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        if request.target.authoritative_start_token().is_none() {
            return Err(invalid_process_request(
                CapabilityId::PROCESS_RESOURCE_CONTROL,
            ));
        }
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().resource_control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    /// System-level (no target) per-feature escalation: offer the OS-native
    /// prompt for per-process byte accounting; the next network observation
    /// reflects the resulting capture state.
    pub fn submit_process_network_escalation(
        &mut self,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().process().network_escalation(),
            submitted_at_ms,
            ProcessNetworkEscalationRequest,
        )?;
        Ok(id)
    }
    fn finish_process_insight_submission(
        &mut self,
        id: RequestId,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        facet: ProcessInsightFacet,
        result: Result<(), SubmissionError>,
    ) -> Result<RequestId, SubmissionError> {
        match result {
            Ok(()) => {
                self.process_insight_requests.insert(
                    id,
                    PendingProcessInsightRequest {
                        target: target.clone(),
                        revision,
                        facet,
                    },
                );
                Ok(id)
            }
            Err(error) => {
                let _ = self.process_insights_projection.apply_failure(
                    target,
                    revision,
                    facet,
                    ProcessInsightUnavailable::Submission(error.kind),
                );
                Err(error)
            }
        }
    }

    fn next_process_insights_revision(
        &mut self,
    ) -> Result<crate::ProcessInsightsRevision, ProcessInsightsSubmissionError> {
        let next = self
            .process_insights_revision
            .checked_next()
            .ok_or(ProcessInsightsSubmissionError::RevisionExhausted)?;
        self.process_insights_revision = next;
        Ok(next)
    }
}

fn control_request_has_exact_identity(request: &ProcessControlRequest) -> bool {
    match request {
        ProcessControlRequest::EndTask(target)
        | ProcessControlRequest::SendSignal { target, .. }
        | ProcessControlRequest::Suspend { target }
        | ProcessControlRequest::Resume { target } => target.authoritative_start_token().is_some(),
        ProcessControlRequest::ExecuteBatch(intent) => {
            !intent.targets.is_empty()
                && intent
                    .targets
                    .iter()
                    .all(|target| target.authoritative_start_token().is_some())
        }
    }
}

fn invalid_process_request(capability: CapabilityId) -> SubmissionError {
    SubmissionError {
        capability,
        kind: SubmissionErrorKind::InvalidRequest,
    }
}
