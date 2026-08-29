//! Deterministic fixtures and English-only copy for capture/headless wiring.

use taskmanager_core::core::metrics::{DiskMetrics, DiskScalarObservations, SmartAvailability};
use taskmanager_core::core::{
    DeviceGeneration, DeviceId, DeviceState, DeviceStatus, FailureKind, FilesystemHealth,
    FilesystemHealthSnapshot, FilesystemHealthStatus, SensorCenterSnapshot, SensorDescriptor,
    SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
    SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
};

use super::{SensorGroup, SystemHealthText};

#[derive(Clone, Debug)]
pub struct SystemHealthCaptureFixture {
    pub filesystems: FilesystemHealthSnapshot,
    pub sensors: SensorCenterSnapshot,
    pub selected_disk: DiskMetrics,
    pub smart_report: SmartSelfTestReport,
}

pub fn capture_fixture() -> SystemHealthCaptureFixture {
    let now = 1_700_000_000_000;
    let filesystem =
        |mount: &str, source: &str, fs_type: &str, read_only, errors, status| FilesystemHealth {
            mount_point: mount.into(),
            source: Some(source.into()),
            fs_type: fs_type.into(),
            read_only,
            error_count: errors,
            status,
            state: DeviceState::healthy(now),
            integrity_state: errors
                .map(|_| DeviceState::healthy(now))
                .unwrap_or_default(),
        };
    let sensor = |id: &str,
                  label: &str,
                  descriptor: SensorDescriptor,
                  magnitude: Option<SensorMagnitude>,
                  state: DeviceState| {
        let observation = magnitude.map_or_else(
            || {
                SensorMeasurementObservation::unavailable(
                    descriptor.clone(),
                    state.status.failure().unwrap_or(FailureKind::ProviderFault),
                )
            },
            |magnitude| {
                SensorMeasurementObservation::available(descriptor.clone(), magnitude, now)
                    .unwrap_or_else(|_| {
                        SensorMeasurementObservation::unavailable(
                            descriptor.clone(),
                            FailureKind::ProviderFault,
                        )
                    })
            },
        );
        SensorReading::from_measurement_observation(
            DeviceId::new(format!("capture:{id}")),
            id.into(),
            label.into(),
            observation,
        )
        .with_device_generation(DeviceGeneration::new(1))
    };
    SystemHealthCaptureFixture {
        filesystems: FilesystemHealthSnapshot {
            state: DeviceState::healthy(now),
            filesystems: vec![
                filesystem(
                    "/",
                    "/dev/nvme0n1p2",
                    "ext4",
                    Some(false),
                    Some(0),
                    FilesystemHealthStatus::Healthy,
                ),
                filesystem(
                    "/srv/archive",
                    "/dev/sdb1",
                    "xfs",
                    Some(true),
                    None,
                    FilesystemHealthStatus::ReadOnly,
                ),
                filesystem(
                    "/media/backup",
                    "/dev/sdc1",
                    "ext4",
                    Some(false),
                    Some(3),
                    FilesystemHealthStatus::ErrorsReported,
                ),
            ],
        },
        sensors: SensorCenterSnapshot {
            state: DeviceState::healthy(now),
            timestamp_ms: now,
            readings: vec![
                sensor(
                    "temp:package",
                    "CPU package",
                    SensorDescriptor::temperature(SensorScale::IDENTITY),
                    Some(SensorMagnitude::Decimal(67.5)),
                    DeviceState::healthy(now),
                ),
                sensor(
                    "fan:cpu",
                    "CPU fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    Some(SensorMagnitude::Unsigned(1_380)),
                    DeviceState::healthy(now),
                ),
                sensor(
                    "fan:chassis",
                    "Chassis fan",
                    SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                    None,
                    DeviceState::default().transition(DeviceStatus::PermissionDenied, now),
                ),
                sensor(
                    "power:package",
                    "Package power",
                    SensorDescriptor::power(SensorScale::IDENTITY),
                    Some(SensorMagnitude::Decimal(42.75)),
                    DeviceState::healthy(now),
                ),
            ],
            thermal_control: Default::default(),
            device_lifecycles: Default::default(),
        },
        selected_disk: {
            let mut disk = DiskMetrics::new("nvme0n1");
            disk.device_id = "disk:wwid:capture-nvme".into();
            disk.device_generation = DeviceGeneration::INITIAL;
            disk.device_state = DeviceState::healthy(now);
            disk.disk_type = "NVMe SSD".into();
            disk.model = "Capture NVMe 2 TB".into();
            disk.mount_point = "/".into();
            disk.fs_type = "ext4".into();
            disk.smart_availability = SmartAvailability::Available;
            disk.smart_state = DeviceState::healthy(now);
            disk.smart_temperature_c = Some(39.0);
            disk.apply_scalar_observations(DiskScalarObservations {
                capacity_bytes: taskmanager_core::core::ScalarObservation::available(
                    2_000_000_000_000,
                    now,
                ),
                available_bytes: taskmanager_core::core::ScalarObservation::available(
                    625_000_000_000,
                    now,
                ),
                ..Default::default()
            });
            disk
        },
        smart_report: SmartSelfTestReport {
            state: DeviceState::healthy(now),
            phase: SmartSelfTestPhase::Completed,
            kind: Some(SmartSelfTestKind::Extended),
            progress_pct: Some(100.0),
            lifetime_hours: Some(12_876),
            first_error_lba: None,
            failure: None,
        },
    }
}

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
        SystemHealthText::SensorGroup(SensorGroup::Temperature) => "Temperature",
        SystemHealthText::SensorGroup(SensorGroup::FanSpeed) => "Fans",
        SystemHealthText::SensorGroup(SensorGroup::Power) => "Power",
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
