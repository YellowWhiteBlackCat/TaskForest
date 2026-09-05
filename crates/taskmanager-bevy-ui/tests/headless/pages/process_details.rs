//! Behavior tests for the Bevy selected-process details projection.

use taskmanager_application::process_details_vm::ProcessDetailsField;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;

use taskmanager_shell::{ShellApp, fixture};

use super::projection;

fn shell_with(mut process: ProcessItem) -> ShellApp {
    let mut scalars = *process.scalar_observations();
    scalars.cpu_percentage = ScalarObservation::available(17.5, 1);
    scalars.memory_bytes = ScalarObservation::available(256 * 1024 * 1024, 1);
    scalars.threads = ScalarObservation::available(6, 1);
    process.apply_scalar_observations(scalars);
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(vec![process]));
    shell
}

#[test]
fn selected_projection_uses_the_shared_vm_and_keeps_insights_typed() {
    let shell = shell_with(ProcessItem::new(42, "worker"));
    let view = projection(&shell);

    assert_eq!(
        view.selected,
        Some(super::ProcessDetailsSelection {
            identity: None,
            pid: 42,
            name: "worker".to_owned(),
        })
    );
    let value = |field: ProcessDetailsField| {
        view.overview
            .iter()
            .find(|row| row.label == taskmanager_application::i18n::t(field_label(field)))
            .map(|row| row.value.as_str())
    };
    assert_eq!(value(ProcessDetailsField::Cpu), Some("17.5%"));
    assert_eq!(value(ProcessDetailsField::Memory), Some("256.0 MiB"));
    assert_eq!(value(ProcessDetailsField::Threads), Some("6"));
    assert_eq!(view.insights.len(), 7);
    assert!(
        view.insights
            .iter()
            .all(|card| card.value == taskmanager_application::i18n::t("proc_insights.collecting")),
        "no process-insights projection means collecting, never fabricated zeros"
    );
}

#[test]
fn empty_process_projection_is_an_explicit_unselected_state() {
    let view = projection(&ShellApp::new());
    assert_eq!(view.selected, None);
    assert!(view.overview.is_empty());
    assert!(view.insights.is_empty());
}

fn field_label(field: ProcessDetailsField) -> &'static str {
    match field {
        ProcessDetailsField::Cpu => "common.cpu",
        ProcessDetailsField::Memory => "common.memory",
        ProcessDetailsField::Threads => "common.threads",
        _ => unreachable!("test only looks up the three scalar rows above"),
    }
}

#[test]
fn resources_summary_empty_keeps_honest_gap() {
    use taskmanager_core::core::process_telemetry::ProcessResourceSnapshot;
    let snapshot = ProcessResourceSnapshot::default();
    assert_eq!(
        super::resources_summary(&snapshot),
        taskmanager_shell::presentation::MISSING_VALUE
    );
}

#[test]
fn resources_summary_exposes_memory_cpu_pids_and_cgroup_locator() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::identity::ProviderId;
    use taskmanager_core::core::process_telemetry::ResourceObservation;
    use taskmanager_core::core::process_telemetry::{
        LimitValue, ProcessResourceObservations, ProcessResourceSnapshot, ResourceGroupMembership,
    };

    let now_ms = 1000;
    let snapshot = ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(now_ms),
        ProcessResourceObservations {
            resource_groups: ResourceObservation::current(
                vec![ResourceGroupMembership {
                    provider: ProviderId::borrowed("cgroup.test"),
                    native_hierarchy_id: Some(0),
                    capabilities: Vec::new(),
                    native_locator: "/system.slice/worker.scope".into(),
                }],
                now_ms,
            ),
            memory_usage_bytes: ResourceObservation::current(256 * 1024 * 1024, now_ms),
            memory_limit: ResourceObservation::current(
                LimitValue::Value(1024 * 1024 * 1024),
                now_ms,
            ),
            cpu_time_quota_micros: ResourceObservation::current(LimitValue::Value(150_000), now_ms),
            cpu_time_period_micros: ResourceObservation::current(100_000, now_ms),
            process_count: ResourceObservation::current(7, now_ms),
            process_limit: ResourceObservation::current(LimitValue::Value(64), now_ms),
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    );

    let summary = super::resources_summary(&snapshot);
    assert_eq!(
        summary,
        "256.0 MiB / 1.0 GiB · CPU 150% · 7 / 64 Processes · /system.slice/worker.scope"
    );
}

#[test]
fn resources_summary_handles_unlimited_quotas_and_limits() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::ResourceObservation;
    use taskmanager_core::core::process_telemetry::{
        LimitValue, ProcessResourceObservations, ProcessResourceSnapshot,
    };

    let now_ms = 1000;
    let snapshot = ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(now_ms),
        ProcessResourceObservations {
            memory_usage_bytes: ResourceObservation::current(512 * 1024 * 1024, now_ms),
            memory_limit: ResourceObservation::current(LimitValue::Unlimited, now_ms),
            cpu_time_quota_micros: ResourceObservation::current(LimitValue::Unlimited, now_ms),
            process_count: ResourceObservation::current(3, now_ms),
            process_limit: ResourceObservation::current(LimitValue::Unlimited, now_ms),
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    );

    let summary = super::resources_summary(&snapshot);
    assert_eq!(summary, "512.0 MiB / ∞ · CPU ∞ · 3 / ∞ Processes");
}

#[test]
fn resources_summary_partial_observations_never_fabricate_missing_values() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::ResourceObservation;
    use taskmanager_core::core::process_telemetry::{
        ProcessResourceObservations, ProcessResourceSnapshot,
    };

    let now_ms = 1000;
    // Process count only, no memory or CPU observations.
    let snapshot = ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(now_ms),
        ProcessResourceObservations {
            process_count: ResourceObservation::current(12, now_ms),
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    );

    let summary = super::resources_summary(&snapshot);
    assert_eq!(summary, "12 Processes");
}

#[test]
fn isolation_summary_exposes_sandboxed_and_container_dimensions() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::{IsolationKind, ProcessIsolation};

    // Host process without sandboxed fact
    let host = ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: None,
        container_id: None,
        sandboxed: None,
    };
    assert_eq!(super::isolation_summary(&host), "Host process");

    // Host process explicitly not sandboxed
    let host_not_sandboxed = ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: None,
        container_id: None,
        sandboxed: Some(false),
    };
    assert_eq!(
        super::isolation_summary(&host_not_sandboxed),
        "Host process · not sandboxed"
    );

    // Host process sandboxed
    let host_sandboxed = ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: None,
        container_id: None,
        sandboxed: Some(true),
    };
    assert_eq!(
        super::isolation_summary(&host_sandboxed),
        "Host process · Sandboxed"
    );

    // Container with container ID and sandboxed true
    let docker_sandboxed = ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: Some(IsolationKind::Docker),
        container_id: Some("c-abcdef123".into()),
        sandboxed: Some(true),
    };
    assert_eq!(
        super::isolation_summary(&docker_sandboxed),
        "Docker · c-abcdef123 · Sandboxed"
    );

    // Container with container ID and sandboxed false
    let flatpak_not_sandboxed = ProcessIsolation {
        state: DeviceState::healthy(1),
        kind: Some(IsolationKind::Flatpak),
        container_id: Some("org.example.App".into()),
        sandboxed: Some(false),
    };
    assert_eq!(
        super::isolation_summary(&flatpak_not_sandboxed),
        "Flatpak · org.example.App · not sandboxed"
    );
}

#[test]
fn threads_summary_empty_and_populated_with_gap_honesty() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::{
        ProcessThreadInfo, ProcessThreads, ThreadState,
    };

    let empty = ProcessThreads::default();
    assert_eq!(
        super::threads_summary(&empty),
        taskmanager_application::i18n::t("proc_insights.no_threads")
    );

    let populated = ProcessThreads {
        state: DeviceState::healthy(1000),
        threads: vec![
            ProcessThreadInfo {
                tid: 101,
                comm: "worker-pool".into(),
                state: ThreadState::Running,
                cpu_time_secs: Some(2.5),
                cpu_percent: Some(25.0),
            },
            ProcessThreadInfo {
                tid: 102,
                comm: String::new(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
            },
            ProcessThreadInfo {
                tid: 103,
                comm: "io".into(),
                state: ThreadState::UninterruptibleSleep,
                cpu_time_secs: Some(0.1),
                cpu_percent: None,
            },
            ProcessThreadInfo {
                tid: 104,
                comm: "overflow".into(),
                state: ThreadState::Idle,
                cpu_time_secs: None,
                cpu_percent: Some(1.0),
            },
        ],
    };

    let summary = super::threads_summary(&populated);
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(lines[0], "4");
    assert_eq!(lines[1], "101  worker-pool  R  2.5s  25.0%");
    assert_eq!(lines[2], "102  —  S  —  —");
    assert_eq!(lines[3], "103  io  D  0.1s  —");
    assert_eq!(lines[4], "…");
    assert_eq!(lines.len(), 5);
}

#[test]
fn open_files_summary_empty_unreadable_and_populated() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::{
        OpenFileEntry, OpenFileKind, ProcessOpenFiles,
    };

    let empty = ProcessOpenFiles::default();
    assert_eq!(
        super::open_files_summary(&empty),
        taskmanager_application::i18n::t("proc_insights.no_open_files")
    );

    let files = ProcessOpenFiles {
        state: DeviceState::healthy(1000),
        entries: vec![
            OpenFileEntry {
                fd: 0,
                kind: OpenFileKind::File,
                target: Some("/dev/null".into()),
            },
            OpenFileEntry {
                fd: 1,
                kind: OpenFileKind::Socket,
                target: None, // unreadable readlink
            },
            OpenFileEntry {
                fd: 2,
                kind: OpenFileKind::Pipe,
                target: Some("pipe:[12345]".into()),
            },
            OpenFileEntry {
                fd: 3,
                kind: OpenFileKind::File,
                target: Some("/var/log/app.log".into()),
            },
        ],
        unreadable_count: 1,
    };

    let summary = super::open_files_summary(&files);
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "4 · 1 {}",
            taskmanager_application::i18n::t("proc_insights.unreadable")
        )
    );
    assert_eq!(lines[1], "0 -> /dev/null");
    assert_eq!(
        lines[2],
        format!(
            "1 -> {}",
            taskmanager_application::i18n::t("proc_insights.unreadable")
        )
    );
    assert_eq!(lines[3], "2 -> pipe:[12345]");
    assert_eq!(lines[4], "…");
    assert_eq!(lines.len(), 5);
}

#[test]
fn network_summary_formats_rates_endpoints_and_escalation() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::process_telemetry::{
        ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport, ProcessConnection,
        ProcessNetworkSnapshot,
    };

    let normal = ProcessNetworkSnapshot {
        state: DeviceState::healthy(1000),
        connections: vec![
            ProcessConnection {
                transport: ConnectionTransport::Tcp,
                family: ConnectionAddressFamily::Ipv4,
                local: ConnectionEndpoint::Ip(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    8080,
                )),
                remote: ConnectionEndpoint::Ip(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    45678,
                )),
                state: taskmanager_core::core::process_telemetry::ConnectionState::Established,
                provider_key: None,
            },
            ProcessConnection {
                transport: ConnectionTransport::Local,
                family: ConnectionAddressFamily::Unspecified,
                local: ConnectionEndpoint::Local {
                    path: "/run/user/1000/bus".into(),
                },
                remote: ConnectionEndpoint::Unspecified,
                state: taskmanager_core::core::process_telemetry::ConnectionState::Established,
                provider_key: None,
            },
        ],
        rx_bytes_per_sec: Some(1024 * 1024),
        tx_bytes_per_sec: Some(512 * 1024),
        traffic_state: DeviceState::healthy(1000),
        traffic_failure: None,
        traffic_provider: None,
    };

    let summary = super::network_summary(&normal);
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(lines[0], "2 · RX 1.0 MiB/s · TX 512.0 KiB/s");
    assert_eq!(lines[1], "TCP 127.0.0.1:8080 -> 127.0.0.1:45678");
    assert_eq!(lines[2], "UNIX /run/user/1000/bus -> —");
    assert_eq!(lines.len(), 3);

    // Escalation-requiring snapshot
    let escalating = ProcessNetworkSnapshot {
        state: DeviceState::healthy(1000),
        connections: Vec::new(),
        rx_bytes_per_sec: None,
        tx_bytes_per_sec: None,
        traffic_state: DeviceState::healthy(1000),
        traffic_failure: Some(FailureKind::RequiresEscalation),
        traffic_provider: None,
    };

    let esc_summary = super::network_summary(&escalating);
    assert!(esc_summary.contains("0 · RX — · TX —"));
    assert!(esc_summary.contains(taskmanager_application::i18n::t(
        "proc_insights.network_requires_escalation"
    )));
    assert!(esc_summary.contains(taskmanager_application::i18n::t(
        "proc_insights.enable_network_capture"
    )));
}

#[test]
fn environment_summary_formats_entries_and_truncation() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::process_telemetry::{ProcessEnvironment, ProcessEnvironmentEntry};

    let empty = ProcessEnvironment::default();
    assert_eq!(
        super::environment_summary(&empty),
        taskmanager_application::i18n::t("prop.environment_empty")
    );

    let env = ProcessEnvironment {
        state: DeviceState::healthy(1000),
        working_directory: None,
        entries: vec![
            ProcessEnvironmentEntry {
                key: "PATH".into(),
                value: "/usr/bin".into(),
            },
            ProcessEnvironmentEntry {
                key: "USER".into(),
                value: "alice".into(),
            },
            ProcessEnvironmentEntry {
                key: "SHELL".into(),
                value: "/bin/bash".into(),
            },
            ProcessEnvironmentEntry {
                key: "HOME".into(),
                value: "/tmp/alice".into(),
            },
        ],
        truncated_count: 15,
    };

    let summary = super::environment_summary(&env);
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(lines[0], "4 · +15");
    assert_eq!(lines[1], "PATH=/usr/bin");
    assert_eq!(lines[2], "USER=alice");
    assert_eq!(lines[3], "SHELL=/bin/bash");
    assert_eq!(lines[4], "…");
    assert_eq!(lines.len(), 5);
}

#[test]
fn gpu_summary_formats_devices_engines_and_cold_start_gap() {
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::process_telemetry::{
        ProcessGpuDevice, ProcessGpuEngineUsage, ProcessGpuEngines, ProcessGpuSnapshot,
    };

    let empty = ProcessGpuSnapshot::default();
    assert_eq!(
        super::gpu_summary(&empty),
        taskmanager_application::i18n::t("proc_insights.no_gpu")
    );

    let gpu = ProcessGpuSnapshot {
        state: DeviceState::healthy(1000),
        devices: vec![
            ProcessGpuDevice {
                device_id: "0".into(),
                memory_bytes: Some(1024 * 1024 * 1024),
                utilization_pct: Some(45.5),
                engine_time_ns: None,
            },
            ProcessGpuDevice {
                device_id: "1".into(),
                memory_bytes: None,
                utilization_pct: None,
                engine_time_ns: None,
            },
        ],
        engines: ProcessGpuEngines {
            state: DeviceState::healthy(1000),
            engines: vec![
                ProcessGpuEngineUsage {
                    name: "render".into(),
                    usage_pct: ScalarObservation::available(78.2, 1000),
                    engine_time_ns: ScalarObservation::available(2_500_000_000, 1000),
                    engine_cycles: ScalarObservation::unavailable(FailureKind::Unsupported),
                },
                ProcessGpuEngineUsage {
                    name: "copy".into(),
                    usage_pct: ScalarObservation::unavailable(FailureKind::Unsupported), // cold start gap
                    engine_time_ns: ScalarObservation::unavailable(FailureKind::Unsupported),
                    engine_cycles: ScalarObservation::available(1_500_000, 1000),
                },
            ],
        },
    };

    let summary = super::gpu_summary(&gpu);
    let lines: Vec<&str> = summary.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "2 · 2 {}",
            taskmanager_application::i18n::t("proc_insights.gpu_engines")
        )
    );
    assert_eq!(
        lines[1],
        format!(
            "{} #0 45.5% · {} 1.0 GiB",
            taskmanager_application::i18n::t("common.gpu"),
            taskmanager_application::i18n::t("gpu.vram_in_use")
        )
    );
    assert_eq!(
        lines[2],
        format!(
            "{} #1 — · {} —",
            taskmanager_application::i18n::t("common.gpu"),
            taskmanager_application::i18n::t("gpu.vram_in_use")
        )
    );
    assert_eq!(lines[3], "render  78.2%  2.5s");
    assert_eq!(lines[4], "copy  —  1.5M cycles");
    assert_eq!(lines.len(), 5);
}

#[test]
fn insight_cards_wires_network_escalation_action() {
    use taskmanager_application::{
        ProcessInsightFacetState, ProcessInsightUnavailable, ProcessInsightsProjection,
        ProcessInsightsRevision,
    };
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::process::FrozenProcessIdentity;
    use taskmanager_core::core::process_telemetry::ProcessNetworkSnapshot;

    let target = FrozenProcessIdentity::from_authoritative_parts(42, "test", 1, 1).unwrap();

    // Unavailable(RequiresEscalation)
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), ProcessInsightsRevision::new(1));
    let mut projection_esc = tracker.snapshot().unwrap();
    projection_esc.network = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation),
    );
    let cards = super::insight_cards(Some(&projection_esc));
    let net_card = cards
        .iter()
        .find(|c| c.title == taskmanager_application::i18n::t("proc_insights.network_throughput"))
        .unwrap();
    assert_eq!(
        net_card.action,
        Some(super::InsightCardAction::NetworkEscalation)
    );
    assert!(net_card.value.contains(taskmanager_application::i18n::t(
        "proc_insights.enable_network_capture"
    )));

    // Healthy without escalation
    let mut tracker_healthy = ProcessInsightsProjection::default();
    tracker_healthy.begin(target, ProcessInsightsRevision::new(1));
    let mut projection_healthy = tracker_healthy.snapshot().unwrap();
    projection_healthy.network =
        ProcessInsightFacetState::Current(ProcessNetworkSnapshot::default());
    let cards_healthy = super::insight_cards(Some(&projection_healthy));
    let net_card_healthy = cards_healthy
        .iter()
        .find(|c| c.title == taskmanager_application::i18n::t("proc_insights.network_throughput"))
        .unwrap();
    assert_eq!(net_card_healthy.action, None);
}
