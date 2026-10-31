//! Platform-neutral Mission Center first-run setup-script contracts.
//!
//! The descriptor is metadata for one fixed, audited asset. It is not an
//! arbitrary shell command container; native adapters decide how the typed
//! actions are executed and may return an honest unavailable/failure result.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupScriptInfo {
    pub path: PathBuf,
    pub run_command: String,
    pub revert_command: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupScriptAction {
    Observe,
    View,
    Run,
    Revert,
    Restart,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupScriptEvent {
    Observed(Option<SetupScriptInfo>),
    ActionCompleted { action: SetupScriptAction },
}
