//! Storage-axis request submission on `PlatformClient`: filesystem health,
//! SMART observation/control, and directory-usage scan lifecycle.

use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{
    DirectoryUsageRequest, SmartControlRequest, SmartObservationRequest, StorageHealthRequest,
};

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_storage_health(
        &mut self,
        request: StorageHealthRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().storage().health(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_smart_observation(
        &mut self,
        request: SmartObservationRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().storage().smart_observation(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_smart_control(
        &mut self,
        request: SmartControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().storage().smart_control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    /// Submit a directory-usage scan lifecycle request (start/resume/cancel).
    /// The scan runs on its own bounded lane; progress arrives as
    /// `DirectoryUsageEvent` publications in the next event batch.
    pub fn submit_directory_usage(
        &mut self,
        request: DirectoryUsageRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().storage().directory_usage(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
