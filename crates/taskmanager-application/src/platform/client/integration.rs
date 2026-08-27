//! Integration-axis request submission on `PlatformClient`: command launch,
//! resource reveal, URL open, and desktop appearance.

use taskmanager_platform_contract::{RequestId, SubmissionError};

use crate::platform::{
    CommandLaunchRequest, DesktopAppearanceRequest, DesktopNotificationRequest,
    ResourceRevealRequest, SetupScriptRequest, UrlOpenRequest,
};

use super::{PlatformClient, submit_request};

impl PlatformClient {
    pub fn submit_command_launch(
        &mut self,
        request: CommandLaunchRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().command_launch(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_resource_reveal(
        &mut self,
        request: ResourceRevealRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().resource_reveal(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_url_open(
        &mut self,
        request: UrlOpenRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().url_open(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_desktop_appearance(
        &mut self,
        request: DesktopAppearanceRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().desktop_appearance(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_desktop_notification(
        &mut self,
        request: DesktopNotificationRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().desktop_notification(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    pub fn submit_setup_script(
        &mut self,
        request: SetupScriptRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().integration().setup_script(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
