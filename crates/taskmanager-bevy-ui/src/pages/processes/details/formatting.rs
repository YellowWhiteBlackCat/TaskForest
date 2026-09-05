//! Formatter helpers for Bevy Process Insights cards.

use taskmanager_application::{i18n::t, project_process_resources};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport, IsolationKind, LimitValue,
    ProcessEnvironment, ProcessGpuSnapshot, ProcessIsolation, ProcessNetworkSnapshot,
    ProcessOpenFiles, ProcessResourceSnapshot, ProcessThreadInfo, ProcessThreads,
};
use taskmanager_shell::presentation::{MISSING_VALUE, bytes};

pub(crate) fn threads_summary(threads: &ProcessThreads) -> String {
    if threads.threads.is_empty() {
        return t("proc_insights.no_threads").to_owned();
    }
    let mut lines = vec![threads.threads.len().to_string()];
    for thread in threads.threads.iter().take(3) {
        lines.push(format_thread_row(thread));
    }
    if threads.threads.len() > 3 {
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
    format!(
        "{}  {}  {}  {}  {}",
        thread.tid,
        comm,
        thread.state.as_short_label(),
        cpu_time,
        cpu_percent
    )
}

pub(crate) fn open_files_summary(files: &ProcessOpenFiles) -> String {
    if files.entries.is_empty() && files.unreadable_count == 0 {
        return t("proc_insights.no_open_files").to_owned();
    }
    let header = if files.unreadable_count == 0 {
        files.entries.len().to_string()
    } else {
        format!(
            "{} · {} {}",
            files.entries.len(),
            files.unreadable_count,
            t("proc_insights.unreadable")
        )
    };
    if files.entries.is_empty() {
        return header;
    }
    let mut lines = vec![header];
    for entry in files.entries.iter().take(3) {
        let target = entry
            .target
            .as_deref()
            .unwrap_or_else(|| t("proc_insights.unreadable"));
        lines.push(format!("{} -> {}", entry.fd, target));
    }
    if files.entries.len() > 3 {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}

pub(crate) fn network_summary(network: &ProcessNetworkSnapshot) -> String {
    let rx = network.rx_bytes_per_sec.map_or_else(
        || MISSING_VALUE.to_owned(),
        |value| format!("{}/s", bytes(value)),
    );
    let tx = network.tx_bytes_per_sec.map_or_else(
        || MISSING_VALUE.to_owned(),
        |value| format!("{}/s", bytes(value)),
    );
    let mut lines = vec![format!("{} · RX {rx} · TX {tx}", network.connections.len())];

    for connection in network.connections.iter().take(3) {
        lines.push(format!(
            "{} {} -> {}",
            format_transport(&connection.transport, &connection.family),
            format_endpoint(&connection.local),
            format_endpoint(&connection.remote),
        ));
    }
    if network.connections.len() > 3 {
        lines.push("…".to_owned());
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

pub(crate) fn gpu_summary(gpu: &ProcessGpuSnapshot) -> String {
    let devices = gpu.devices.len();
    let engines = gpu.engines.engines.len();
    if devices == 0 && engines == 0 {
        return t("proc_insights.no_gpu").to_owned();
    }
    let header = format!("{devices} · {engines} {}", t("proc_insights.gpu_engines"));
    let mut lines = vec![header];

    for device in gpu.devices.iter().take(2) {
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
    if gpu.devices.len() > 2 {
        lines.push("…".to_owned());
    }

    for engine in gpu.engines.engines.iter().take(3) {
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
    if gpu.engines.engines.len() > 3 {
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
    let projection = project_process_resources(resources);
    let memory = match (projection.memory_usage_bytes, projection.memory_limit) {
        (Some(used), Some(LimitValue::Value(limit))) => {
            Some(format!("{} / {}", bytes(used), bytes(limit)))
        }
        (Some(used), Some(LimitValue::Unlimited)) => Some(format!("{} / ∞", bytes(used))),
        (Some(used), None) => Some(bytes(used)),
        (None, Some(LimitValue::Value(limit))) => Some(format!("— / {}", bytes(limit))),
        (None, Some(LimitValue::Unlimited)) => Some("— / ∞".to_owned()),
        (None, None) => None,
    };
    let cpu_quota = match (
        projection.cpu_time_quota_micros,
        projection.cpu_time_period_micros,
    ) {
        (Some(LimitValue::Unlimited), _) => Some("CPU ∞".to_owned()),
        (Some(LimitValue::Value(quota)), Some(period)) if period > 0 => Some(format!(
            "CPU {:.0}%",
            (quota as f64 / period as f64) * 100.0
        )),
        (Some(LimitValue::Value(quota)), _) => Some(format!("CPU {quota}µs")),
        (None, _) => None,
    };
    let pids = match (projection.process_count, projection.process_limit) {
        (Some(count), Some(LimitValue::Value(limit))) => {
            Some(format!("{count} / {limit} {}", t("proc_insights.pids")))
        }
        (Some(count), Some(LimitValue::Unlimited)) => {
            Some(format!("{count} / ∞ {}", t("proc_insights.pids")))
        }
        (Some(count), None) => Some(format!("{count} {}", t("proc_insights.pids"))),
        (None, Some(LimitValue::Value(limit))) => {
            Some(format!("— / {limit} {}", t("proc_insights.pids")))
        }
        (None, Some(LimitValue::Unlimited)) => Some(format!("— / ∞ {}", t("proc_insights.pids"))),
        (None, None) => None,
    };
    let resource_group = projection.resource_group.map(ToOwned::to_owned);

    let mut parts = Vec::new();
    if let Some(mem) = memory {
        parts.push(mem);
    }
    if let Some(cpu) = cpu_quota {
        parts.push(cpu);
    }
    if let Some(p) = pids {
        parts.push(p);
    }
    if let Some(group) = resource_group {
        parts.push(group);
    }

    if parts.is_empty() {
        MISSING_VALUE.to_owned()
    } else {
        parts.join(" · ")
    }
}

pub(crate) fn isolation_summary(isolation: &ProcessIsolation) -> String {
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
    let base = match &isolation.container_id {
        Some(id) if !id.is_empty() => format!("{kind} · {id}"),
        _ => kind.to_owned(),
    };
    match isolation.sandboxed {
        Some(true) => format!("{base} · {}", t("proc_insights.sandboxed")),
        Some(false) => format!("{base} · not sandboxed"),
        None => base,
    }
}

pub(crate) fn environment_summary(environment: &ProcessEnvironment) -> String {
    if environment.entries.is_empty() {
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
    for entry in environment.entries.iter().take(3) {
        lines.push(format!("{}={}", entry.key, entry.value));
    }
    if environment.entries.len() > 3 || environment.truncated_count > 0 {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}
