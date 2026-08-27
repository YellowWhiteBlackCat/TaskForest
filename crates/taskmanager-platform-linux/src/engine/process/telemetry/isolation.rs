//! Container and desktop-sandbox identification from process-owned facts.

use std::path::Path;

use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::{IsolationKind, ProcessIsolation};

use super::state_for_status;

/// Isolation owns its own cgroup/environment reads so resource observation
/// failures or cgroupfs latency cannot serialize this domain.
pub(super) fn collect_independent_from_proc_dir(proc_dir: &Path, now_ms: u64) -> ProcessIsolation {
    let cgroup = std::fs::read_to_string(proc_dir.join("cgroup"));
    let cgroup_denied =
        matches!(&cgroup, Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied);
    let environment = std::fs::read(proc_dir.join("environ"));
    let environment_denied =
        matches!(&environment, Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied);
    let flatpak_marker = proc_dir.join("root/.flatpak-info").exists();
    let cgroup = cgroup.ok();
    let environment = environment.ok();
    let detected = detect_isolation(
        cgroup.as_deref().unwrap_or_default(),
        environment.as_deref().unwrap_or_default(),
        flatpak_marker,
    );
    let sources_readable = cgroup.is_some() || environment.is_some() || flatpak_marker;
    ProcessIsolation {
        state: state_for_status(
            if sources_readable {
                DeviceStatus::Healthy
            } else if cgroup_denied || environment_denied {
                DeviceStatus::PermissionDenied
            } else {
                DeviceStatus::Stale
            },
            now_ms,
        ),
        sandboxed: sources_readable.then_some(detected.0.is_some()),
        kind: detected.0,
        container_id: detected.1,
    }
}

pub fn detect_isolation(
    cgroup_text: &str,
    environment: &[u8],
    flatpak_marker: bool,
) -> (Option<IsolationKind>, Option<String>) {
    let env = environment
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .collect::<Vec<_>>();
    if flatpak_marker
        || env
            .iter()
            .any(|entry| entry.starts_with("FLATPAK_ID=") || *entry == "container=flatpak")
    {
        return (Some(IsolationKind::Flatpak), None);
    }
    if env
        .iter()
        .any(|entry| entry.starts_with("SNAP=") || entry.starts_with("SNAP_NAME="))
    {
        return (Some(IsolationKind::Snap), None);
    }
    let lower = cgroup_text.to_ascii_lowercase();
    let kind = if lower.contains("kubepods") {
        Some(IsolationKind::Kubernetes)
    } else if lower.contains("docker") {
        Some(IsolationKind::Docker)
    } else if lower.contains("libpod") || lower.contains("podman") {
        Some(IsolationKind::Podman)
    } else if lower.contains("lxc") {
        Some(IsolationKind::Lxc)
    } else if lower.contains("machine.slice/machine-") {
        Some(IsolationKind::SystemdNspawn)
    } else if env.iter().any(|entry| entry.starts_with("container=")) {
        Some(IsolationKind::OtherContainer)
    } else {
        None
    };
    let container_id = kind
        .as_ref()
        .and_then(|_| extract_container_id(cgroup_text));
    (kind, container_id)
}

fn extract_container_id(text: &str) -> Option<String> {
    text.split(|character: char| !character.is_ascii_hexdigit())
        .filter(|part| part.len() >= 12)
        .max_by_key(|part| part.len())
        .map(|part| part.chars().take(64).collect())
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_isolation_tests.rs"]
mod tests;
