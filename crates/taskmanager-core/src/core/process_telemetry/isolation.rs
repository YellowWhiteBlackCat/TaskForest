use serde::{Deserialize, Serialize};

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
}
