//! Container and sandbox runtime metadata and cgroup signatures.

use serde::{Deserialize, Serialize};

/// Known container and application sandbox runtimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerRuntime {
    Docker,
    Podman,
    Containerd,
    Crio,
    Flatpak,
    Snap,
    Other,
}

impl ContainerRuntime {
    #[must_use]
    pub const fn display_label(&self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Podman => "Podman",
            Self::Containerd => "containerd",
            Self::Crio => "CRI-O",
            Self::Flatpak => "Flatpak",
            Self::Snap => "Snap",
            Self::Other => "Container",
        }
    }
}

/// Discovered container metadata associated with a process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContainerMetadata {
    pub runtime: ContainerRuntime,
    pub container_id: String,
    pub name: Option<String>,
}

impl ContainerMetadata {
    #[must_use]
    pub fn new(runtime: ContainerRuntime, container_id: impl Into<String>) -> Self {
        Self {
            runtime,
            container_id: container_id.into(),
            name: None,
        }
    }

    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn display_badge(&self) -> String {
        let label = self.runtime.display_label();
        if let Some(ref name) = self.name {
            format!("[{label}: {name}]")
        } else if self.container_id.len() >= 12 {
            format!("[{label}: {}]", &self.container_id[..12])
        } else if !self.container_id.is_empty() {
            format!("[{label}: {}]", self.container_id)
        } else {
            format!("[{label}]")
        }
    }
}

/// Parse Linux cgroup content (v1 or v2) to identify if the process runs inside a container.
#[must_use]
pub fn parse_cgroup_container_metadata(content: &str) -> Option<ContainerMetadata> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Format: hierarchy:controllers:path (v1) or 0::path (v2)
        let path = trimmed.splitn(3, ':').nth(2).unwrap_or(trimmed);

        if let Some(idx) = path.find("docker-") {
            let rest = &path[idx + 7..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Docker, id));
            }
        } else if let Some(idx) = path.find("/docker/") {
            let rest = &path[idx + 8..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Docker, id));
            }
        } else if let Some(idx) = path.find("libpod-") {
            let rest = &path[idx + 7..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Podman, id));
            }
        } else if let Some(idx) = path.find("/libpod/") {
            let rest = &path[idx + 8..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Podman, id));
            }
        } else if let Some(idx) = path.find("crio-") {
            let rest = &path[idx + 5..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Crio, id));
            }
        } else if let Some(idx) = path.find("cri-containerd-") {
            let rest = &path[idx + 15..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Containerd, id));
            }
        } else if let Some(idx) = path.find("containerd-") {
            let rest = &path[idx + 11..];
            let id = extract_id(rest);
            if !id.is_empty() {
                return Some(ContainerMetadata::new(ContainerRuntime::Containerd, id));
            }
        } else if let Some(idx) = path.find("app-flatpak-") {
            let rest = &path[idx + 12..];
            let name = extract_flatpak_app_id(rest);
            if !name.is_empty() {
                return Some(
                    ContainerMetadata::new(ContainerRuntime::Flatpak, name.clone()).with_name(name),
                );
            }
        } else if let Some(idx) = path.find("snap.") {
            let rest = &path[idx + 5..];
            let snap_name = rest.split('.').next().unwrap_or(rest);
            if !snap_name.is_empty() {
                return Some(
                    ContainerMetadata::new(ContainerRuntime::Snap, snap_name).with_name(snap_name),
                );
            }
        }
    }
    None
}

fn extract_id(s: &str) -> String {
    let clean = s.strip_suffix(".scope").unwrap_or(s);
    let id: String = clean
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if id.len() >= 12 {
        id[..12].to_string()
    } else {
        id
    }
}

fn extract_flatpak_app_id(s: &str) -> String {
    let clean = s.strip_suffix(".scope").unwrap_or(s);
    if let Some((app_id, _pid)) = clean.rsplit_once('-') {
        app_id.to_string()
    } else {
        clean.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cgroup_v2_docker() {
        let cgroup = "0::/system.slice/docker-a1b2c3d4e5f678901234567890abcdef.scope";
        let meta = parse_cgroup_container_metadata(cgroup).expect("should parse docker");
        assert_eq!(meta.runtime, ContainerRuntime::Docker);
        assert_eq!(meta.container_id, "a1b2c3d4e5f6");
        assert_eq!(meta.display_badge(), "[Docker: a1b2c3d4e5f6]");
    }

    #[test]
    fn test_parse_cgroup_v2_podman() {
        let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/libpod-fedcba0987654321fedcba0987654321.scope";
        let meta = parse_cgroup_container_metadata(cgroup).expect("should parse podman");
        assert_eq!(meta.runtime, ContainerRuntime::Podman);
        assert_eq!(meta.container_id, "fedcba098765");
        assert_eq!(meta.display_badge(), "[Podman: fedcba098765]");
    }

    #[test]
    fn test_parse_cgroup_v2_crio() {
        let cgroup = "0::/kubepods.slice/kubepods-burstable.slice/crio-1122334455667788.scope";
        let meta = parse_cgroup_container_metadata(cgroup).expect("should parse crio");
        assert_eq!(meta.runtime, ContainerRuntime::Crio);
        assert_eq!(meta.container_id, "112233445566");
    }

    #[test]
    fn test_parse_cgroup_v2_flatpak() {
        let cgroup = "0::/app.slice/app-flatpak-org.mozilla.firefox-12345.scope";
        let meta = parse_cgroup_container_metadata(cgroup).expect("should parse flatpak");
        assert_eq!(meta.runtime, ContainerRuntime::Flatpak);
        assert_eq!(meta.name.as_deref(), Some("org.mozilla.firefox"));
        assert_eq!(meta.display_badge(), "[Flatpak: org.mozilla.firefox]");
    }

    #[test]
    fn test_parse_cgroup_v2_snap() {
        let cgroup = "0::/user.slice/user-1000.slice/snap.discord.discord-1234.scope";
        let meta = parse_cgroup_container_metadata(cgroup).expect("should parse snap");
        assert_eq!(meta.runtime, ContainerRuntime::Snap);
        assert_eq!(meta.name.as_deref(), Some("discord"));
    }

    #[test]
    fn test_parse_cgroup_host_process_returns_none() {
        let cgroup =
            "0::/user.slice/user-1000.slice/user@1000.service/session.slice/session-2.scope";
        assert_eq!(parse_cgroup_container_metadata(cgroup), None);
    }
}
