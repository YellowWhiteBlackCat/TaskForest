//! Typed, background-only per-process Linux telemetry.
//!
//! The provider deliberately separates facts that procfs can prove (socket
//! ownership, DRM fdinfo counters, limits, cgroups and isolation markers) from
//! measurements that need privileged eBPF/vendor APIs. Unsupported values stay
//! `None`; callers must run collection off the render thread.

use std::path::{Path, PathBuf};

use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
#[cfg(feature = "test-support")]
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
    ProcessConnection, ProcessGpuDevice, ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity,
    ProcessNetworkSnapshot,
};
#[cfg(not(feature = "test-support"))]
use taskmanager_core::core::process_telemetry::{
    ConnectionAddressFamily, ConnectionEndpoint, ConnectionState, ConnectionTransport,
    ProcessConnection, ProcessGpuDevice, ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity,
    ProcessNetworkSnapshot,
};

pub mod containers;
pub mod environment;
mod facets;
pub mod gpu;
pub mod gpu_engines;
pub mod isolation;
// The AF_PACKET attribution join consumes the Linux-only boundary crate; the
// rest of this telemetry hub compiles on every target.
#[cfg(target_os = "linux")]
pub mod net_accounting;
pub mod network;
pub mod open_files;
pub mod resources;
pub mod threads;

pub use facets::{ProcessEnvironmentCollector, ProcessOpenFilesCollector, ProcessThreadsCollector};
pub(crate) use facets::{
    ProcessGpuCollector, ProcessIsolationCollector, ProcessNetworkCollector,
    ProcessResourcesCollector, SharedAccountingBackend,
};

#[cfg(not(feature = "test-support"))]
pub(crate) use gpu::ProcessGpuRateTracker;
#[cfg(feature = "test-support")]
pub use gpu::ProcessGpuRateTracker;
#[cfg(feature = "test-support")]
pub use network::{
    NetworkAccountingFailure, NetworkByteCounters, ProcessNetworkAccountingBackend,
    ProcessNetworkRateTracker,
};
#[cfg(not(feature = "test-support"))]
pub(crate) use network::{ProcessNetworkAccountingBackend, ProcessNetworkRateTracker};
pub use resources::{
    AuthorizedCgroupLimitPlan, CgroupCpuLimit, CgroupIoError, CgroupLimitApplyError,
    CgroupLimitConfirmation, CgroupLimitFile, CgroupLimitOperation, CgroupLimitPlan,
    CgroupLimitPlanError, CgroupLimitRequest, CgroupMembership, CgroupPlanIo,
    apply_cgroup_limit_plan, apply_cgroup_limit_plan_with, authorize_cgroup_limit_plan,
    parse_proc_cgroup, plan_cgroup_limits,
};

pub(crate) fn state_for_status(status: DeviceStatus, now_ms: u64) -> DeviceState {
    DeviceState::default().transition(status, now_ms)
}

pub(crate) fn status_from_io_error(error: &std::io::Error) -> DeviceStatus {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => DeviceStatus::PermissionDenied,
        std::io::ErrorKind::NotFound => DeviceStatus::Stale,
        _ => DeviceStatus::Stale,
    }
}

pub(crate) fn parse_start_time_ticks(text: &str) -> Option<u64> {
    let right_paren = text.rfind(')')?;
    let fields = text
        .get(right_paren + 1..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    fields.get(19)?.parse().ok()
}

pub(crate) fn safe_cgroup_path(root: &Path, path: &str) -> Option<PathBuf> {
    let relative = Path::new(path.trim_start_matches('/'));
    if relative
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(root.join(relative))
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_telemetry_tests.rs"]
mod tests;
