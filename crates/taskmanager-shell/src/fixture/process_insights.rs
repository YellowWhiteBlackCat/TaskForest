//! Deterministic Process Insights state for headless layout and capture.

use crate::{ShellApp, fixture::edit_processes};

use std::net::SocketAddr;
use taskmanager_application::{
    ProcessInsightFacetEvent, ProcessInsightObservation, ProcessInsightsProjection,
    ProcessInsightsRevision, ProjectedProcessInsights,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem};
use taskmanager_core::core::process_telemetry::ProcessInsightSnapshot;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
    IsolationKind, LimitValue, LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry,
    NamespaceAuditStatus, OpenFileEntry, OpenFileKind, ProcessCapabilities, ProcessConnection,
    ProcessEnvironment, ProcessEnvironmentEntry, ProcessGpuDevice, ProcessGpuSnapshot,
    ProcessIdentity, ProcessIsolation, ProcessNetworkSnapshot, ProcessOpenFiles,
    ProcessResourceSnapshot, ProcessTelemetrySnapshot, ProcessThreadInfo, ProcessThreads,
    ResourceGroupMembership, ThreadState,
};

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::{
    ProcessGpuEngineUsage, ProcessGpuEngines, ProcessResourceObservations, ResourceObservation,
    ScalarObservation,
};

pub fn process_insights_snapshot() -> ProcessTelemetrySnapshot {
    let now_ms = 42_000;
    ProcessTelemetrySnapshot {
        identity: ProcessIdentity {
            pid: 4242,
            start_token: 987_654,
        },
        state: DeviceState::healthy(now_ms),
        network: ProcessNetworkSnapshot {
            state: DeviceState::healthy(now_ms),
            connections: vec![
                ProcessConnection {
                    transport: ConnectionTransport::Tcp,
                    family: ConnectionAddressFamily::Ipv4,
                    local: SocketAddr::from(([127, 0, 0, 1], 51_842)).into(),
                    remote: SocketAddr::from(([10, 20, 0, 8], 443)).into(),
                    state: ConnectionState::Established,
                    provider_key: Some(424_242.into()),
                    rtt_ms: Some(12.4),
                },
                ProcessConnection {
                    transport: ConnectionTransport::Udp,
                    family: ConnectionAddressFamily::Ipv6,
                    local: SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 53_535)).into(),
                    remote: SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 53)).into(),
                    state: ConnectionState::Unconnected,
                    provider_key: Some(424_243.into()),
                    rtt_ms: None,
                },
                ProcessConnection {
                    transport: ConnectionTransport::Local,
                    family: ConnectionAddressFamily::Local,
                    local: ConnectionEndpoint::local("/run/taskmanager.sock"),
                    remote: ConnectionEndpoint::Unspecified,
                    state: ConnectionState::Listen,
                    provider_key: Some("fixture-local-token".to_string().into()),
                    rtt_ms: None,
                },
            ],
            rx_bytes_per_sec: None,
            tx_bytes_per_sec: None,
            traffic_state: DeviceState::default(),
            traffic_failure: None,
            traffic_provider: None,
            connection_counters: None,
        },
        gpu: ProcessGpuSnapshot {
            state: DeviceState::healthy(now_ms),
            devices: vec![ProcessGpuDevice {
                device_id: "gpu:pci:0000:03:00.0".into(),
                memory_bytes: Some(768 * 1024 * 1024),
                utilization_pct: Some(37.5),
                engine_time_ns: Some(8_000_000_000),
            }],
            engines: ProcessGpuEngines {
                state: DeviceState::healthy(now_ms),
                engines: vec![
                    ProcessGpuEngineUsage {
                        name: "render".into(),
                        usage_pct: ScalarObservation::available(37.5, now_ms),
                        engine_time_ns: ScalarObservation::available(8_000_000_000, now_ms),
                        engine_cycles: ScalarObservation::default(),
                    },
                    ProcessGpuEngineUsage {
                        name: "video".into(),
                        usage_pct: ScalarObservation::available(0.0, now_ms),
                        engine_time_ns: ScalarObservation::available(1_500_000_000, now_ms),
                        engine_cycles: ScalarObservation::default(),
                    },
                ],
            },
        },
        resources: ProcessResourceSnapshot::from_observations(
            DeviceState::healthy(now_ms),
            ProcessResourceObservations {
                resource_groups: ResourceObservation::current(
                    vec![ResourceGroupMembership {
                        provider: ProviderId::borrowed("fixture.cgroup"),
                        native_hierarchy_id: Some(0),
                        capabilities: Vec::new(),
                        native_locator: "/system.slice/telemetry-worker.scope".into(),
                    }],
                    now_ms,
                ),
                memory_usage_bytes: ResourceObservation::current(384 * 1024 * 1024, now_ms),
                memory_limit: ResourceObservation::current(
                    LimitValue::Value(1024 * 1024 * 1024),
                    now_ms,
                ),
                cpu_time_quota_micros: ResourceObservation::current(
                    LimitValue::Value(150_000),
                    now_ms,
                ),
                cpu_time_period_micros: ResourceObservation::current(100_000, now_ms),
                process_count: ResourceObservation::current(7, now_ms),
                process_limit: ResourceObservation::current(LimitValue::Value(64), now_ms),
                ..ProcessResourceObservations::default()
            },
            Vec::new(),
        ),
        isolation: ProcessIsolation {
            state: DeviceState::healthy(now_ms),
            kind: Some(IsolationKind::Docker),
            container_id: Some("0123456789abcdef0123456789abcdef".into()),
            sandboxed: Some(true),
            seccomp_mode: Some(2),
            no_new_privs: Some(true),
            child_subreaper: Some(false),
            security_profile: Some("docker-default".into()),
            yama_ptrace_scope: Some(1),
            capabilities: Some(ProcessCapabilities::from_masks(
                DeviceState::healthy(now_ms),
                Some(0),
                Some(1 << 10),
                Some(1 << 10),
                Some(1 << 10),
                Some(0),
            )),
            namespaces: Some(LinuxNamespaceAudit::from_entries(
                DeviceState::healthy(now_ms),
                LinuxNamespaceKind::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, kind)| NamespaceAuditEntry {
                        kind,
                        status: if kind == LinuxNamespaceKind::User {
                            NamespaceAuditStatus::Host {
                                inode: 1000 + index as u64,
                            }
                        } else {
                            NamespaceAuditStatus::Isolated {
                                inode: 2000 + index as u64,
                                host_inode: 1000 + index as u64,
                            }
                        },
                    })
                    .collect(),
            )),
            root_uid_host: None,
            rootfs_read_only: None,
            sandbox_confinement: None,
            sandbox_permissions: Vec::new(),
        },
        open_files: ProcessOpenFiles {
            state: DeviceState::healthy(now_ms),
            unreadable_count: 1,
            entries: vec![
                OpenFileEntry {
                    fd: 0,
                    kind: OpenFileKind::File,
                    target: Some("/dev/null".into()),
                    deleted: false,
                },
                OpenFileEntry {
                    fd: 3,
                    kind: OpenFileKind::Socket,
                    target: Some("socket:[424242]".into()),
                    deleted: false,
                },
                OpenFileEntry {
                    fd: 7,
                    kind: OpenFileKind::File,
                    target: Some("/var/log/telemetry.log".into()),
                    deleted: false,
                },
                OpenFileEntry {
                    fd: 9,
                    kind: OpenFileKind::Other,
                    target: None,
                    deleted: false,
                },
            ],
        },
        environment: ProcessEnvironment {
            state: DeviceState::healthy(now_ms),
            working_directory: Some("/opt/app".into()),
            entries: vec![
                ProcessEnvironmentEntry {
                    key: "PATH".into(),
                    value: "/usr/local/bin:/usr/bin:/bin".into(),
                },
                ProcessEnvironmentEntry {
                    key: "SHELL".into(),
                    value: "/bin/bash".into(),
                },
                ProcessEnvironmentEntry {
                    key: "USER".into(),
                    value: "app".into(),
                },
            ],
            truncated_count: 0,
        },
        threads: ProcessThreads {
            state: DeviceState::healthy(now_ms),
            threads: vec![
                ProcessThreadInfo {
                    tid: 4242,
                    comm: "telemetry-main".into(),
                    state: ThreadState::Sleep,
                    cpu_time_secs: Some(12.5),
                    cpu_percent: Some(18.5),
                    wchan: Some("poll".into()),
                    run_queue_wait_ns: None,
                    wait_kind: None,
                },
                ProcessThreadInfo {
                    tid: 4243,
                    comm: "worker".into(),
                    state: ThreadState::Running,
                    cpu_time_secs: Some(48.0),
                    cpu_percent: Some(72.0),
                    wchan: None,
                    run_queue_wait_ns: None,
                    wait_kind: None,
                },
            ],
        },
    }
}

/// Deliver all independent fixture domains through the real application reducer.
pub fn process_insights_projection(
    target: FrozenProcessIdentity,
) -> Option<ProjectedProcessInsights> {
    process_insights_projection_from_snapshot(target, process_insights_snapshot())
}

pub fn process_insights_projection_from_snapshot(
    target: FrozenProcessIdentity,
    mut snapshot: ProcessTelemetrySnapshot,
) -> Option<ProjectedProcessInsights> {
    let revision = ProcessInsightsRevision::new(1);
    snapshot.identity.pid = target.pid;
    snapshot.identity.start_token = target.authoritative_start_token()?;
    let identity = snapshot.identity;
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    for event in [
        ProcessInsightFacetEvent::Network(observation(
            &target,
            revision,
            identity,
            snapshot.network,
        )),
        ProcessInsightFacetEvent::Gpu(observation(&target, revision, identity, snapshot.gpu)),
        ProcessInsightFacetEvent::Resources(observation(
            &target,
            revision,
            identity,
            snapshot.resources,
        )),
        ProcessInsightFacetEvent::Isolation(observation(
            &target,
            revision,
            identity,
            snapshot.isolation,
        )),
        ProcessInsightFacetEvent::Threads(observation(
            &target,
            revision,
            identity,
            snapshot.threads,
        )),
        ProcessInsightFacetEvent::OpenFiles(observation(
            &target,
            revision,
            identity,
            snapshot.open_files,
        )),
        ProcessInsightFacetEvent::Environment(observation(
            &target,
            revision,
            identity,
            snapshot.environment,
        )),
    ] {
        let _ = tracker.apply(&event);
    }
    tracker.snapshot()
}
fn observation<T>(
    target: &FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
    identity: ProcessIdentity,
    value: T,
) -> Box<ProcessInsightObservation<T>> {
    Box::new(ProcessInsightObservation {
        target: target.clone(),
        revision,
        snapshot: ProcessInsightSnapshot { identity, value },
    })
}

/// Seed scalar observations and recent histories for the same captured process.
pub fn seed_process_properties_history(shell: &mut ShellApp) {
    edit_processes(shell, |rows| {
        let Some(process) = rows.as_mut().and_then(|rows| rows.first_mut()) else {
            return;
        };
        seed_process_properties_observations(process);
    });
}

/// Scalar and recent-history facts for one exact process incarnation.
pub fn seed_process_properties_observations(process: &mut ProcessItem) {
    let now = 42_000;
    let mib = 1024_u64 * 1024;
    let mut scalars = *process.scalar_observations();
    scalars.cpu_percentage = ScalarObservation::available(37.5, now);
    scalars.memory_bytes = ScalarObservation::available(315 * mib, now);
    scalars.memory_pss_bytes = ScalarObservation::available(240 * mib, now);
    scalars.memory_uss_bytes = ScalarObservation::available(192 * mib, now);
    scalars.memory_anon_huge_pages_bytes = ScalarObservation::available(32 * mib, now);
    scalars.swap_bytes = ScalarObservation::available(64 * mib, now);
    scalars.disk_read_bytes_per_sec = ScalarObservation::available(8 * mib, now);
    scalars.disk_write_bytes_per_sec = ScalarObservation::available(2 * mib, now);
    process.apply_scalar_observations(scalars);
    process.cpu_history = (0..60)
        .map(|index| {
            if index == 0 {
                79.0
            } else {
                20.0 + (index % 12) as f32 * 2.0
            }
        })
        .collect();
    if let Some(last) = process.cpu_history.last_mut() {
        *last = 37.5;
    }
    process.mem_history = (0..60)
        .map(|index| (256 + index) as f32 * mib as f32)
        .collect();
    process.disk_read_history = (0..60)
        .map(|index| (index % 9) as f32 * mib as f32)
        .collect();
    process.disk_write_history = (0..60)
        .map(|index| (index % 3) as f32 * mib as f32)
        .collect();
}

/// Positive process facts required before a native properties capture can acknowledge data.
pub fn process_properties_capture_data_ready(shell: &ShellApp, scenario: &str) -> bool {
    let Some(target) = shell.process_properties_target() else {
        return false;
    };
    let Some(process) = target
        .live_key()
        .and_then(|key| shell.process_by_identity(key))
    else {
        return false;
    };
    match scenario {
        "process-properties-performance" => [
            &process.cpu_history,
            &process.mem_history,
            &process.disk_read_history,
            &process.disk_write_history,
        ]
        .into_iter()
        .all(|samples| samples.iter().filter(|value| value.is_finite()).count() >= 2),
        "process-memory-pss-swap" => {
            process.current_memory_pss_bytes().is_some()
                && process.current_memory_uss_bytes().is_some()
                && process.current_swap_bytes().is_some()
        }
        _ => {
            let Some(projection) = shell
                .projection()
                .process_insights
                .as_ref()
                .filter(|p| p.target == *target)
            else {
                return false;
            };
            process_insights_capture_data_ready(projection, scenario)
        }
    }
}

/// Shared positive insight facts; denied or pending fixtures cannot certify pixels.
pub fn process_insights_capture_data_ready(
    projection: &ProjectedProcessInsights,
    scenario: &str,
) -> bool {
    use taskmanager_application::ProcessInsightFacetState;
    match scenario {
        "process-network-details" => {
            matches!(&projection.network, ProcessInsightFacetState::Current(value)
            if !value.connections.is_empty() && value.connections.iter().any(|connection| connection.rtt_ms.is_some()))
        }
        "process-gpu-details" => matches!(&projection.gpu, ProcessInsightFacetState::Current(value)
            if !value.devices.is_empty() && !value.engines.engines.is_empty()),
        "process-resource-limits" => {
            matches!(&projection.resources, ProcessInsightFacetState::Current(value)
            if value.current_memory_limit().is_some() && value.current_process_limit().is_some())
        }
        "process-isolation" => {
            matches!(&projection.isolation, ProcessInsightFacetState::Current(value)
            if value.seccomp_mode.is_some() && value.namespaces.is_some())
        }
        _ => false,
    }
}
