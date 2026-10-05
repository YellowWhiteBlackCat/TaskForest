//! English copy for capture wiring; deterministic data belongs to shell.
use super::{SensorGroup, SystemHealthText};
use taskmanager_core::core::{
    DeviceStatus, FilesystemHealthStatus, SmartSelfTestFailure, SmartSelfTestKind,
    SmartSelfTestPhase,
};
pub fn capture_english_text(text: SystemHealthText) -> String {
    match text {
        SystemHealthText::StorageHealth => "Storage health",
        SystemHealthText::Filesystems => "Filesystems",
        SystemHealthText::Space => "Space",
        SystemHealthText::Free => "free",
        SystemHealthText::Inodes => "Inodes",
        SystemHealthText::ReadOnly => "Read-only",
        SystemHealthText::Errors => "Errors",
        SystemHealthText::Source => "Source",
        SystemHealthText::SmartSelfTest => "SMART self-test",
        SystemHealthText::ShortTest => "Short test…",
        SystemHealthText::ExtendedTest => "Extended test…",
        SystemHealthText::ConfirmationRequired => "Confirmation is required before a test starts.",
        SystemHealthText::SensorCenter => "Sensors",
        SystemHealthText::NoFilesystems => "No filesystem health data",
        SystemHealthText::NoReadings => "No readings",
        SystemHealthText::Unavailable => "Unavailable",
        SystemHealthText::Yes => "Yes",
        SystemHealthText::No => "No",
        SystemHealthText::Status => "Status",
        SystemHealthText::Progress => "Progress",
        SystemHealthText::LifetimeHours => "Lifetime hours",
        SystemHealthText::FirstErrorLba => "First error LBA",
        SystemHealthText::SensorGroup(SensorGroup::Temperature) => "Thermal zones",
        SystemHealthText::SensorGroup(SensorGroup::FanSpeed) => "Fans",
        SystemHealthText::SensorGroup(SensorGroup::Power) => "Power sensors",
        SystemHealthText::DeviceStatus(DeviceStatus::Healthy) => "Healthy",
        SystemHealthText::DeviceStatus(DeviceStatus::Stale) => "Stale",
        SystemHealthText::DeviceStatus(DeviceStatus::PermissionDenied) => "Permission denied",
        SystemHealthText::DeviceStatus(DeviceStatus::MissingTool) => "Required tool missing",
        SystemHealthText::DeviceStatus(DeviceStatus::Unsupported) => "Unsupported",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::Healthy) => "Healthy",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::ReadOnly) => "Read-only",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::ErrorsReported) => {
            "Errors reported"
        }
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Idle) => "Idle",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Running) => "Running",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Completed) => "Completed",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Aborted) => "Aborted",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Failed) => "Failed",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Unknown) => "Unknown",
        SystemHealthText::SmartKind(SmartSelfTestKind::Short) => "Short",
        SystemHealthText::SmartKind(SmartSelfTestKind::Extended) => "Extended",
        SystemHealthText::SmartKind(SmartSelfTestKind::Conveyance) => "Conveyance",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::InvalidDevice) => "Invalid device",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::MissingTool) => "Tool missing",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::RequiresEscalation) => {
            "Authorization required"
        }
        SystemHealthText::SmartFailure(SmartSelfTestFailure::PermissionDenied) => {
            "Permission denied"
        }
        SystemHealthText::SmartFailure(
            SmartSelfTestFailure::ProviderUnavailable | SmartSelfTestFailure::TimedOut,
        ) => "Provider unavailable",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::Rejected) => "Request rejected",
    }
    .into()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_system_health_view_capture_tests.rs"]
mod tests;
