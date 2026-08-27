//! Service-axis request submission on `PlatformClient`: inventory,
//! dependencies, control, and log snapshot/stream.

use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{
    ServiceControlRequest, ServiceDependenciesRequest, ServiceInventoryRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest,
};

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_service_inventory(
        &mut self,
        request: ServiceInventoryRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().service().inventory(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_service_dependencies(
        &mut self,
        request: ServiceDependenciesRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().service().dependencies(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_service_control(
        &mut self,
        request: ServiceControlRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().service().control(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_service_log_snapshot(
        &mut self,
        request: ServiceLogSnapshotRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().service().log_snapshot(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_service_log_stream(
        &mut self,
        request: ServiceLogStreamRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().service().log_stream(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
