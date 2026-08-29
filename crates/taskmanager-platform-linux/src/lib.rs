//! Non-blocking Linux implementations of the application layer's platform ports.
//!
//! Cross-platform provider traits have one public authority:
//! `taskmanager-platform-provider`. This adapter exports Linux composition
//! groups and runtime entry points, but deliberately does not re-export that
//! SPI under a Linux-specific path.

#![forbid(unsafe_code)]

// Keep direct builds of the native provider aligned with the application:
// release artifacts contain the full hardware registry and select a backend at
// runtime. `--no-default-features` remains available only for debug/test
// coverage of honest fallback behavior.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require `hardware-all`; hardware vendor backends are runtime-selected"
);

mod backend;
mod config;
mod engine;
mod local_time;
mod platform_handle;
mod provider;

pub use backend::{
    EnvironmentProviders, IntegrationProviders, LinuxProviderRegistry, PowerProviders,
    ProcessControlProviders, ProcessObservationProviders, ProcessProviders, SensorProviders,
    ServiceProviders, StorageProviders, SystemAuxiliaryProviders, SystemObservationProviders,
    SystemProviders,
};
pub use config::{user_config_path, user_history_dir};
pub use local_time::local_time_rules;

/// Return whether a process that previously owned a persistent-history lock
/// is no longer present in procfs.
///
/// The history store supplies this through a callback, so neither it nor the
/// application host needs to know the Linux process-filesystem layout.
#[must_use]
pub fn history_lock_holder_is_gone(pid: u32) -> bool {
    !std::path::Path::new(&format!("/proc/{pid}")).exists()
}
// Neutral system-tray seam implementation (ksni StatusNotifierItem, ADR-0NN);
// the frontends reach it through `taskmanager-platform-native::tray`.
pub mod tray;
// Single-instance adapter (D-Bus well-known name, borrowed core from
// tauri-plugin-single-instance; ADR-032 follow-up).
pub mod instance;
#[cfg(feature = "test-support")]
pub use engine::hardware::{
    classify_disk_type, classify_storage_connection, describe_disk_type, detect_cpu_cache,
    detect_gpu_metrics_from_paths, is_virtual_interface, parse_cpulist, parse_size_to_kb,
    physical_disk_key,
};
// Pure zram mm_stat parser for the fuzz workspace (fuzz/mm_stat target).
#[cfg(feature = "test-support")]
pub use engine::collector::parse_zram_mm_stat;
// The cgroup-v2 resource-limit write pipeline is plain cgroupfs I/O with no
// test-only types, so it ships in release builds: a shipped product can apply
// memory.max / cpu.max / pids.max, not just read them.
pub use engine::process::telemetry::{
    AuthorizedCgroupLimitPlan, CgroupCpuLimit, CgroupIoError, CgroupLimitApplyError,
    CgroupLimitConfirmation, CgroupLimitFile, CgroupLimitOperation, CgroupLimitPlan,
    CgroupLimitPlanError, CgroupLimitRequest, CgroupMembership, CgroupPlanIo,
    apply_cgroup_limit_plan, apply_cgroup_limit_plan_with, authorize_cgroup_limit_plan,
    plan_cgroup_limits,
};
// Per-process inspection data layer (lsof-style open files + per-thread
// breakdown). Plain /proc reads with no test-only types, so it ships in
// release builds alongside the cgroup pipeline. The collectors reuse the same
// start-token freeze + post-collection revalidation contract as the other
// facets; provider/runtime request-routing is wired separately.
pub use engine::process::telemetry::environment::collect_environment_from_proc_dir;
pub use engine::process::telemetry::open_files::{
    classify_open_file_target, collect_open_files_from_proc_dir,
};
pub use engine::process::telemetry::threads::{collect_threads_from_proc_dir, parse_thread_stat};
// Per-container aggregated CPU + memory rollup (cgroup-v2). Plain cgroupfs I/O
// with no test-only types, so it ships in release builds alongside the
// open-files/threads collectors. Reuses the runtime detection vocabulary from
// `isolation` so a container row and a per-process IsolationKind badge share one
// signature set.
pub use engine::process::telemetry::containers::{
    ContainerCpuRateTracker, ContainerRollupCollector, classify_container_cgroup,
    parse_cgroup_procs, parse_cpu_stat_usage_usec, parse_memory_current,
};
#[cfg(feature = "test-support")]
pub use engine::process::telemetry::{
    NetworkAccountingFailure, NetworkByteCounters, ProcessGpuRateTracker,
    ProcessNetworkAccountingBackend, ProcessNetworkRateTracker,
};
pub use engine::process::telemetry::{
    ProcessEnvironmentCollector, ProcessOpenFilesCollector, ProcessThreadsCollector,
};
#[cfg(feature = "test-support")]
pub use engine::process::{
    ProcIoFields, ProcStatFields, ProcessBatchSubmitError, ProcessBatchWorker, ProcessManager,
    kill_process, open_file_location, parent_dir, parse_proc_io, parse_proc_stat,
    parse_proc_status_memory, pause_process, read_exe_path, resume_process, terminate_process,
};
pub use engine::runtime_evidence::{
    EbpfObjectBuildIdentity, HardwareBuildProfile, InitRuntimeEvidence,
    LinuxProviderCapabilityReceipt, LinuxTargetEnvironmentCapabilityReceipt,
    ProviderProbeEligibility, ReceiptSource, collect_linux_provider_capability_receipt,
    linux_provider_capability_receipt_json,
};
#[cfg(feature = "test-support")]
pub use engine::sensors::trend::{collect_thermal_throttle, collect_thermal_throttle_from};
#[cfg(feature = "test-support")]
pub use engine::sensors::{collect_sensor_center, collect_sensor_center_from};
#[cfg(feature = "test-support")]
pub use engine::services::{
    InitSystem, ServiceLogCommandOutcome, ServiceLogStreamRequestError, ServiceLogStreamWorker,
    ServiceLogWorker, ServiceManager, classify_service_log_outcome, parse_openrc_description,
    parse_openrc_status, parse_openrc_update, parse_systemctl_show_deps, parse_unit_description,
};
#[cfg(feature = "test-support")]
pub use engine::session::{SessionManager, parse_loginctl_sessions};
#[cfg(feature = "test-support")]
pub use engine::smart::self_test::{
    SmartSelfTestPlan, parse_smart_self_test_json, read_smart_self_test_status,
    smart_self_test_plan, start_smart_self_test,
};
#[cfg(feature = "test-support")]
pub use engine::smart::{
    parse_smart_log_stdout, parse_smartctl_json, read_disk_smart, read_disk_smart_for_connection,
    read_nvme_smart,
};
#[cfg(feature = "test-support")]
pub use engine::startup::evidence::{
    collect_startup_boot_evidence, parse_systemd_critical_chain, parse_systemd_failed_units,
};
#[cfg(feature = "test-support")]
pub use engine::startup::{StartupManager, autostart_dirs_from_env, parse_systemd_blame};
#[cfg(feature = "test-support")]
pub use engine::storage_health::{
    collect_filesystem_health, collect_filesystem_health_from, parse_btrfs_error_stats,
    parse_mountinfo, parse_xfs_health_output,
};
pub use platform_handle::{LinuxPlatformRuntime, NativePlatformRuntime};

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
