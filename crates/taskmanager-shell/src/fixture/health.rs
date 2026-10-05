//! Deterministic fixtures and English-only copy for capture/headless wiring.

use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::metrics::{DiskMetrics, DiskScalarObservations, SmartAvailability};
use taskmanager_core::core::storage_health::FilesystemBackingKind;
use taskmanager_core::core::{
    DeviceGeneration, DeviceId, DeviceState, DeviceStatus, FailureKind, FilesystemHealth,
    FilesystemHealthSnapshot, FilesystemHealthStatus, SensorCenterSnapshot, SensorDescriptor,
    SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale, SmartSelfTestKind,
    SmartSelfTestPhase, SmartSelfTestReport,
};

#[derive(Clone, Debug)]
pub struct HealthFixture {
    pub filesystems: FilesystemHealthSnapshot,
    pub sensors: SensorCenterSnapshot,
    pub selected_disk: DiskMetrics,
    pub smart_report: SmartSelfTestReport,
}

pub fn health_fixture() -> HealthFixture {
    let now = 1_700_000_000_000;
    let filesystem =
        |mount: &str, source: &str, fs_type: &str, read_only, errors, status| FilesystemHealth {
            mount_point: mount.into(),
            source: Some(source.into()),
            fs_type: fs_type.into(),
            backing_kind: FilesystemBackingKind::Unknown,
            read_only,
            error_count: errors,
            inode_used: None,
            inode_total: None,
            inode_usage_percent: None,
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
    HealthFixture {
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
                capacity_bytes: ScalarObservation::available(2_000_000_000_000, now),
                available_bytes: ScalarObservation::available(625_000_000_000, now),
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

/// Install the same non-destructive health observations through owned fixture
/// facts and the normal revisioned storage/SMART fold. No native command runs.
pub fn seed_shell_health(app: &mut crate::ShellApp) {
    use crate::fixture::{ProjectionSeedFact, seed_projection_fact};
    use taskmanager_application::{
        CorrelatedEvent, PlatformEventBatch, PlatformEventContext, SmartEvent,
        SmartObservationBatch, SmartStateRevision, StorageHealthEvent,
    };
    use taskmanager_core::core::system_health::SmartSelfTestObservation;
    use taskmanager_platform_contract::{
        CapabilityId, CompositeSourceSnapshot, EventSequence, RequestId,
    };
    let fixture = health_fixture();
    if let Some(mut snapshot) = app.projection().snapshot.clone() {
        snapshot.disks = vec![fixture.selected_disk.clone()];
        seed_projection_fact(app, ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))));
    }
    seed_projection_fact(app, ProjectionSeedFact::Sensors(Some(fixture.sensors)));
    let Some(request_id) = RequestId::new(1) else {
        return;
    };
    let context = |capability| PlatformEventContext {
        request_id,
        capability,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_700_000_000_000,
    };
    let mut batch = PlatformEventBatch::default();
    batch.storage_health_events.push(CorrelatedEvent::new(
        context(CapabilityId::STORAGE_HEALTH),
        StorageHealthEvent::Snapshot(CompositeSourceSnapshot::new(
            fixture.filesystems,
            Vec::new(),
        )),
    ));
    let (current, _) = app.projection().smart_projection();
    let revision = current.revision().checked_next().unwrap_or_default();
    batch.smart_events.push(CorrelatedEvent::new(
        context(CapabilityId::SMART),
        SmartEvent::Batch(SmartObservationBatch {
            revision: SmartStateRevision::new(revision.get()),
            subject: None,
            observations: vec![SmartSelfTestObservation {
                device_id: fixture.selected_disk.device_id.into(),
                device_generation: fixture.selected_disk.device_generation,
                device_key: fixture.selected_disk.name.into(),
                display_name: fixture.selected_disk.model,
                report: fixture.smart_report,
            }],
            issues: Vec::new(),
            ended: Vec::new(),
        }),
    ));
    app.apply_platform_batch(batch);
}
