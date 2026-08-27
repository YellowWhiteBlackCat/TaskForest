//! Submission helper for the independently scheduled OpenFiles insight facet.

use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{ProcessInsightFacet, ProcessOpenFilesRequest};

use super::super::{PlatformClient, submit_request};

impl PlatformClient {
    pub(super) fn submit_process_open_files(
        &mut self,
        target: &crate::FrozenProcessIdentity,
        revision: crate::ProcessInsightsRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().process().open_files(),
            submitted_at_ms,
            ProcessOpenFilesRequest {
                target: target.clone(),
                revision,
            },
        );
        self.finish_process_insight_submission(
            id,
            target,
            revision,
            ProcessInsightFacet::OpenFiles,
            result,
        )
    }
}
