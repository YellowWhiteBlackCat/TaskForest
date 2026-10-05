//! Read-only service capture data uses the actual correlated details state.
use crate::gpui_app::root::RootView;
use taskmanager_application::ServiceUpdate;
use taskmanager_core::core::services::{ServiceLogSnapshot, ServiceLogState};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;

pub(crate) fn seed(view: &mut RootView, target: &ServiceId) {
    if view
        .service_details
        .accept_log_snapshot(target, RequestId::MIN)
    {
        view.service_details.apply(ServiceUpdate::Logs {
            request_id: RequestId::MIN,
            snapshot: ServiceLogSnapshot {
                service_id: target.clone(),
                state: ServiceLogState::from_lines(vec![
                    "Telemetry service started".into(),
                    "Collectors ready: cpu memory disk network gpu".into(),
                    "Health check passed".into(),
                    "Waiting for next refresh".into(),
                ]),
            },
        });
    }
}
