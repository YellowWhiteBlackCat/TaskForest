//! Submission helper for the independently scheduled Environment insight
//! facet (working directory + bounded environment table).

use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{ProcessEnvironmentRequest, ProcessInsightFacet};

use super::super::{PlatformClient, submit_request};

impl PlatformClient {
    pub(super) fn submit_process_environment(
        &mut self,
        target: &crate::FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().environment(),
            submitted_at_ms,
            ProcessEnvironmentRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Environment,
            result,
        )
    }
}
