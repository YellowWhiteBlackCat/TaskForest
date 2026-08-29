//! Deterministic Process Insights state for headless layout and capture.

use std::net::SocketAddr;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
    IsolationKind, LimitValue, OpenFileEntry, OpenFileKind, ProcessConnection, ProcessEnvironment,
    ProcessGpuDevice, ProcessGpuSnapshot, ProcessIdentity, ProcessIsolation,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessTelemetrySnapshot,
    ProcessThreadInfo, ProcessThreads, ResourceGroupMembership, ThreadState,
};

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::{
    ProcessGpuEngineUsage, ProcessGpuEngines, ProcessResourceObservations, ResourceObservation,
    ScalarObservation,
};

use super::super::ProcessInsightsState;

pub fn process_insights_capture_fixture() -> ProcessInsightsState {
    let now_ms = 42_000;
    ProcessInsightsState::Ready(Box::new(ProcessTelemetrySnapshot {
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
                },
                ProcessConnection {
                    transport: ConnectionTransport::Udp,
                    family: ConnectionAddressFamily::Ipv6,
                    local: SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 53_535)).into(),
                    remote: SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 53)).into(),
                    state: ConnectionState::Unconnected,
                    provider_key: Some(424_243.into()),
                },
                ProcessConnection {
                    transport: ConnectionTransport::Local,
                    family: ConnectionAddressFamily::Local,
                    local: ConnectionEndpoint::local("/run/taskmanager.sock"),
                    remote: ConnectionEndpoint::Unspecified,
                    state: ConnectionState::Listen,
                    provider_key: Some("fixture-local-token".to_string().into()),
                },
            ],
            rx_bytes_per_sec: None,
            tx_bytes_per_sec: None,
            traffic_state: DeviceState::default(),
            traffic_failure: None,
            traffic_provider: None,
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
        },
        open_files: ProcessOpenFiles {
            state: DeviceState::healthy(now_ms),
            unreadable_count: 1,
            entries: vec![
                OpenFileEntry {
                    fd: 0,
                    kind: OpenFileKind::File,
                    target: Some("/dev/null".into()),
                },
                OpenFileEntry {
                    fd: 3,
                    kind: OpenFileKind::Socket,
                    target: Some("socket:[424242]".into()),
                },
                OpenFileEntry {
                    fd: 7,
                    kind: OpenFileKind::File,
                    target: Some("/var/log/telemetry.log".into()),
                },
                OpenFileEntry {
                    fd: 9,
                    kind: OpenFileKind::Other,
                    target: None,
                },
            ],
        },
        environment: ProcessEnvironment::default(),
        threads: ProcessThreads {
            state: DeviceState::healthy(now_ms),
            threads: vec![
                ProcessThreadInfo {
                    tid: 4242,
                    comm: "telemetry-main".into(),
                    state: ThreadState::Sleep,
                    cpu_time_secs: Some(12.5),
                    cpu_percent: Some(18.5),
                },
                ProcessThreadInfo {
                    tid: 4243,
                    comm: "worker".into(),
                    state: ThreadState::Running,
                    cpu_time_secs: Some(48.0),
                    cpu_percent: Some(72.0),
                },
            ],
        },
    }))
}
