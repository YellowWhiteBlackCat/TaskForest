//! Formatter helpers for Bevy Process Insights cards.

use taskmanager_application::process_details_vm::render_environment_variable;
use taskmanager_application::{ProjectedProcessResources, i18n::t};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::ThreadWaitKind;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport, IsolationKind,
    ProcessEnvironment, ProcessGpuSnapshot, ProcessIsolation, ProcessNetworkSnapshot,
    ProcessOpenFiles, ProcessResourceSnapshot, ProcessThreadInfo, ProcessThreads,
};
use taskmanager_shell::presentation::namespaces_summary;
use taskmanager_shell::presentation::network_connection_counters_summary;
use taskmanager_shell::presentation::sandbox_details_summary;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes, format_open_files_saturation};
use taskmanager_shell::presentation::{capabilities_summary, process_resource_rows};

#[derive(Clone, Copy)]
pub(crate) enum InsightDetail {
    Summary,
    Complete,
}
impl InsightDetail {
    fn limit(self, summary: usize) -> usize {
        match self {
            Self::Summary => summary,
            Self::Complete => usize::MAX,
        }
    }
}

pub(crate) fn threads_summary(threads: &ProcessThreads, detail: InsightDetail) -> String {
    if threads.threads.is_empty() {
        return t("proc_insights.no_threads").to_owned();
    }
    let mut lines = vec![threads.threads.len().to_string()];
    for thread in threads.threads.iter().take(detail.limit(3)) {
        let mut row = format_thread_row(thread);
        if matches!(detail, InsightDetail::Complete)
            && let Some(wait) = &thread.wchan
        {
            row.push_str(&format!(" · {} {wait}", t("proc_insights.thread_wait")));
        }
        lines.push(row);
    }
    if threads.threads.len() > detail.limit(3) {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}

fn format_thread_row(thread: &ProcessThreadInfo) -> String {
    let comm = if thread.comm.is_empty() {
        MISSING_VALUE.to_owned()
    } else {
        thread.comm.clone()
    };
    let cpu_time = thread
        .cpu_time_secs
        .map_or_else(|| MISSING_VALUE.to_owned(), |v| format!("{v:.1}s"));
    let cpu_percent = thread
        .cpu_percent
        .map_or_else(|| MISSING_VALUE.to_owned(), |v| format!("{v:.1}%"));
    let mut line = format!(
        "{}  {}  {}  {}  {}",
        thread.tid,
        comm,
        thread.state.as_short_label(),
        cpu_time,
        cpu_percent
    );
    if let Some(nanos) = thread.run_queue_wait_ns {
        let kind = thread
            .wait_kind
            .map(ThreadWaitKind::as_str)
            .unwrap_or("wait");
        line.push_str(&format!("  {kind} {:.1}ms", nanos as f64 / 1_000_000.0));
    }
    line
}

pub(crate) fn open_files_summary(
    files: &ProcessOpenFiles,
    resources: Option<&ProjectedProcessResources>,
    detail: InsightDetail,
) -> String {
    if files.entries.is_empty() && files.unreadable_count == 0 {
        return t("proc_insights.no_open_files").to_owned();
    }
    let count = files.entries.len() as u64;
    let sat = resources.and_then(|r| {
        format_open_files_saturation(count, r.open_files_soft_limit, r.open_files_hard_limit, "∞")
    });
    let count_text = sat.unwrap_or_else(|| files.entries.len().to_string());
    let header = if files.unreadable_count == 0 {
        count_text
    } else {
        format!(
            "{} · {} {}",
            count_text,
            files.unreadable_count,
            t("proc_insights.unreadable")
        )
    };
    if files.entries.is_empty() {
        return header;
    }
    let mut lines = vec![header];
    for entry in files.entries.iter().take(detail.limit(3)) {
        let target = entry
            .target
            .as_deref()
            .unwrap_or_else(|| t("proc_insights.unreadable"));
        let deleted = if entry.deleted { " [deleted]" } else { "" };
        lines.push(format!(
            "{} [{}] -> {target}{deleted}",
            entry.fd,
            entry.resolved_kind(),
        ));
    }
    if files.entries.len() > detail.limit(3) {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}

pub(crate) fn network_summary(network: &ProcessNetworkSnapshot, detail: InsightDetail) -> String {
    let rx = network.rx_bytes_per_sec.map_or_else(
        || MISSING_VALUE.to_owned(),
        |value| format!("{}/s", bytes(value)),
    );
    let tx = network.tx_bytes_per_sec.map_or_else(
        || MISSING_VALUE.to_owned(),
        |value| format!("{}/s", bytes(value)),
    );
    let mut lines = vec![format!("{} · RX {rx} · TX {tx}", network.connections.len())];

    for connection in network.connections.iter().take(detail.limit(3)) {
        let rtt = connection
            .rtt_ms
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map_or_else(String::new, |value| {
                format!(" · {} {value:.1} ms", t("net.rtt"))
            });
        lines.push(format!(
            "{} [{} · {}] {} -> {}{}",
            format_transport(&connection.transport, &connection.family),
            connection.state,
            if connection.is_loopback() {
                "LOOPBACK"
            } else {
                "EXTERNAL"
            },
            format_endpoint(&connection.local),
            format_endpoint(&connection.remote),
            rtt,
        ));
    }
    if network.connections.len() > detail.limit(3) {
        lines.push("…".to_owned());
    }
    if let Some(counters) = network.connection_counters.as_ref()
        && let Some(summary) = network_connection_counters_summary(counters)
    {
        lines.push(summary);
    }
    if network.traffic_failure == Some(FailureKind::RequiresEscalation) {
        lines.push(format!(
            "{} ({})",
            t("proc_insights.network_requires_escalation"),
            t("proc_insights.enable_network_capture")
        ));
    }

    lines.join("\n")
}

fn format_transport(transport: &ConnectionTransport, family: &ConnectionAddressFamily) -> String {
    match (transport, family) {
        (ConnectionTransport::Tcp, ConnectionAddressFamily::Ipv6) => "TCP6".to_owned(),
        (ConnectionTransport::Udp, ConnectionAddressFamily::Ipv6) => "UDP6".to_owned(),
        (ConnectionTransport::Tcp, _) => "TCP".to_owned(),
        (ConnectionTransport::Udp, _) => "UDP".to_owned(),
        (ConnectionTransport::Sctp, _) => "SCTP".to_owned(),
        (ConnectionTransport::Local, _) => "UNIX".to_owned(),
        (ConnectionTransport::Other(s), _) => s.clone(),
    }
}

fn format_endpoint(endpoint: &ConnectionEndpoint) -> String {
    match endpoint {
        ConnectionEndpoint::Ip(address) => address.to_string(),
        ConnectionEndpoint::Local { path } => path.clone(),
        ConnectionEndpoint::Opaque { value } => value.clone(),
        ConnectionEndpoint::Unspecified => MISSING_VALUE.to_owned(),
    }
}

pub(crate) fn gpu_summary(gpu: &ProcessGpuSnapshot, detail: InsightDetail) -> String {
    let devices = gpu.devices.len();
    let engines = gpu.engines.engines.len();
    if devices == 0 && engines == 0 {
        return t("proc_insights.no_gpu").to_owned();
    }
    let header = format!("{devices} · {engines} {}", t("proc_insights.gpu_engines"));
    let mut lines = vec![header];

    for device in gpu.devices.iter().take(detail.limit(2)) {
        let util = device
            .utilization_pct
            .map_or_else(|| MISSING_VALUE.to_owned(), |v| format!("{v:.1}%"));
        let vram = device
            .memory_bytes
            .map_or_else(|| MISSING_VALUE.to_owned(), bytes);
        lines.push(format!(
            "{} #{} {} · {} {}",
            t("common.gpu"),
            device.device_id,
            util,
            t("gpu.vram_in_use"),
            vram
        ));
    }
    if gpu.devices.len() > detail.limit(2) {
        lines.push("…".to_owned());
    }

    for engine in gpu.engines.engines.iter().take(detail.limit(3)) {
        let usage = engine
            .usage_pct
            .current_value()
            .map_or_else(|| MISSING_VALUE.to_owned(), |v| format!("{v:.1}%"));
        let cumulative = engine
            .engine_time_ns
            .current_value()
            .map(|ns| format_engine_time(*ns))
            .or_else(|| {
                engine
                    .engine_cycles
                    .current_value()
                    .map(|cycles| format_engine_cycles(*cycles))
            })
            .unwrap_or_else(|| MISSING_VALUE.to_owned());
        lines.push(format!("{}  {}  {}", engine.name, usage, cumulative));
    }
    if gpu.engines.engines.len() > detail.limit(3) {
        lines.push("…".to_owned());
    }

    lines.join("\n")
}

fn format_engine_time(nanoseconds: u64) -> String {
    let seconds = nanoseconds as f64 / 1_000_000_000.0;
    format!("{seconds:.1}s")
}

fn format_engine_cycles(cycles: u64) -> String {
    if cycles >= 1_000_000_000 {
        format!("{:.2}G cycles", cycles as f64 / 1_000_000_000.0)
    } else if cycles >= 1_000_000 {
        format!("{:.1}M cycles", cycles as f64 / 1_000_000.0)
    } else {
        format!("{cycles} cycles")
    }
}

pub(crate) fn resources_summary(resources: &ProcessResourceSnapshot) -> String {
    let values = process_resource_rows(resources)
        .into_iter()
        .map(|(_, value)| value)
        .filter(|value| value != MISSING_VALUE)
        .collect::<Vec<_>>();
    if values.is_empty() {
        MISSING_VALUE.to_owned()
    } else {
        values.join(" · ")
    }
}

pub(crate) fn isolation_rows(isolation: &ProcessIsolation) -> Vec<(String, String)> {
    let kind = match isolation.kind {
        Some(IsolationKind::Docker) => "Docker",
        Some(IsolationKind::Podman) => "Podman",
        Some(IsolationKind::Kubernetes) => "Kubernetes",
        Some(IsolationKind::Lxc) => "LXC",
        Some(IsolationKind::SystemdNspawn) => "systemd-nspawn",
        Some(IsolationKind::Flatpak) => "Flatpak",
        Some(IsolationKind::Snap) => "Snap",
        Some(IsolationKind::Wsl) => "WSL",
        Some(IsolationKind::OtherContainer) => "Container",
        None => t("proc_insights.host_process"),
    };
    let boolean = |value: Option<bool>| {
        value.map(|value| t(if value { "common.yes" } else { "common.no" }).to_owned())
    };
    let mut rows: Vec<(String, String)> = [
        ("proc_insights.isolation", Some(kind.to_owned())),
        ("proc_insights.container_id", isolation.container_id.clone()),
        ("proc_insights.sandboxed", boolean(isolation.sandboxed)),
        (
            "proc_insights.security_profile",
            isolation.security_profile.clone(),
        ),
        (
            "proc_insights.seccomp",
            isolation.seccomp_mode.map(|mode| mode.to_string()),
        ),
        (
            "proc_insights.no_new_privs",
            boolean(isolation.no_new_privs),
        ),
        (
            "proc_insights.ptrace_scope",
            isolation.yama_ptrace_scope.map(|scope| scope.to_string()),
        ),
        (
            "proc_insights.capabilities",
            isolation.capabilities.as_ref().map(capabilities_summary),
        ),
        (
            "proc_insights.namespaces",
            isolation.namespaces.as_ref().map(namespaces_summary),
        ),
    ]
    .into_iter()
    .map(|(label, value)| {
        (
            t(label).to_owned(),
            value
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| MISSING_VALUE.to_owned()),
        )
    })
    .collect();
    if let Some(details) = sandbox_details_summary(isolation) {
        rows.push((t("proc_insights.sandbox_details").to_owned(), details));
    }
    rows
}

pub(crate) fn isolation_summary(isolation: &ProcessIsolation) -> String {
    isolation_rows(isolation)
        .into_iter()
        .enumerate()
        .filter(|(_, (_, value))| value != MISSING_VALUE)
        .map(|(index, (label, value))| match index {
            0 | 1 => value,
            2 => {
                if isolation.sandboxed == Some(true) {
                    label
                } else {
                    "not sandboxed".to_owned()
                }
            }
            _ => format!("{label}: {value}"),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(crate) fn environment_summary(
    environment: &ProcessEnvironment,
    detail: InsightDetail,
) -> String {
    if environment.entries.is_empty() && environment.working_directory.is_none() {
        return t("prop.environment_empty").to_owned();
    }
    let header = if environment.truncated_count == 0 {
        environment.entries.len().to_string()
    } else {
        format!(
            "{} · +{}",
            environment.entries.len(),
            environment.truncated_count
        )
    };
    let mut lines = vec![header];
    if matches!(detail, InsightDetail::Complete)
        && let Some(directory) = &environment.working_directory
    {
        lines.push(format!(
            "{}: {}",
            t("prop.working_directory"),
            directory.display()
        ));
    }
    for entry in environment.entries.iter().take(detail.limit(3)) {
        lines.push(render_environment_variable(&entry.key, &entry.value));
    }
    if environment.entries.len() > detail.limit(3) || environment.truncated_count > 0 {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}
