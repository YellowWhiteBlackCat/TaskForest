//! Environment-axis request submission on `PlatformClient`: startup evidence
//! and startup/session inventory and control.

use taskmanager_platform_contract::{
    CapabilityId, RequestId, SubmissionError, SubmissionErrorKind,
};

use crate::platform::{
    SessionInventoryRequest, StartupEvidenceRequest, StartupEvidenceUnavailable,
    StartupInventoryRequest,
};

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_startup_inventory(
        &mut self,
        request: StartupInventoryRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().environment().startup_inventory(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_startup_evidence(
        &mut self,
        request: StartupEvidenceRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let Some(revision) = self.startup_evidence_revision.checked_next() else {
            return Err(SubmissionError {
                capability: CapabilityId::STARTUP_EVIDENCE,
                kind: SubmissionErrorKind::InvalidRequest,
            });
        };
        self.startup_evidence_revision = revision;
        self.startup_evidence_requests.clear();
        self.startup_evidence_projection.begin(revision);
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().environment().startup_evidence(),
            submitted_at_ms,
            request,
        );
        match result {
            Ok(()) => {
                self.startup_evidence_requests.insert(id, revision);
                Ok(id)
            }
            Err(error) => {
                let _ = self.startup_evidence_projection.apply_failure(
                    revision,
                    StartupEvidenceUnavailable::Submission(error.kind),
                    submitted_at_ms,
                );
                Err(error)
            }
        }
    }

    pub fn submit_startup_control(
        &mut self,
        request: crate::StartupControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().environment().startup_control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_session_inventory(
        &mut self,
        request: SessionInventoryRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().environment().session_inventory(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_session_control(
        &mut self,
        request: crate::SessionControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().environment().session_control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
