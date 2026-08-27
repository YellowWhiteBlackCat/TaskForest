use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::SensorRequest;

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_sensor(
        &mut self,
        request: SensorRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().sensor().observation(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
