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
}
