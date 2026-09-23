//! Formatter helpers for Bevy Process Insights cards.

use taskmanager_application::process_details_vm::render_environment_variable;
use taskmanager_application::{i18n::t, project_process_resources};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process_telemetry::ThreadWaitKind;
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport, IsolationKind, LimitValue,
    ProcessEnvironment, ProcessGpuSnapshot, ProcessIsolation, ProcessNetworkSnapshot,
    ProcessOpenFiles, ProcessResourceSnapshot, ProcessThreadInfo, ProcessThreads,
};
use taskmanager_shell::presentation::capabilities_summary;
use taskmanager_shell::presentation::namespaces_summary;
use taskmanager_shell::presentation::network_connection_counters_summary;
use taskmanager_shell::presentation::sandbox_details_summary;
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
        lines.push(format!(
            "{} [{}] -> {}",
            entry.fd,
            entry.resolved_kind(),
            target
        ));
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
    if network.connections.len() > 3 {
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
    let memory = projection
        .memory_limit
        .and_then(|limit| limit.usage_percent(projection.memory_usage_bytes))
        .map_or(memory.clone(), |percent| {
            memory.map(|value| format!("{value} ({percent:.0}%)"))
        });
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
    let pids = projection
        .process_limit
        .and_then(|limit| limit.usage_percent(projection.process_count))
        .map_or(pids.clone(), |percent| {
            pids.map(|value| format!("{value} ({percent:.0}%)"))
        });
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
    let base = match isolation.sandboxed {
        Some(true) => format!("{base} · {}", t("proc_insights.sandboxed")),
        Some(false) => format!("{base} · not sandboxed"),
        None => base,
    };
    let mut security = Vec::new();
    if let Some(profile) = isolation.security_profile.as_deref() {
        security.push(format!(
            "{}: {profile}",
            t("proc_insights.security_profile")
        ));
    }
    if let Some(mode) = isolation.seccomp_mode {
        security.push(format!("{}: {mode}", t("proc_insights.seccomp")));
    }
    if let Some(enabled) = isolation.no_new_privs {
        security.push(format!(
            "{}: {}",
            t("proc_insights.no_new_privs"),
            t(if enabled { "common.yes" } else { "common.no" })
        ));
    }
    if let Some(scope) = isolation.yama_ptrace_scope {
        security.push(format!("{}: {scope}", t("proc_insights.ptrace_scope")));
    }
    if let Some(capabilities) = isolation.capabilities.as_ref() {
        security.push(format!(
            "{}: {}",
            t("proc_insights.capabilities"),
            capabilities_summary(capabilities),
        ));
    }
    if let Some(namespaces) = isolation.namespaces.as_ref() {
        security.push(format!(
            "{}: {}",
            t("proc_insights.namespaces"),
            namespaces_summary(namespaces),
        ));
    }
    if let Some(details) = sandbox_details_summary(isolation) {
        security.push(format!("{}: {details}", t("proc_insights.sandbox_details")));
    }
    if security.is_empty() {
        base
    } else {
        format!("{base} · {}", security.join(" · "))
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
        lines.push(render_environment_variable(&entry.key, &entry.value));
    }
    if environment.entries.len() > 3 || environment.truncated_count > 0 {
        lines.push("…".to_owned());
    }
    lines.join("\n")
}
