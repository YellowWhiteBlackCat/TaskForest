//! Submission helper for the independently scheduled Threads insight facet.

use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{ProcessInsightFacet, ProcessThreadsRequest};

use super::super::{PlatformClient, submit_request};

impl PlatformClient {
    pub(super) fn submit_process_threads(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().threads(),
            submitted_at_ms,
            ProcessThreadsRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::Threads,
            result,
        )
    }
}
