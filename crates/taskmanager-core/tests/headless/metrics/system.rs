use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};

use super::*;
use crate::core::{
    DeviceId, DeviceLifecycle, DeviceStatus, DiskPartition, FailureKind, ProviderId,
    ScalarObservation, SourceOutcome,
};

fn source(provider: &'static str, outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count: 1,
    }
}

fn host_facts(observed_at_ms: u64) -> HostRuntimeFacts {
    HostRuntimeFacts {
        uptime_secs: ScalarObservation::available(90, observed_at_ms),
        processes: ScalarObservation::available(4, observed_at_ms),
        threads: ScalarObservation::available(12, observed_at_ms),
    }
}

fn current_domains() -> SystemTelemetryDomains {
    SystemTelemetryDomains {
        host: HostRuntimeObservation::current(host_facts(10), 10, Vec::new()),
        cpu: CpuTelemetryObservation::current(CpuMetrics::default(), 20, Vec::new()),
        memory: MemoryTelemetryObservation::current(MemoryMetrics::default(), 30, Vec::new()),
        storage: StorageTelemetryObservation::current(
            Vec::new(),
            40,
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        ),
        network: NetworkTelemetryObservation::current(
            Vec::new(),
            50,
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        ),
        gpu: GpuTelemetryObservation::current(
            Vec::new(),
            60,
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        ),
    }
}

#[test]
fn six_domain_observation_types_are_distinct() {
    let types = HashSet::from([
        TypeId::of::<HostRuntimeObservation>(),
        TypeId::of::<CpuTelemetryObservation>(),
        TypeId::of::<MemoryTelemetryObservation>(),
        TypeId::of::<StorageTelemetryObservation>(),
        TypeId::of::<NetworkTelemetryObservation>(),
        TypeId::of::<GpuTelemetryObservation>(),
    ]);

    assert_eq!(types.len(), 6);
}

#[test]
fn partial_is_current_while_stale_is_last_known_only() {
    let partial = CpuTelemetryObservation::partial(
        CpuMetrics::default(),
        42,
        FailureKind::PermissionDenied,
        vec![
            source(
                "test.z",
                SourceOutcome::Unavailable(FailureKind::PermissionDenied),
            ),
            source("test.a", SourceOutcome::Available),
        ],
    );
    assert!(partial.current_value().is_some());
    assert_eq!(partial.state().observed_at_ms(), Some(42));
    assert_eq!(
        partial.state().failure(),
        Some(FailureKind::PermissionDenied)
    );
    assert_eq!(partial.sources()[0].provider.as_str(), "test.a");

    let stale = CpuTelemetryObservation::stale(
        CpuMetrics::default(),
        21,
        FailureKind::TemporarilyUnavailable,
        Vec::new(),
    );
    assert!(stale.current_value().is_none());
    assert!(stale.last_known_value().is_some());
    assert_eq!(stale.state().observed_at_ms(), None);
    assert_eq!(stale.state().last_success_ms(), Some(21));
}

#[test]
fn compatibility_projection_rejects_stale_or_unavailable_domains() {
    let mut domains = current_domains();
    domains.cpu = CpuTelemetryObservation::stale(
        CpuMetrics::default(),
        20,
        FailureKind::TemporarilyUnavailable,
        Vec::new(),
    );
    assert!(SystemSnapshot::from_current_domains(&domains).is_none());

    domains.cpu = CpuTelemetryObservation::unavailable(FailureKind::PermissionDenied, Vec::new());
    assert!(SystemSnapshot::from_current_domains(&domains).is_none());
}

#[test]
fn compatibility_projection_never_turns_unknown_host_counts_into_zero() {
    let mut domains = current_domains();
    let mut facts = host_facts(10);
    facts.processes = ScalarObservation::unavailable(FailureKind::PermissionDenied);
    domains.host =
        HostRuntimeObservation::partial(facts, 10, FailureKind::PermissionDenied, Vec::new());

    assert!(SystemSnapshot::from_current_domains(&domains).is_none());

    let zero_facts = HostRuntimeFacts {
        uptime_secs: ScalarObservation::available(0, 10),
        processes: ScalarObservation::available(0, 10),
        threads: ScalarObservation::available(0, 10),
    };
    domains.host = HostRuntimeObservation::current(zero_facts, 10, Vec::new());
    let snapshot =
        SystemSnapshot::from_current_domains(&domains).expect("observed zeroes stay current");
    assert_eq!(snapshot.uptime_secs, 0);
    assert_eq!(snapshot.processes, 0);
    assert_eq!(snapshot.threads, Some(0));
}

#[test]
fn compatibility_timestamp_is_latest_domain_time_not_atomic_sample_proof() {
    let domains = current_domains();
    let snapshot = SystemSnapshot::from_current_domains(&domains).expect("all domains are current");

    assert_eq!(domains.host.state().observed_at_ms(), Some(10));
    assert_eq!(domains.cpu.state().observed_at_ms(), Some(20));
    assert_eq!(domains.gpu.state().observed_at_ms(), Some(60));
    assert_eq!(snapshot.timestamp_ms, 60);
}

#[test]
fn compatibility_projection_rebases_partition_children_to_the_parent_generation() {
    let mut partition = DiskPartition::new("nvme0n1p1");
    partition.device_id = "partition:disk:wwid:old:nvme0n1p1".into();
    partition.parent_device_id = "disk:wwid:old".into();
    partition.device_generation = crate::core::DeviceGeneration::new(1);
    let mut disk = DiskMetrics::new("");
    disk.device_id = "disk:wwid:new".into();
    disk.device_generation = crate::core::DeviceGeneration::new(4);
    disk.partitions = vec![partition];
    let mut domains = current_domains();
    domains.storage = StorageTelemetryObservation::current(
        vec![disk],
        40,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
    );

    let snapshot = SystemSnapshot::from_current_domains(&domains).expect("all domains are current");
    let partition = &snapshot.disks[0].partitions[0];
    assert_eq!(partition.parent_device_id, "disk:wwid:new");
    assert_eq!(partition.device_id, "partition:disk:wwid:new:nvme0n1p1");
    assert_eq!(partition.device_generation.get(), 4);
}

#[test]
fn device_domains_keep_their_own_lifecycle_and_provider_state() {
    let lifecycle = DeviceLifecycle {
        state: crate::core::DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: Some(40),
        },
        ..Default::default()
    };
    let provider_state = ProviderRuntimeState {
        provider: ProviderId::borrowed("test.storage"),
        status: DeviceStatus::Healthy,
        last_success_ms: Some(40),
    };
    let mut domains = current_domains();
    domains.storage = StorageTelemetryObservation::current(
        Vec::new(),
        40,
        Vec::new(),
        vec![provider_state],
        BTreeMap::from([(DeviceId::new("storage:test"), lifecycle)]),
    );

    assert!(domains.network.device_lifecycles().is_empty());
    assert!(domains.gpu.provider_states().is_empty());
    assert_eq!(
        domains.storage.device_lifecycles().get("storage:test"),
        Some(&lifecycle)
    );
    let snapshot =
        SystemSnapshot::from_current_domains(&domains).expect("domain identities are unique");
    assert_eq!(
        snapshot
            .provider_states
            .first()
            .map(|state| state.provider.as_str()),
        Some("test.storage")
    );
    assert_eq!(
        snapshot
            .device_lifecycles
            .get("storage:test")
            .map(|entry| entry.state.status),
        Some(DeviceStatus::Healthy)
    );
}

#[test]
fn unavailable_state_serializes_without_a_value_or_success_time() {
    let observation = MemoryTelemetryObservation::unavailable(FailureKind::Unsupported, Vec::new());
    let value = serde_json::to_value(&observation).expect("observation should serialize");

    assert_eq!(value["value"]["state"], "unavailable");
    assert_eq!(value["value"]["failure"], "unsupported");
    assert!(value["value"].get("value").is_none());
    assert_eq!(observation.state().last_success_ms(), None);
}
