//! Platform-neutral filesystem-health contracts.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemHealthStatus {
    Healthy,
    ReadOnly,
    ErrorsReported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemHealth {
    pub mount_point: PathBuf,
    pub source: Option<PathBuf>,
    pub fs_type: String,
    pub read_only: Option<bool>,
    pub error_count: Option<u64>,
    pub status: FilesystemHealthStatus,
    pub state: DeviceState,
    pub integrity_state: DeviceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FilesystemHealthSnapshot {
    pub state: DeviceState,
    pub filesystems: Vec<FilesystemHealth>,
}
