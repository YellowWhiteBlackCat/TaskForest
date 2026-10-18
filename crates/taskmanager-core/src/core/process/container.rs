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
#[path = "../../../tests/headless/core_core_process_container_tests.rs"]
mod tests;
