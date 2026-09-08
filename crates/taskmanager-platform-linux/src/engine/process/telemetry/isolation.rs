//! Container and desktop-sandbox identification from process-owned facts.

use std::path::Path;

use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::{FailureKind, IsolationKind, ProcessCapabilities, ProcessIsolation};
use taskmanager_core::{
    LinuxNamespaceAudit, LinuxNamespaceKind, NamespaceAuditEntry, NamespaceAuditStatus,
    parse_namespace_inode,
};

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
    let status = std::fs::read_to_string(proc_dir.join("status"));
    let (seccomp_mode, no_new_privs) = status
        .as_deref()
        .map(parse_security_status)
        .unwrap_or((None, None));
    let child_subreaper = observe_child_subreaper(proc_dir);
    let capabilities = status
        .as_deref()
        .ok()
        .map(|text| parse_capability_status(text, now_ms));
    let namespaces = proc_dir
        .parent()
        .map(|proc_root| collect_namespace_audit(proc_dir, proc_root, now_ms));
    let root_uid_host = std::fs::read_to_string(proc_dir.join("uid_map"))
        .ok()
        .and_then(|text| parse_root_host_uid(&text));
    let rootfs_read_only = std::fs::read_to_string(proc_dir.join("mountinfo"))
        .ok()
        .and_then(|text| parse_rootfs_read_only(&text));
    let sandbox_confinement = parse_snap_confinement(environment.as_deref().unwrap_or_default());
    let sandbox_permissions = std::fs::read_to_string(proc_dir.join("root/.flatpak-info"))
        .ok()
        .map(|text| parse_sandbox_permissions(&text))
        .unwrap_or_default();
    let security_profile = std::fs::read_to_string(proc_dir.join("attr/current"))
        .ok()
        .map(|profile| profile.trim().to_owned())
        .filter(|profile| !profile.is_empty());
    let yama_ptrace_scope = proc_dir
        .parent()
        .and_then(|proc_root| {
            std::fs::read_to_string(proc_root.join("sys/kernel/yama/ptrace_scope")).ok()
        })
        .and_then(|value| value.trim().parse::<u8>().ok());
    let flatpak_marker = proc_dir.join("root/.flatpak-info").exists();
    let cgroup = cgroup.ok();
    let environment = environment.ok();
    let detected = detect_isolation(
        cgroup.as_deref().unwrap_or_default(),
        environment.as_deref().unwrap_or_default(),
        flatpak_marker,
    );
    let sources_readable = cgroup.is_some()
        || environment.is_some()
        || flatpak_marker
        || status.is_ok()
        || security_profile.is_some()
        || yama_ptrace_scope.is_some();
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
        seccomp_mode,
        no_new_privs,
        child_subreaper,
        security_profile,
        yama_ptrace_scope,
        capabilities,
        namespaces,
        root_uid_host,
        rootfs_read_only,
        sandbox_confinement,
        sandbox_permissions,
    }
}

/// `PR_GET_CHILD_SUBREAPER` is a per-calling-process query; it cannot inspect
/// an arbitrary target PID. Report the real value for this process only and
/// keep every other process explicitly unknown.
fn observe_child_subreaper(proc_dir: &Path) -> Option<bool> {
    let pid = proc_dir.file_name()?.to_str()?.parse::<u32>().ok()?;
    (pid == std::process::id())
        .then(|| nix::sys::prctl::get_child_subreaper().ok())
        .flatten()
}

/// Read all seven namespace symlinks for `proc_dir` and compare them to the
/// corresponding host PID 1 entries. Each class is independent: one denied
/// or racing read becomes an `Unavailable` row while successful comparisons
/// remain useful to the UI.
#[must_use]
pub fn collect_namespace_audit(
    proc_dir: &Path,
    proc_root: &Path,
    now_ms: u64,
) -> LinuxNamespaceAudit {
    let host_dir = proc_root.join("1/ns");
    let mut entries = Vec::with_capacity(LinuxNamespaceKind::ALL.len());
    let mut readable = 0_usize;
    let mut failure = None;
    for kind in LinuxNamespaceKind::ALL {
        let process_result = std::fs::read_link(proc_dir.join("ns").join(kind.proc_name()));
        let host_result = std::fs::read_link(host_dir.join(kind.proc_name()));
        let status = match (process_result, host_result) {
            (Ok(process), Ok(host)) => match (
                parse_namespace_inode(&process.to_string_lossy()),
                parse_namespace_inode(&host.to_string_lossy()),
            ) {
                (Some(inode), Some(host_inode)) => {
                    readable += 1;
                    if inode == host_inode {
                        NamespaceAuditStatus::Host { inode }
                    } else {
                        NamespaceAuditStatus::Isolated { inode, host_inode }
                    }
                }
                _ => NamespaceAuditStatus::Unavailable(FailureKind::ProviderFault),
            },
            (Err(process), _) | (_, Err(process)) => {
                let candidate = match process.kind() {
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::Unsupported => {
                        FailureKind::Unsupported
                    }
                    std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
                    std::io::ErrorKind::InvalidData => FailureKind::ProviderFault,
                    std::io::ErrorKind::TimedOut => FailureKind::TimedOut,
                    _ => FailureKind::TemporarilyUnavailable,
                };
                failure = Some(failure.map_or(candidate, |current| {
                    if failure_priority(current) >= failure_priority(candidate) {
                        current
                    } else {
                        candidate
                    }
                }));
                NamespaceAuditStatus::Unavailable(candidate)
            }
        };
        entries.push(NamespaceAuditEntry { kind, status });
    }
    let status = if readable > 0 {
        DeviceStatus::Healthy
    } else {
        match failure.unwrap_or(FailureKind::TemporarilyUnavailable) {
            FailureKind::PermissionDenied => DeviceStatus::PermissionDenied,
            _ => DeviceStatus::Stale,
        }
    };
    let state = state_for_status(status, now_ms);
    LinuxNamespaceAudit::from_entries(state, entries)
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 8,
        FailureKind::PermissionDenied => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::TimedOut => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

/// Parse the first uid namespace mapping. A mapping that does not include
/// namespace UID 0 is not a useful root identity claim and returns `None`.
#[must_use]
pub fn parse_root_host_uid(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let namespace_uid = fields.next()?.parse::<u64>().ok()?;
        let host_uid = fields.next()?.parse::<u64>().ok()?;
        let range = fields.next()?.parse::<u64>().ok()?;
        if namespace_uid == 0 && range > 0 {
            u32::try_from(host_uid).ok()
        } else {
            None
        }
    })
}

/// Read the root mount's explicit `ro`/`rw` flag from Linux mountinfo. The
/// parser intentionally returns `None` when no root mount or explicit flag is
/// present; a missing flag is not permission to claim read-write safety.
#[must_use]
pub fn parse_rootfs_read_only(text: &str) -> Option<bool> {
    text.lines().find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let mount_point = fields.get(4)?.replace("\\040", " ");
        if mount_point != "/" {
            return None;
        }
        fields.get(5)?.split(',').find_map(|option| match option {
            "ro" => Some(true),
            "rw" => Some(false),
            _ => None,
        })
    })
}

/// Extract Snap's explicit confinement mode from NUL-separated process
/// environment. Flatpak and other sandboxes leave this absent rather than
/// being mislabelled as Snap.
#[must_use]
pub fn parse_snap_confinement(environment: &[u8]) -> Option<String> {
    environment
        .split(|byte| *byte == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .find_map(|entry| {
            let value = entry.strip_prefix("SNAP_CONFINEMENT=")?.trim();
            (!value.is_empty()).then(|| value.to_owned())
        })
}

/// Parse bounded permission-like keys from the Flatpak info key file. This is
/// deliberately a declaration summary, not an attempt to infer effective
/// kernel policy.
#[must_use]
pub fn parse_sandbox_permissions(text: &str) -> Vec<String> {
    let mut permissions = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !matches!(
            key.trim(),
            "shared" | "sockets" | "filesystems" | "persistent"
        ) {
            continue;
        }
        for item in value.split([';', ',']) {
            let item = item.trim();
            if !item.is_empty() && permissions.len() < 32 && !permissions.iter().any(|v| v == item)
            {
                permissions.push(item.to_owned());
            }
        }
    }
    permissions
}

/// Parse the five Linux capability masks from `/proc/<pid>/status`.
///
/// Hex masks are independent facts: a malformed or absent line stays `None`
/// for that mask while other valid masks remain usable. The caller supplies
/// the already-established procfs state so capability readability cannot
/// silently promote a denied process to healthy.
#[must_use]
pub fn parse_capability_status(text: &str, observed_at_ms: u64) -> ProcessCapabilities {
    let mut masks = [None; 5];
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let index = match key.trim() {
            "CapInh" => 0,
            "CapPrm" => 1,
            "CapEff" => 2,
            "CapBnd" => 3,
            "CapAmb" => 4,
            _ => continue,
        };
        masks[index] = u64::from_str_radix(value.trim(), 16).ok();
    }
    ProcessCapabilities::from_masks(
        super::state_for_status(DeviceStatus::Healthy, observed_at_ms),
        masks[0],
        masks[1],
        masks[2],
        masks[3],
        masks[4],
    )
}

/// Parse the security fields from a Linux `/proc/<pid>/status` fixture.
/// Unknown or malformed values remain unavailable instead of becoming a
/// permissive `false`/`0` claim.
#[must_use]
pub fn parse_security_status(text: &str) -> (Option<u8>, Option<bool>) {
    let mut seccomp = None;
    let mut no_new_privs = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Seccomp:") {
            seccomp = value.trim().parse::<u8>().ok().filter(|mode| *mode <= 2);
        } else if let Some(value) = line.strip_prefix("NoNewPrivs:") {
            no_new_privs = match value.trim() {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            };
        }
    }
    (seccomp, no_new_privs)
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
