use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::PowerSupplyRequest;

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_power_supply(
        &mut self,
        request: PowerSupplyRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().power().supplies(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
