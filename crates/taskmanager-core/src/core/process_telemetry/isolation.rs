use serde::{Deserialize, Serialize};

use super::capabilities::ProcessCapabilities;
use super::namespaces::LinuxNamespaceAudit;
use crate::core::device_state::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationKind {
    Docker,
    Podman,
    Kubernetes,
    Lxc,
    SystemdNspawn,
    Flatpak,
    Snap,
    Wsl,
    OtherContainer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProcessIsolation {
    pub state: DeviceState,
    pub kind: Option<IsolationKind>,
    pub container_id: Option<String>,
    pub sandboxed: Option<bool>,
    /// Linux seccomp mode: 0 disabled, 1 strict, 2 filtered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seccomp_mode: Option<u8>,
    /// Linux `NoNewPrivs` hardening bit, when exposed by procfs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_new_privs: Option<bool>,
    /// Whether this process is a Linux child subreaper. Linux exposes this
    /// prctl flag only to the calling process; other PIDs remain `None`
    /// instead of being inferred from namespace shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_subreaper: Option<bool>,
    /// AppArmor profile or SELinux context from `/proc/<pid>/attr/current`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_profile: Option<String>,
    /// System Yama ptrace scope, when the host kernel exposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yama_ptrace_scope: Option<u8>,
    /// Linux capability masks, when `/proc/<pid>/status` exposes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ProcessCapabilities>,
    /// Seven Linux namespace inode comparisons against host PID 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<LinuxNamespaceAudit>,
    /// Host UID mapped to container UID 0, when the provider can read
    /// `uid_map` and the first mapping starts at namespace UID 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_uid_host: Option<u32>,
    /// Whether the process namespace's `/` mount is explicitly read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rootfs_read_only: Option<bool>,
    /// Snap confinement mode or an equivalent provider-reported sandbox mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_confinement: Option<String>,
    /// Bounded Flatpak/shared-sandbox permission declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sandbox_permissions: Vec<String>,
}

impl ProcessIsolation {
    /// Return `true` only when the provider proves the conjunction that makes
    /// a container escape warning actionable: isolated namespaces, a critical
    /// effective capability set, and a writable root filesystem. Unknown
    /// facets stay unknown and therefore cannot manufacture a warning.
    #[must_use]
    pub fn has_privilege_escape_risk(&self) -> bool {
        self.namespaces
            .as_ref()
            .is_some_and(super::namespaces::LinuxNamespaceAudit::is_container_like)
            && self.capabilities.as_ref().is_some_and(|capabilities| {
                capabilities.risk_level() == super::capabilities::CapabilityRiskLevel::Critical
            })
            && self.rootfs_read_only == Some(false)
    }
}
