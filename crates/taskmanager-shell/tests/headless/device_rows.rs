//! test-intent: behavior
//! CORE-13 behavior coverage for family-qualified device rows and anchors.

use super::*;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::app::ShellApp;
use taskmanager_application::{
    CorrelatedEvent, CorrelatedNpuInventoryEvent, CorrelatedSystemTelemetryOutcome,
    DeviceLifecycleApplyResult, DeviceLifecyclePartition, DeviceLifecycleProjectionIssue,
    DeviceLifecycleSnapshotRevision, NpuInventoryEvent, PlatformEventBatch, PlatformEventContext,
    ProjectedSystemTelemetry, SystemTelemetryDomainEvent, SystemTelemetryDomainOutcome,
    SystemTelemetryDomainState, SystemTelemetryProjection, SystemTelemetryProjectionApplyResult,
    SystemTelemetryRevision,
};
use taskmanager_core::core::device_state::{
    DeviceLifecycle, DevicePresence, DeviceState, DeviceStatus,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuTelemetryObservation, DiskMetrics, DiskPartition, GpuMetrics,
    GpuTelemetryObservation, HostRuntimeFacts, HostRuntimeObservation, MemoryMetrics,
    MemoryTelemetryObservation, NetworkMetrics, NetworkTelemetryObservation, ScalarObservation,
    StorageTelemetryObservation, SystemSnapshot,
};
use taskmanager_core::core::npu::{NpuDevice, NpuInventorySnapshot};
use taskmanager_core::core::power::{BatteryInfo, PowerSupplySnapshot};
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorReading};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

fn lifecycle(now_ms: u64) -> DeviceLifecycle {
    DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(now_ms),
        generation: DeviceGeneration::INITIAL,
        first_seen_ms: Some(now_ms),
        last_seen_ms: Some(now_ms),
        absent_since_ms: None,
    }
}

fn lifecycle_with_generation(now_ms: u64, generation: DeviceGeneration) -> DeviceLifecycle {
    DeviceLifecycle {
        generation,
        ..lifecycle(now_ms)
    }
}

fn disk_metrics(id: &str, generation: DeviceGeneration) -> DiskMetrics {
    let mut disk = DiskMetrics::new("");
    disk.device_id = id.to_owned();
    disk.device_generation = generation;
    disk
}

fn network_metrics(id: &str, generation: DeviceGeneration) -> NetworkMetrics {
    let mut network = NetworkMetrics::new("");
    network.device_id = Arc::from(id);
    network.device_generation = generation;
    network
}

fn gpu_metrics(id: &str, generation: DeviceGeneration) -> GpuMetrics {
    let mut gpu = GpuMetrics::new(id, "");
    gpu.device_generation = generation;
    gpu
}

fn battery_info(id: &str, generation: DeviceGeneration) -> BatteryInfo {
    let mut battery = BatteryInfo::default();
    battery.id = id.to_owned();
    battery.device_generation = generation;
    battery
}

fn sidecar(id: &str) -> BTreeMap<DeviceId, DeviceLifecycle> {
    BTreeMap::from([(DeviceId::new(id), lifecycle(10))])
}

fn admit_storage(store: &mut SystemProjectionStore, id: &str, revision: u64) {
    let observation =
        StorageTelemetryObservation::current(Vec::new(), 10, Vec::new(), Vec::new(), sidecar(id));
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_storage_telemetry_observation(
                DeviceLifecycleSnapshotRevision::new(revision),
                &observation
            ),
        DeviceLifecycleApplyResult::Applied(_)
    ));
}

fn admit_network(store: &mut SystemProjectionStore, id: &str, revision: u64) {
    let observation =
        NetworkTelemetryObservation::current(Vec::new(), 10, Vec::new(), Vec::new(), sidecar(id));
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_network_telemetry_observation(
                DeviceLifecycleSnapshotRevision::new(revision),
                &observation
            ),
        DeviceLifecycleApplyResult::Applied(_)
    ));
}

fn admit_gpu(store: &mut SystemProjectionStore, id: &str, revision: u64) {
    let observation =
        GpuTelemetryObservation::current(Vec::new(), 10, Vec::new(), Vec::new(), sidecar(id));
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_gpu_telemetry_observation(
                DeviceLifecycleSnapshotRevision::new(revision),
                &observation
            ),
        DeviceLifecycleApplyResult::Applied(_)
    ));
}

fn store_with_two_disks() -> SystemProjectionStore {
    let mut store = SystemProjectionStore::default();
    admit_storage(&mut store, "disk:a", 1);
    admit_storage(&mut store, "disk:b", 2);
    store.snapshot = Some(SystemSnapshot {
        disks: vec![
            disk_metrics("disk:a", DeviceGeneration::INITIAL),
            disk_metrics("disk:b", DeviceGeneration::INITIAL),
        ],
        ..SystemSnapshot::default()
    });
    store.note_device_row_family_changed(DeviceRowFamily::Storage);
    store
}

fn npu_inventory_event(
    sequence: u64,
    snapshot: NpuInventorySnapshot,
) -> CorrelatedNpuInventoryEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request ID"),
            capability: CapabilityId::ACCELERATOR_NPU,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: snapshot.observed_at_ms,
        },
        NpuInventoryEvent::Update(snapshot),
    )
}

fn fold_npu(app: &mut ShellApp, sequence: u64, snapshot: NpuInventorySnapshot) {
    let mut batch = PlatformEventBatch::default();
    batch
        .npu_inventory_events
        .push(npu_inventory_event(sequence, snapshot));
    app.apply_platform_batch(batch);
}

fn telemetry_source(provider: &'static str) -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome: SourceOutcome::Available,
        item_count: 1,
    }]
}

fn storage_observation(device_ids: &[&str], observed_at_ms: u64) -> StorageTelemetryObservation {
    storage_observation_with_generation(device_ids, observed_at_ms, DeviceGeneration::INITIAL)
}

fn storage_observation_with_generation(
    device_ids: &[&str],
    observed_at_ms: u64,
    generation: DeviceGeneration,
) -> StorageTelemetryObservation {
    StorageTelemetryObservation::current(
        device_ids
            .iter()
            .map(|id| disk_metrics(id, generation))
            .collect(),
        observed_at_ms,
        telemetry_source("fixture.storage"),
        Vec::new(),
        device_ids
            .iter()
            .map(|id| {
                (
                    DeviceId::new(*id),
                    lifecycle_with_generation(observed_at_ms, generation),
                )
            })
            .collect(),
    )
}

fn full_telemetry_projection(
    revision: SystemTelemetryRevision,
    storage: &StorageTelemetryObservation,
) -> ProjectedSystemTelemetry {
    let host = SystemTelemetryDomainEvent::Host {
        revision,
        observation: Box::new(HostRuntimeObservation::current(
            HostRuntimeFacts {
                uptime_secs: ScalarObservation::available(100, 10),
                processes: ScalarObservation::available(5, 10),
                threads: ScalarObservation::available(8, 10),
                pressure: ScalarObservation::default(),
                load_average: ScalarObservation::default(),
            },
            10,
            telemetry_source("fixture.host"),
        )),
    };
    let cpu = SystemTelemetryDomainEvent::Cpu {
        revision,
        observation: Box::new(CpuTelemetryObservation::current(
            CpuMetrics::default(),
            11,
            telemetry_source("fixture.cpu"),
        )),
    };
    let memory = SystemTelemetryDomainEvent::Memory {
        revision,
        observation: Box::new(MemoryTelemetryObservation::current(
            MemoryMetrics::default(),
            12,
            telemetry_source("fixture.memory"),
        )),
    };
    let storage = SystemTelemetryDomainEvent::Storage {
        revision,
        observation: Box::new(storage.clone()),
    };
    let network = SystemTelemetryDomainEvent::Network {
        revision,
        observation: Box::new(NetworkTelemetryObservation::current(
            Vec::new(),
            14,
            telemetry_source("fixture.network"),
            Vec::new(),
            BTreeMap::new(),
        )),
    };
    let gpu = SystemTelemetryDomainEvent::Gpu {
        revision,
        observation: Box::new(GpuTelemetryObservation::current(
            Vec::new(),
            15,
            telemetry_source("fixture.gpu"),
            Vec::new(),
            BTreeMap::new(),
        )),
    };

    let mut reducer = SystemTelemetryProjection::default();
    reducer.begin(revision);
    for event in [host, cpu, memory, storage, network, gpu] {
        assert!(matches!(
            reducer.apply(&event),
            SystemTelemetryProjectionApplyResult::AppliedPartial(_)
                | SystemTelemetryProjectionApplyResult::AppliedTerminal { .. }
        ));
    }
    reducer.snapshot().expect("complete telemetry projection")
}

fn system_storage_batch(sequence: u64, revision: u64, device_ids: &[&str]) -> PlatformEventBatch {
    let observation = storage_observation(device_ids, sequence.saturating_mul(10));
    system_storage_batch_with_observation(sequence, revision, observation, true)
}

fn storage_lifecycle_outcome(
    sequence: u64,
    revision: u64,
    observation: StorageTelemetryObservation,
) -> CorrelatedSystemTelemetryOutcome {
    let event = SystemTelemetryDomainEvent::Storage {
        revision: SystemTelemetryRevision::new(revision),
        observation: Box::new(observation),
    };
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request ID"),
            capability: CapabilityId::TELEMETRY_STORAGE,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: sequence.saturating_mul(10),
        },
        SystemTelemetryDomainOutcome::Observed(event),
    )
}

fn system_storage_batch_with_observation(
    sequence: u64,
    revision: u64,
    observation: StorageTelemetryObservation,
    complete: bool,
) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch
        .system_telemetry_outcomes
        .push(storage_lifecycle_outcome(
            sequence,
            revision,
            observation.clone(),
        ));
    let mut projection =
        full_telemetry_projection(SystemTelemetryRevision::new(revision), &observation);
    if !complete {
        projection.gpu = SystemTelemetryDomainState::Pending;
    }
    batch.system_telemetry_projections.push(projection);
    batch
}

#[test]
fn owner_ids_include_family_and_incarnation_and_reject_incomplete_facts() {
    let device_id = DeviceId::new("opaque:shared");
    let generation = DeviceGeneration::INITIAL;
    let storage = DeviceRowId::StorageDisk {
        device_id: device_id.clone(),
        generation,
    };
    let network = DeviceRowId::Network {
        device_id: device_id.clone(),
        generation,
    };
    assert_ne!(storage, network, "family is part of identity");
    assert_ne!(
        network,
        DeviceRowId::Network {
            device_id: device_id.clone(),
            generation: generation.next(),
        },
        "a reappeared device has a new row identity"
    );

    assert_eq!(
        DeviceRowId::from_disk(&DiskMetrics::default()),
        None,
        "a legacy disk with no owner ID or generation has no stable row identity"
    );
    assert_eq!(
        DeviceRowId::from_network(&NetworkMetrics::new("legacy")),
        None,
        "a network row without an admitted generation is display-only"
    );
    assert_eq!(
        DeviceRowId::from_npu(&NpuDevice {
            device_id,
            ..NpuDevice::default()
        }),
        None,
        "an NPU generation is supplied by the lifecycle projection"
    );
    let missing_channel = SensorReading::from_measurement_observation(
        DeviceId::new("sensor:0"),
        String::new(),
        "unnamed channel".into(),
        Default::default(),
    )
    .with_device_generation(DeviceGeneration::INITIAL);
    assert_eq!(
        DeviceRowId::from_sensor(&missing_channel),
        None,
        "a sensor channel ID is required in addition to its physical device ID"
    );
}

#[test]
fn each_owner_family_projects_rows_with_physical_incarnation_identity() {
    let mut store = SystemProjectionStore::default();
    for (family, id) in [
        (DeviceRowFamily::Storage, "disk:0"),
        (DeviceRowFamily::Network, "net:0"),
        (DeviceRowFamily::Gpu, "gpu:0"),
    ] {
        match family {
            DeviceRowFamily::Storage => admit_storage(&mut store, id, 1),
            DeviceRowFamily::Network => admit_network(&mut store, id, 1),
            DeviceRowFamily::Gpu => admit_gpu(&mut store, id, 1),
            _ => unreachable!("only system telemetry families are listed"),
        }
    }
    store.snapshot = Some(SystemSnapshot {
        disks: vec![{
            let mut disk = disk_metrics("disk:0", DeviceGeneration::INITIAL);
            let mut partition = DiskPartition::new("p1");
            partition.device_id = "partition:disk:0:p1".into();
            partition.parent_device_id = "disk:0".into();
            partition.device_generation = DeviceGeneration::INITIAL;
            disk.partitions.push(partition);
            disk
        }],
        networks: vec![network_metrics("net:0", DeviceGeneration::INITIAL)],
        gpu: vec![gpu_metrics("gpu:0", DeviceGeneration::INITIAL)],
        ..SystemSnapshot::default()
    });

    let power = PowerSupplySnapshot {
        batteries: vec![battery_info("battery:0", DeviceGeneration::INITIAL)],
        device_lifecycles: HashMap::from([("battery:0".into(), lifecycle(10))]),
        ..PowerSupplySnapshot::default()
    };
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_power_supply_snapshot(DeviceLifecycleSnapshotRevision::new(1), &power),
        DeviceLifecycleApplyResult::Applied(_)
    ));
    store.power_supplies = Some(power.clone());

    let sensor = SensorReading::from_measurement_observation(
        DeviceId::new("sensor:0"),
        "temp1".into(),
        "CPU temperature".into(),
        Default::default(),
    )
    .with_device_generation(DeviceGeneration::INITIAL);
    let sensors = SensorCenterSnapshot {
        readings: vec![sensor],
        device_lifecycles: HashMap::from([("sensor:0".into(), lifecycle(10))]),
        ..SensorCenterSnapshot::default()
    };
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(1), &sensors),
        DeviceLifecycleApplyResult::Applied(_)
    ));
    store.sensors = Some(sensors);

    let mut npu = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("npu:0"),
            ..NpuDevice::default()
        }],
        10,
    );
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut npu),
        DeviceLifecycleApplyResult::Applied(_)
    ));
    store.npu_inventory = Some(npu);

    let storage_ids = store.device_row_ids(DeviceRowFamily::Storage);
    assert_eq!(
        storage_ids.len(),
        2,
        "disk and filesystem child are separate rows"
    );
    assert!(matches!(
        &storage_ids[1],
        DeviceRowId::StoragePartition {
            parent_device_id,
            generation,
            partition_id,
        } if parent_device_id.as_str() == "disk:0"
            && *generation == DeviceGeneration::INITIAL
            && partition_id.as_str() == "partition:disk:0:p1"
    ));
    assert_eq!(store.device_row_ids(DeviceRowFamily::Network).len(), 1);
    assert_eq!(store.device_row_ids(DeviceRowFamily::Gpu).len(), 1);
    assert_eq!(store.device_row_ids(DeviceRowFamily::PowerSupply).len(), 1);
    assert_eq!(
        store.device_row_ids(DeviceRowFamily::Sensor),
        [DeviceRowId::SensorChannel {
            device_id: DeviceId::new("sensor:0"),
            generation: DeviceGeneration::INITIAL,
            channel_id: "temp1".into(),
        }]
    );
    assert_eq!(store.device_row_ids(DeviceRowFamily::Npu).len(), 1);
}

#[test]
fn storage_partition_identity_must_match_its_enclosing_disk_incarnation() {
    let mut store = SystemProjectionStore::default();
    let observation = StorageTelemetryObservation::current(
        Vec::new(),
        10,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([
            (DeviceId::new("disk:a"), lifecycle(10)),
            (DeviceId::new("disk:b"), lifecycle(10)),
        ]),
    );
    assert!(matches!(
        store
            .device_lifecycle_projection
            .apply_storage_telemetry_observation(
                DeviceLifecycleSnapshotRevision::new(1),
                &observation,
            ),
        DeviceLifecycleApplyResult::Applied(_)
    ));

    let mut outer_disk = disk_metrics("disk:a", DeviceGeneration::INITIAL);
    let mut wrong_parent = DiskPartition::new("misattached");
    wrong_parent.device_id = "partition:disk:b:misattached".into();
    wrong_parent.parent_device_id = "disk:b".into();
    wrong_parent.device_generation = DeviceGeneration::INITIAL;
    let mut wrong_generation = DiskPartition::new("old-incarnation");
    wrong_generation.device_id = "partition:disk:a:old-incarnation".into();
    wrong_generation.parent_device_id = "disk:a".into();
    wrong_generation.device_generation = DeviceGeneration::new(2);
    outer_disk
        .partitions
        .extend([wrong_parent, wrong_generation]);
    store.snapshot = Some(SystemSnapshot {
        disks: vec![
            outer_disk,
            disk_metrics("disk:b", DeviceGeneration::INITIAL),
        ],
        ..SystemSnapshot::default()
    });

    assert_eq!(
        store.device_row_ids(DeviceRowFamily::Storage),
        [
            DeviceRowId::StorageDisk {
                device_id: DeviceId::new("disk:a"),
                generation: DeviceGeneration::INITIAL,
            },
            DeviceRowId::StorageDisk {
                device_id: DeviceId::new("disk:b"),
                generation: DeviceGeneration::INITIAL,
            },
        ]
    );
}

#[test]
fn sensor_and_power_fixture_seeds_apply_their_lifecycle_owner_projection() {
    let mut app = ShellApp::new();
    let sensor = SensorReading::from_measurement_observation(
        DeviceId::new("fixture:sensor"),
        "temperature".into(),
        "Temperature".into(),
        Default::default(),
    )
    .with_device_generation(DeviceGeneration::INITIAL);
    crate::fixture::seed_projection_fact(
        &mut app,
        crate::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            readings: vec![sensor],
            device_lifecycles: HashMap::from([("fixture:sensor".into(), lifecycle(10))]),
            ..SensorCenterSnapshot::default()
        })),
    );

    crate::fixture::seed_projection_fact(
        &mut app,
        crate::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            batteries: vec![battery_info("fixture:battery", DeviceGeneration::INITIAL)],
            device_lifecycles: HashMap::from([("fixture:battery".into(), lifecycle(10))]),
            ..PowerSupplySnapshot::default()
        })),
    );

    assert_eq!(
        app.projection().device_row_ids(DeviceRowFamily::Sensor),
        [DeviceRowId::SensorChannel {
            device_id: DeviceId::new("fixture:sensor"),
            generation: DeviceGeneration::INITIAL,
            channel_id: "temperature".into(),
        }]
    );
    assert_eq!(
        app.projection()
            .device_row_ids(DeviceRowFamily::PowerSupply),
        [DeviceRowId::PowerSupply {
            device_id: DeviceId::new("fixture:battery"),
            generation: DeviceGeneration::INITIAL,
        }]
    );
    assert_eq!(
        app.projection()
            .device_lifecycle_projection
            .accepted_revision_for(DeviceLifecyclePartition::Sensors),
        Some(DeviceLifecycleSnapshotRevision::new(1))
    );
    assert_eq!(
        app.projection()
            .device_lifecycle_projection
            .accepted_revision_for(DeviceLifecyclePartition::PowerSupplies),
        Some(DeviceLifecycleSnapshotRevision::new(1))
    );
}

#[test]
fn gpu_rows_fail_closed_without_matching_lifecycle_identity() {
    let mut store = SystemProjectionStore::default();
    store.snapshot = Some(SystemSnapshot {
        gpu: vec![gpu_metrics(
            "macos:gpu:system-profiler:board",
            DeviceGeneration::INITIAL,
        )],
        ..SystemSnapshot::default()
    });
    assert!(
        store.device_row_ids(DeviceRowFamily::Gpu).is_empty(),
        "a metric without an accepted lifecycle sidecar has no stable row identity"
    );

    admit_gpu(&mut store, "macos:gpu:system-profiler:board", 1);
    assert_eq!(store.device_row_ids(DeviceRowFamily::Gpu).len(), 1);
    store.snapshot.as_mut().expect("GPU snapshot").gpu[0].device_generation =
        DeviceGeneration::INITIAL.next();
    assert!(
        store.device_row_ids(DeviceRowFamily::Gpu).is_empty(),
        "a row generation that differs from the lifecycle owner must fail closed"
    );
}

#[test]
fn npu_shell_rows_use_incarnations_and_ignore_duplicate_or_failed_inventory() {
    let mut app = ShellApp::new();
    let npu = || NpuDevice {
        device_id: DeviceId::new("npu:0"),
        ..NpuDevice::default()
    };
    fold_npu(
        &mut app,
        1,
        NpuInventorySnapshot::discovered(vec![npu()], 10),
    );
    let first_id = app
        .projection()
        .device_row_identity_at(DeviceRowFamily::Npu, 0)
        .expect("accepted NPU has a selectable shell row");
    assert!(matches!(
        &first_id,
        DeviceRowId::Npu {
            generation,
            device_id,
        } if *generation == DeviceGeneration::INITIAL && device_id.as_str() == "npu:0"
    ));
    let first_generation = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Npu);

    fold_npu(
        &mut app,
        1,
        NpuInventorySnapshot::discovered(vec![npu()], 10),
    );
    assert_eq!(
        app.projection()
            .device_row_projection_generation(DeviceRowFamily::Npu),
        first_generation,
        "a duplicate event sequence is ignored by both lifecycle and row revisions"
    );
    assert_eq!(
        app.projection().device_row_ids(DeviceRowFamily::Npu),
        [first_id]
    );

    fold_npu(
        &mut app,
        2,
        NpuInventorySnapshot::failed(FailureKind::ProviderFault, "inventory unavailable", 20),
    );
    assert!(
        app.projection()
            .device_row_ids(DeviceRowFamily::Npu)
            .is_empty()
    );
    let failed_generation = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Npu);
    assert_eq!(
        failed_generation.data_revision().get(),
        first_generation.data_revision().get() + 1
    );
    assert_eq!(
        failed_generation.order_revision().get(),
        first_generation.order_revision().get() + 1
    );

    fold_npu(
        &mut app,
        3,
        NpuInventorySnapshot::discovered(vec![npu()], 30),
    );
    assert!(matches!(
        app.projection().device_row_identity_at(DeviceRowFamily::Npu, 0),
        Some(DeviceRowId::Npu { generation, .. }) if generation == DeviceGeneration::INITIAL
    ));
    fold_npu(
        &mut app,
        4,
        NpuInventorySnapshot::discovered(Vec::new(), 40),
    );
    fold_npu(
        &mut app,
        5,
        NpuInventorySnapshot::discovered(vec![npu()], 50),
    );
    assert!(matches!(
        app.projection().device_row_identity_at(DeviceRowFamily::Npu, 0),
        Some(DeviceRowId::Npu { generation, .. }) if generation.get() == 2
    ));

    fold_npu(
        &mut app,
        6,
        NpuInventorySnapshot::discovered(
            vec![
                npu(),
                npu(),
                NpuDevice {
                    device_id: DeviceId::new("npu:other"),
                    ..NpuDevice::default()
                },
            ],
            60,
        ),
    );
    assert_eq!(
        app.projection().device_row_ids(DeviceRowFamily::Npu),
        [DeviceRowId::Npu {
            device_id: DeviceId::new("npu:other"),
            generation: DeviceGeneration::INITIAL,
        }],
        "duplicate NPU owner IDs have no stable row identity"
    );
}

#[test]
fn system_family_revisions_advance_after_commit_and_ignore_stale_source() {
    let mut app = ShellApp::new();
    app.apply_platform_batch(system_storage_batch(1, 1, &["disk:a", "disk:b"]));
    let first = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(first.data_revision().get(), 1);
    assert_eq!(first.order_revision().get(), 1);
    let anchor = app
        .projection()
        .device_row_anchor_at(DeviceRowFamily::Storage, 0)
        .expect("committed storage row");

    app.apply_platform_batch(system_storage_batch(2, 2, &["disk:b", "disk:a"]));
    let reordered = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(reordered.data_revision().get(), 2);
    assert_eq!(reordered.order_revision().get(), 2);
    assert_eq!(
        app.projection().reconcile_device_row_identity(anchor.id()),
        Some(1)
    );
    assert_eq!(
        app.projection().resolve_device_row_anchor(&anchor),
        None,
        "the old geometry anchor is stale after a committed reorder"
    );

    app.apply_platform_batch(system_storage_batch(1, 1, &["disk:a", "disk:b"]));
    assert_eq!(
        app.projection()
            .device_row_projection_generation(DeviceRowFamily::Storage),
        reordered,
        "an out-of-order lifecycle event and stale telemetry projection do not advance row revisions"
    );
    assert_eq!(
        app.projection().device_row_ids(DeviceRowFamily::Storage),
        [
            DeviceRowId::StorageDisk {
                device_id: DeviceId::new("disk:b"),
                generation: DeviceGeneration::INITIAL,
            },
            DeviceRowId::StorageDisk {
                device_id: DeviceId::new("disk:a"),
                generation: DeviceGeneration::INITIAL,
            },
        ]
    );
}

#[test]
fn accepted_new_lifecycle_invalidates_old_rows_before_later_snapshot_commit() {
    let mut app = ShellApp::new();
    app.apply_platform_batch(system_storage_batch(1, 1, &["disk:reused"]));
    let old_id = app
        .projection()
        .device_row_identity_at(DeviceRowFamily::Storage, 0)
        .expect("initial generation row");
    let initial_revision = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Storage);
    assert!(matches!(
        &old_id,
        DeviceRowId::StorageDisk { generation, .. }
            if *generation == DeviceGeneration::INITIAL
    ));

    let absent = DeviceLifecycle {
        presence: DevicePresence::Absent,
        state: DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(10),
        },
        generation: DeviceGeneration::INITIAL,
        first_seen_ms: Some(10),
        last_seen_ms: Some(10),
        absent_since_ms: Some(20),
    };
    let absent_observation = StorageTelemetryObservation::current(
        Vec::new(),
        20,
        telemetry_source("fixture.storage"),
        Vec::new(),
        BTreeMap::from([(DeviceId::new("disk:reused"), absent)]),
    );
    let next_generation = DeviceGeneration::INITIAL.next();
    let returned_observation =
        storage_observation_with_generation(&["disk:reused"], 30, next_generation);
    let mut partial_batch =
        system_storage_batch_with_observation(3, 2, returned_observation.clone(), false);
    partial_batch.system_telemetry_outcomes = vec![
        storage_lifecycle_outcome(2, 2, absent_observation),
        storage_lifecycle_outcome(3, 2, returned_observation.clone()),
    ];

    app.apply_platform_batch(partial_batch);

    assert!(
        app.projection()
            .device_row_ids(DeviceRowFamily::Storage)
            .is_empty(),
        "the old snapshot generation cannot remain selectable after lifecycle advances"
    );
    let partial_revision = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(
        partial_revision.data_revision().get(),
        initial_revision.data_revision().get() + 1
    );
    assert_eq!(
        partial_revision.order_revision().get(),
        initial_revision.order_revision().get() + 1
    );

    let mut commit = PlatformEventBatch::default();
    commit
        .system_telemetry_projections
        .push(full_telemetry_projection(
            SystemTelemetryRevision::new(2),
            &returned_observation,
        ));
    app.apply_platform_batch(commit);

    assert!(matches!(
        app.projection()
            .device_row_identity_at(DeviceRowFamily::Storage, 0),
        Some(DeviceRowId::StorageDisk { generation, .. }) if generation == next_generation
    ));
    let committed_revision = app
        .projection()
        .device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(
        committed_revision.data_revision().get(),
        partial_revision.data_revision().get() + 1
    );
    assert_eq!(
        committed_revision.order_revision().get(),
        partial_revision.order_revision().get() + 1
    );
    assert_eq!(
        app.projection().reconcile_device_row_identity(&old_id),
        None,
        "the old incarnation never resolves to the replacement"
    );
}

#[test]
fn global_lifecycle_partition_conflict_fails_closed_for_the_non_owner_family() {
    let mut store = SystemProjectionStore::default();
    admit_storage(&mut store, "opaque:shared", 1);
    let network = NetworkTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(DeviceId::new("opaque:shared"), lifecycle(20))]),
    );
    let result = store
        .device_lifecycle_projection
        .apply_network_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &network);
    assert!(matches!(
        result,
        DeviceLifecycleApplyResult::Applied(delta)
            if matches!(delta.issues.as_slice(), [DeviceLifecycleProjectionIssue::OwnershipConflict {
                authoritative_partition: DeviceLifecyclePartition::SystemStorage,
                observed_partition: DeviceLifecyclePartition::SystemNetwork,
                ..
            }])
    ));
    store.snapshot = Some(SystemSnapshot {
        disks: vec![disk_metrics("opaque:shared", DeviceGeneration::INITIAL)],
        networks: vec![network_metrics("opaque:shared", DeviceGeneration::INITIAL)],
        ..SystemSnapshot::default()
    });

    assert_eq!(store.device_row_ids(DeviceRowFamily::Storage).len(), 1);
    assert!(
        store.device_row_ids(DeviceRowFamily::Network).is_empty(),
        "the first lifecycle partition remains the sole DeviceId owner"
    );
}

#[test]
fn data_and_order_revisions_are_separate_and_unrelated_families_do_not_advance() {
    let mut store = store_with_two_disks();
    let first_identity = store
        .device_row_identity_at(DeviceRowFamily::Storage, 0)
        .expect("first disk row");
    let anchor = store
        .device_row_anchor_at(DeviceRowFamily::Storage, 0)
        .expect("first disk anchor");
    let initial = anchor.generation();
    assert_eq!(store.resolve_device_row_anchor(&anchor), Some(0));

    let mut value_only = store.snapshot.clone().expect("test snapshot");
    value_only.disks[0].name = "renamed display value".into();
    store.snapshot = Some(value_only);
    store.note_device_row_family_changed(DeviceRowFamily::Storage);
    let value_revision = store.device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(
        value_revision.data_revision().get(),
        initial.data_revision().get() + 1
    );
    assert_eq!(value_revision.order_revision(), initial.order_revision());
    assert_eq!(
        store.reconcile_device_row_identity(&first_identity),
        Some(0)
    );
    assert_eq!(store.resolve_device_row_anchor(&anchor), None);

    let mut reordered = store.snapshot.clone().expect("test snapshot");
    reordered.disks.swap(0, 1);
    store.snapshot = Some(reordered);
    store.note_device_row_family_changed(DeviceRowFamily::Storage);
    let reordered_revision = store.device_row_projection_generation(DeviceRowFamily::Storage);
    assert_eq!(
        reordered_revision.order_revision().get(),
        value_revision.order_revision().get() + 1
    );
    assert_eq!(
        store.reconcile_device_row_identity(&first_identity),
        Some(1)
    );

    let storage_before_network = store.device_row_projection_generation(DeviceRowFamily::Storage);
    store.note_device_row_family_changed(DeviceRowFamily::Network);
    assert_eq!(
        store.device_row_projection_generation(DeviceRowFamily::Storage),
        storage_before_network,
        "a network refresh cannot invalidate storage geometry"
    );
}
