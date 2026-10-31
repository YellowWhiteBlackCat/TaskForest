use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

use super::connection::ProcessNetworkSnapshot;
use super::environment::ProcessEnvironment;
use super::gpu::ProcessGpuSnapshot;
use super::isolation::ProcessIsolation;
use super::open_files::ProcessOpenFiles;
use super::resources::ProcessResourceSnapshot;
use super::threads::ProcessThreads;

/// Stable process identity within one boot.
///
/// `start_token` is supplied by the platform provider (for example Linux
/// `/proc/<pid>/stat` start-time ticks). A PID alone is never sufficient
/// because it may be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_token: u64,
}

/// One independently collected process-insights domain plus the provider's
/// raw identity proof.
///
/// `ProcessIdentity::start_token` is provider-native and must only be compared
/// with other observations from the same provider family. It is deliberately
/// distinct from the application-facing frozen process identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInsightSnapshot<T> {
    pub identity: ProcessIdentity,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessTelemetrySnapshot {
    pub identity: ProcessIdentity,
    pub state: DeviceState,
    pub network: ProcessNetworkSnapshot,
    pub gpu: ProcessGpuSnapshot,
    pub resources: ProcessResourceSnapshot,
    pub isolation: ProcessIsolation,
    pub open_files: ProcessOpenFiles,
    pub threads: ProcessThreads,
    pub environment: ProcessEnvironment,
}
