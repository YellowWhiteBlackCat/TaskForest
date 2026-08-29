//! Typed capture preparation for the System > Health surface.

use crate::gpui_app::dashboard::{DashboardState, SystemSection};
use crate::gpui_app::root::TopPage;
use crate::gpui_app::system_health_view::SmartSelfTestConfirmationRequest;
use taskmanager_core::core::{
    DeviceGeneration, FilesystemHealthStatus, SensorQuantity, SmartSelfTestKind,
    SmartSelfTestObservation, SmartSelfTestReport, StorageDeviceKey,
};

use super::{CaptureEvidence, CaptureScenario, SystemHealthCaptureOutcome, emit_marker};

impl CaptureEvidence {
    /// Prepare strict Health-page evidence only after the normal telemetry and
    /// UI readiness gates. This installs typed fixtures and confirmation state;
    /// it has no controller and cannot queue or execute a SMART command.
    pub fn on_system_health_state(
        &mut self,
        page: &mut TopPage,
        dashboard: &mut DashboardState,
        snapshot: &mut taskmanager_core::core::metrics::SystemSnapshot,
        filesystems: &mut taskmanager_core::core::FilesystemHealthSnapshot,
        sensors: &mut taskmanager_core::core::SensorCenterSnapshot,
    ) -> SystemHealthCaptureOutcome {
        if !self.enabled
            || !self.telemetry_ready
            || !self.ui_data_ready
            || !self.system_health_fixture_requested()
        {
            return SystemHealthCaptureOutcome::default();
        }

        let fixture = crate::gpui_app::system_health_view::capture_fixture();
        *page = TopPage::System;
        dashboard.section = SystemSection::Health;
        snapshot.disks = vec![fixture.selected_disk.clone()];
        *filesystems = fixture.filesystems;
        *sensors = fixture.sensors;
        self.system_health_observation = Some(SmartSelfTestObservation {
            device_id: fixture.selected_disk.device_id.clone().into(),
            device_generation: fixture.selected_disk.device_generation,
            device_key: StorageDeviceKey::new(fixture.selected_disk.name.clone()),
            display_name: fixture.selected_disk.model.clone(),
            report: fixture.smart_report,
        });
        let confirmation = if self.scenario == Some(CaptureScenario::SmartSelfTestConfirm) {
            Some(SmartSelfTestConfirmationRequest {
                device_id: fixture.selected_disk.device_id.clone().into(),
                device_generation: fixture.selected_disk.device_generation,
                disk_name: fixture.selected_disk.name,
                disk_label: fixture.selected_disk.model,
                kind: SmartSelfTestKind::Short,
            })
        } else {
            None
        };

        let page_ready = *page == TopPage::System && dashboard.section == SystemSection::Health;
        let target_ready = match self.scenario {
            Some(CaptureScenario::StorageHealth) => {
                confirmation.is_none()
                    && filesystems
                        .filesystems
                        .iter()
                        .any(|filesystem| filesystem.status == FilesystemHealthStatus::Healthy)
                    && filesystems
                        .filesystems
                        .iter()
                        .any(|filesystem| filesystem.status == FilesystemHealthStatus::ReadOnly)
                    && filesystems.filesystems.iter().any(|filesystem| {
                        filesystem.status == FilesystemHealthStatus::ErrorsReported
                    })
            }
            Some(CaptureScenario::SmartSelfTestConfirm) => confirmation.is_some(),
            Some(CaptureScenario::SensorCenter) => {
                confirmation.is_none()
                    && [
                        SensorQuantity::Temperature,
                        SensorQuantity::FanSpeed,
                        SensorQuantity::Power,
                    ]
                    .into_iter()
                    .all(|quantity| {
                        sensors
                            .readings
                            .iter()
                            .any(|item| item.quantity() == &quantity)
                    })
                    && sensors
                        .readings
                        .iter()
                        .any(|reading| reading.current_measurement().is_none())
            }
            _ => false,
        };
        if page_ready && target_ready && !self.scenario_ready {
            self.scenario_ready = true;
            emit_marker("scenario_ready", self.scenario);
        }
        if !page_ready || !target_ready {
            return SystemHealthCaptureOutcome::NotReady;
        }
        match confirmation {
            Some(confirmation) => SystemHealthCaptureOutcome::ReadyWithConfirmation(confirmation),
            None => SystemHealthCaptureOutcome::Ready,
        }
    }

    pub(in crate::gpui_app::root) fn system_health_report_for(
        &self,
        device_id: &str,
        generation: DeviceGeneration,
    ) -> Option<&SmartSelfTestReport> {
        self.system_health_observation
            .as_ref()
            .filter(|observation| {
                observation.device_id.as_str() == device_id
                    && observation.device_generation == generation
            })
            .map(|observation| &observation.report)
    }
}
