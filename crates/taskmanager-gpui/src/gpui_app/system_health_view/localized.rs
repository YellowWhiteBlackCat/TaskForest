//! Production localization resolver for the isolated system-health surface.

use crate::core::{
    DeviceStatus, FilesystemHealthStatus, SmartSelfTestFailure, SmartSelfTestKind,
    SmartSelfTestPhase,
};
use crate::i18n;

use super::{SensorGroup, SystemHealthText};

pub fn localized_text(text: SystemHealthText) -> String {
    let key = match text {
        SystemHealthText::StorageHealth => "health.storage",
        SystemHealthText::Filesystems => "health.filesystems",
        SystemHealthText::Space => "health.space",
        SystemHealthText::Free => "health.free",
        SystemHealthText::Inodes => "health.inodes",
        SystemHealthText::ReadOnly => "health.read_only",
        SystemHealthText::Errors => "health.errors",
        SystemHealthText::Source => "health.source",
        SystemHealthText::SmartSelfTest => "health.smart_self_test",
        SystemHealthText::ShortTest => "health.short_test",
        SystemHealthText::ExtendedTest => "health.extended_test",
        SystemHealthText::ConfirmationRequired => "health.confirmation_required",
        SystemHealthText::SensorCenter => "health.sensors",
        SystemHealthText::NoFilesystems => "health.no_filesystems",
        SystemHealthText::NoReadings => "health.no_readings",
        SystemHealthText::Unavailable => "health.unavailable",
        SystemHealthText::Yes => "common.yes",
        SystemHealthText::No => "common.no",
        SystemHealthText::Status => "common.status",
        SystemHealthText::Progress => "health.progress",
        SystemHealthText::LifetimeHours => "health.lifetime_hours",
        SystemHealthText::FirstErrorLba => "health.first_error_lba",
        SystemHealthText::SensorGroup(SensorGroup::Temperature) => "common.temperature",
        SystemHealthText::SensorGroup(SensorGroup::FanSpeed) => "health.fans",
        SystemHealthText::SensorGroup(SensorGroup::Power) => "common.power",
        SystemHealthText::DeviceStatus(DeviceStatus::Healthy) => "device.healthy",
        SystemHealthText::DeviceStatus(DeviceStatus::Stale) => "device.stale",
        SystemHealthText::DeviceStatus(DeviceStatus::PermissionDenied) => {
            "device.permission_denied"
        }
        SystemHealthText::DeviceStatus(DeviceStatus::MissingTool) => "device.missing_tool",
        SystemHealthText::DeviceStatus(DeviceStatus::Unsupported) => "device.unsupported",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::Healthy) => "device.healthy",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::ReadOnly) => "health.read_only",
        SystemHealthText::FilesystemStatus(FilesystemHealthStatus::ErrorsReported) => {
            "health.errors_reported"
        }
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Idle) => "health.phase_idle",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Running) => "health.phase_running",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Completed) => "health.phase_completed",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Aborted) => "health.phase_aborted",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Failed) => "health.phase_failed",
        SystemHealthText::SmartPhase(SmartSelfTestPhase::Unknown) => "health.phase_unknown",
        SystemHealthText::SmartKind(SmartSelfTestKind::Short) => "health.kind_short",
        SystemHealthText::SmartKind(SmartSelfTestKind::Extended) => "health.kind_extended",
        SystemHealthText::SmartKind(SmartSelfTestKind::Conveyance) => "health.kind_conveyance",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::InvalidDevice) => {
            "health.failure_invalid_device"
        }
        SystemHealthText::SmartFailure(SmartSelfTestFailure::MissingTool) => "device.missing_tool",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::RequiresEscalation) => {
            "device.requires_escalation"
        }
        SystemHealthText::SmartFailure(SmartSelfTestFailure::PermissionDenied) => {
            "device.permission_denied"
        }
        SystemHealthText::SmartFailure(
            SmartSelfTestFailure::ProviderUnavailable | SmartSelfTestFailure::TimedOut,
        ) => "health.failure_provider_unavailable",
        SystemHealthText::SmartFailure(SmartSelfTestFailure::Rejected) => "health.failure_rejected",
    };
    i18n::t(key).to_string()
}
