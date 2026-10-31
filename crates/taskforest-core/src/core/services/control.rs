//! Provider-neutral service lifecycle actions.

use serde::{Deserialize, Serialize};

/// A lifecycle action understood by every native service provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
    /// Ask the selected systemd manager to rescan unit files. The provider
    /// ignores the selected unit for the native command because this is a
    /// manager-wide `daemon-reload`, not a per-service lifecycle transition.
    ReloadDaemon,
}
