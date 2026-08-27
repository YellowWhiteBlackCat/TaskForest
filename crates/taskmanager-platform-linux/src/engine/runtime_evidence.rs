//! Honest runtime capability receipts for Linux reference-provider evidence.
//!
//! A receipt records only what the running host made eligible for probing. It
//! does not turn device presence, a fixture, or a compiled feature into proof
//! that ATA SMART, NVML, or OpenRC calls succeeded. Target-host evidence must
//! pair this marker with the corresponding provider result/log. Standard
//! product artifacts include every supported hardware backend; build-profile
//! fields exist to detect accidental reduced/vendor-specific packaging.

#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use taskmanager_core::core::metrics::{StorageConnection, StorageInterconnect};

mod ebpf;
#[cfg(test)]
#[path = "../../tests/headless/engine/runtime_evidence/ebpf_tests.rs"]
mod ebpf_tests;

#[cfg(target_os = "linux")]
use crate::engine::hardware::classify_storage_connection;

const CAPABILITY_RECEIPT_SCHEMA_VERSION: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptSource {
    LiveHost,
    Fixture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitRuntimeEvidence {
    SystemdPid1,
    OpenrcRuntime,
    #[serde(alias = "assumed_systemd")]
    UnknownPid1,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareBuildProfile {
    /// Standard distributable: every hardware backend supported by this
    /// platform version is compiled and selected at runtime.
    StandardAll,
    /// Debug/test-only reduced build used to exercise fallback behavior.
    DeveloperReduced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProbeEligibility {
    Eligible,
    HardwareNotDetected,
    ToolMissing,
    #[serde(alias = "feature_disabled")]
    BackendNotCompiled,
    BackendInactive,
    PrivilegeRequired,
    #[default]
    BackendUnconfirmed,
    UnsupportedPlatform,
}

/// Exact Phase-3 target matrix carried by every capability receipt.
///
/// These values remain eligibility markers, not live success claims. In
/// particular, `backend_not_compiled` identifies an absent target adapter (the
/// current GPUI AT-SPI bridge), while `backend_unconfirmed` means a compiled
/// path still needs an observed target-host operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct LinuxTargetEnvironmentCapabilityReceipt {
    pub kernel_btf_available: bool,
    pub cgroup_v2_available: bool,
    pub unprivileged_bpf_disabled: Option<u8>,
    pub effective_bpf_privilege: bool,
    pub ebpf_compat_environment_available: bool,
    pub ebpf_compat_probe_permission_required: bool,
    pub amd_device_markers: usize,
    pub sas_candidate_devices: usize,
    pub usb_candidate_devices: usize,
    pub at_spi_session_detected: bool,
    pub ebpf_process_rates: ProviderProbeEligibility,
    pub nvidia_gpu: ProviderProbeEligibility,
    pub amd_gpu: ProviderProbeEligibility,
    pub ata_smart: ProviderProbeEligibility,
    pub sas_smart: ProviderProbeEligibility,
    pub usb_smart: ProviderProbeEligibility,
    pub openrc: ProviderProbeEligibility,
    pub at_spi: ProviderProbeEligibility,
    pub hotplug: ProviderProbeEligibility,
    /// Eligibility for the Intel i915/xe per-engine PMU path. This is a
    /// capability marker only; the provider must still publish a typed
    /// success/failure from the actual counter open/read operation.
    pub intel_gpu_engine_pmu: ProviderProbeEligibility,
    pub intel_gpu_engine_pmu_devices: usize,
    pub effective_perfmon_privilege: bool,
    pub perf_event_paranoid: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbpfObjectBuildIdentity {
    pub abi_version: u16,
    pub source_sha256: String,
    pub object_sha256: String,
    pub object_size: usize,
}

/// Redaction-safe provider capability facts. No model, serial, hostname,
/// command output, or device path is included in this receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LinuxProviderCapabilityReceipt {
    pub schema_version: u8,
    pub source: ReceiptSource,
    /// Always true: consumers must not treat this receipt as proof that a
    /// provider command/API call completed successfully.
    pub capability_only: bool,
    pub observed_at_unix_ms: u64,
    pub hardware_build_profile: HardwareBuildProfile,
    pub init_evidence: InitRuntimeEvidence,
    pub ata_candidate_devices: usize,
    pub nvme_namespace_devices: usize,
    pub nvidia_device_markers: usize,
    pub smartctl_available: bool,
    pub nvme_cli_available: bool,
    pub systemctl_available: bool,
    pub openrc_tools_available: bool,
    #[serde(alias = "nvidia_feature_enabled")]
    pub nvidia_backend_compiled: bool,
    pub ata_smart_probe: ProviderProbeEligibility,
    pub nvidia_nvml_probe: ProviderProbeEligibility,
    pub openrc_probe: ProviderProbeEligibility,
    pub systemd_probe: ProviderProbeEligibility,
    /// Build-time identity proves the standard object was embedded and
    /// checksum-gated. It does not claim live attachment or permission.
    #[serde(default)]
    pub ebpf_object: Option<EbpfObjectBuildIdentity>,
    #[serde(default)]
    pub target_environment: LinuxTargetEnvironmentCapabilityReceipt,
}

#[derive(Debug, Clone)]
struct RawCapabilityProbe {
    supported_platform: bool,
    hardware_build_profile: HardwareBuildProfile,
    init_evidence: InitRuntimeEvidence,
    ata_candidate_devices: usize,
    nvme_namespace_devices: usize,
    nvidia_device_markers: usize,
    smartctl_available: bool,
    nvme_cli_available: bool,
    systemctl_available: bool,
    openrc_tools_available: bool,
    nvidia_backend_compiled: bool,
    kernel_btf_available: bool,
    cgroup_v2_available: bool,
    unprivileged_bpf_disabled: Option<u8>,
    effective_bpf_privilege: bool,
    ebpf_compat_environment_available: bool,
    ebpf_compat_probe_permission_required: bool,
    ebpf_backend_compiled: bool,
    amd_device_markers: usize,
    sas_candidate_devices: usize,
    usb_candidate_devices: usize,
    at_spi_session_detected: bool,
    at_spi_backend_compiled: bool,
    hotplug_inventory_available: bool,
    intel_gpu_engine_pmu_devices: usize,
    effective_perfmon_privilege: bool,
    perf_event_paranoid: Option<i32>,
}

/// Collect redaction-safe markers from the current host. This performs only
/// fixed-scope filesystem/PATH inspection and never invokes a provider command.
#[must_use]
pub fn collect_linux_provider_capability_receipt() -> LinuxProviderCapabilityReceipt {
    build_receipt(
        ReceiptSource::LiveHost,
        unix_time_millis(std::time::SystemTime::now()),
        collect_live_probe(),
    )
}

/// Serialize a receipt with stable struct-field ordering and a trailing newline.
pub fn linux_provider_capability_receipt_json(
    receipt: &LinuxProviderCapabilityReceipt,
) -> Result<String, serde_json::Error> {
    let mut json = serde_json::to_string_pretty(receipt)?;
    json.push('\n');
    Ok(json)
}

fn build_receipt(
    source: ReceiptSource,
    observed_at_unix_ms: u64,
    probe: RawCapabilityProbe,
) -> LinuxProviderCapabilityReceipt {
    let ebpf_object = ebpf::compiled_object_identity(probe.ebpf_backend_compiled);
    let ata_smart_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.ata_candidate_devices == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else if !probe.smartctl_available {
        ProviderProbeEligibility::ToolMissing
    } else {
        ProviderProbeEligibility::Eligible
    };
    let nvidia_nvml_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if !probe.nvidia_backend_compiled {
        ProviderProbeEligibility::BackendNotCompiled
    } else if probe.nvidia_device_markers == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else {
        ProviderProbeEligibility::Eligible
    };
    let openrc_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.init_evidence == InitRuntimeEvidence::UnknownPid1 {
        ProviderProbeEligibility::BackendUnconfirmed
    } else if probe.init_evidence != InitRuntimeEvidence::OpenrcRuntime {
        ProviderProbeEligibility::BackendInactive
    } else if !probe.openrc_tools_available {
        ProviderProbeEligibility::ToolMissing
    } else {
        ProviderProbeEligibility::Eligible
    };
    let systemd_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.init_evidence == InitRuntimeEvidence::UnknownPid1 {
        ProviderProbeEligibility::BackendUnconfirmed
    } else if probe.init_evidence != InitRuntimeEvidence::SystemdPid1 {
        ProviderProbeEligibility::BackendInactive
    } else if !probe.systemctl_available {
        ProviderProbeEligibility::ToolMissing
    } else {
        ProviderProbeEligibility::Eligible
    };
    let sas_smart_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.sas_candidate_devices == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else if !probe.smartctl_available {
        ProviderProbeEligibility::ToolMissing
    } else {
        ProviderProbeEligibility::Eligible
    };
    let usb_smart_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.usb_candidate_devices == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else if !probe.smartctl_available {
        ProviderProbeEligibility::ToolMissing
    } else {
        ProviderProbeEligibility::Eligible
    };
    let amd_gpu_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.amd_device_markers == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else {
        ProviderProbeEligibility::Eligible
    };
    let ebpf_process_rates = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if !probe.ebpf_backend_compiled {
        ProviderProbeEligibility::BackendNotCompiled
    } else if !probe.kernel_btf_available && probe.ebpf_compat_probe_permission_required {
        ProviderProbeEligibility::PrivilegeRequired
    } else if !probe.kernel_btf_available && !probe.ebpf_compat_environment_available {
        ProviderProbeEligibility::BackendInactive
    } else if probe
        .unprivileged_bpf_disabled
        .is_some_and(|value| value > 0)
        && !probe.effective_bpf_privilege
    {
        ProviderProbeEligibility::PrivilegeRequired
    } else if probe.unprivileged_bpf_disabled.is_none() && !probe.effective_bpf_privilege {
        ProviderProbeEligibility::BackendUnconfirmed
    } else {
        ProviderProbeEligibility::Eligible
    };
    let at_spi_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if !probe.at_spi_backend_compiled {
        ProviderProbeEligibility::BackendNotCompiled
    } else if !probe.at_spi_session_detected {
        ProviderProbeEligibility::BackendInactive
    } else {
        ProviderProbeEligibility::Eligible
    };
    let hotplug_probe = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if !probe.hotplug_inventory_available {
        ProviderProbeEligibility::BackendInactive
    } else {
        ProviderProbeEligibility::BackendUnconfirmed
    };
    let intel_gpu_engine_pmu = if !probe.supported_platform {
        ProviderProbeEligibility::UnsupportedPlatform
    } else if probe.intel_gpu_engine_pmu_devices == 0 {
        ProviderProbeEligibility::HardwareNotDetected
    } else if !probe.effective_perfmon_privilege
        && probe.perf_event_paranoid.is_some_and(|value| value >= 2)
    {
        ProviderProbeEligibility::PrivilegeRequired
    } else if !probe.effective_perfmon_privilege && probe.perf_event_paranoid.is_none() {
        ProviderProbeEligibility::BackendUnconfirmed
    } else {
        ProviderProbeEligibility::Eligible
    };
    let target_environment = LinuxTargetEnvironmentCapabilityReceipt {
        kernel_btf_available: probe.kernel_btf_available,
        cgroup_v2_available: probe.cgroup_v2_available,
        unprivileged_bpf_disabled: probe.unprivileged_bpf_disabled,
        effective_bpf_privilege: probe.effective_bpf_privilege,
        ebpf_compat_environment_available: probe.ebpf_compat_environment_available,
        ebpf_compat_probe_permission_required: probe.ebpf_compat_probe_permission_required,
        amd_device_markers: probe.amd_device_markers,
        sas_candidate_devices: probe.sas_candidate_devices,
        usb_candidate_devices: probe.usb_candidate_devices,
        at_spi_session_detected: probe.at_spi_session_detected,
        ebpf_process_rates,
        nvidia_gpu: nvidia_nvml_probe,
        amd_gpu: amd_gpu_probe,
        ata_smart: ata_smart_probe,
        sas_smart: sas_smart_probe,
        usb_smart: usb_smart_probe,
        openrc: openrc_probe,
        at_spi: at_spi_probe,
        hotplug: hotplug_probe,
        intel_gpu_engine_pmu,
        intel_gpu_engine_pmu_devices: probe.intel_gpu_engine_pmu_devices,
        effective_perfmon_privilege: probe.effective_perfmon_privilege,
        perf_event_paranoid: probe.perf_event_paranoid,
    };

    LinuxProviderCapabilityReceipt {
        schema_version: CAPABILITY_RECEIPT_SCHEMA_VERSION,
        source,
        capability_only: true,
        observed_at_unix_ms,
        hardware_build_profile: probe.hardware_build_profile,
        init_evidence: probe.init_evidence,
        ata_candidate_devices: probe.ata_candidate_devices,
        nvme_namespace_devices: probe.nvme_namespace_devices,
        nvidia_device_markers: probe.nvidia_device_markers,
        smartctl_available: probe.smartctl_available,
        nvme_cli_available: probe.nvme_cli_available,
        systemctl_available: probe.systemctl_available,
        openrc_tools_available: probe.openrc_tools_available,
        nvidia_backend_compiled: probe.nvidia_backend_compiled,
        ata_smart_probe,
        nvidia_nvml_probe,
        openrc_probe,
        systemd_probe,
        ebpf_object,
        target_environment,
    }
}

#[cfg(target_os = "linux")]
fn collect_live_probe() -> RawCapabilityProbe {
    let block_names = directory_names(Path::new("/sys/class/block"));
    let ata_candidate_devices = block_names
        .iter()
        .filter(|name| is_ata_candidate(name))
        .count();
    let nvme_namespace_devices = block_names
        .iter()
        .filter(|name| is_nvme_namespace(name))
        .count();
    let proc_nvidia = directory_names(Path::new("/proc/driver/nvidia/gpus")).len();
    let drm_nvidia = count_drm_vendor(Path::new("/sys/class/drm"), "0x10de");
    let amd_device_markers = count_drm_vendor(Path::new("/sys/class/drm"), "0x1002");
    let block_connections: Vec<_> = block_names
        .iter()
        .filter(|name| {
            !Path::new("/sys/class/block")
                .join(name)
                .join("partition")
                .exists()
        })
        .map(|name| classify_live_block_connection(Path::new("/sys/class/block"), name))
        .collect();
    let sas_candidate_devices = block_connections
        .iter()
        .filter(|connection| connection.interconnect == StorageInterconnect::Sas)
        .count();
    let usb_candidate_devices = block_connections
        .iter()
        .filter(|connection| connection.interconnect == StorageInterconnect::Usb)
        .count();
    let path_entries = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<PathBuf>>())
        .unwrap_or_default();
    let pid_one = std::fs::read_to_string("/proc/1/comm").ok();
    // A packaged `/sbin/openrc` only proves that OpenRC is installed. Runtime
    // evidence must agree with provider selection and therefore comes from
    // PID 1 or the active-state directory created for the current boot.
    let openrc_runtime_active =
        Path::new("/run/openrc/softlevel").is_file() || Path::new("/run/openrc").is_dir();
    let init_evidence = classify_init_evidence(pid_one.as_deref(), openrc_runtime_active);
    let intel_gpu_engine_pmu_devices = intel_gpu_engine_pmu_devices();
    let perf_event_paranoid = std::fs::read_to_string("/proc/sys/kernel/perf_event_paranoid")
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok());

    RawCapabilityProbe {
        supported_platform: true,
        hardware_build_profile: classify_hardware_build_profile(
            cfg!(feature = "hardware-all"),
            cfg!(feature = "nvidia"),
        ),
        init_evidence,
        ata_candidate_devices,
        nvme_namespace_devices,
        nvidia_device_markers: proc_nvidia.max(drm_nvidia),
        smartctl_available: command_available("smartctl", &path_entries),
        nvme_cli_available: command_available("nvme", &path_entries),
        systemctl_available: command_available("systemctl", &path_entries),
        openrc_tools_available: ["rc-status", "rc-service", "rc-update"]
            .iter()
            .all(|program| command_available(program, &path_entries)),
        nvidia_backend_compiled: cfg!(feature = "nvidia"),
        kernel_btf_available: Path::new("/sys/kernel/btf/vmlinux").is_file(),
        cgroup_v2_available: Path::new("/sys/fs/cgroup/cgroup.controllers").is_file(),
        unprivileged_bpf_disabled: std::fs::read_to_string(
            "/proc/sys/kernel/unprivileged_bpf_disabled",
        )
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok()),
        effective_bpf_privilege: ebpf::effective_privilege(),
        // The pure-safe-Rust build compiles no eBPF backend; the receipt keeps
        // the eligibility fields so it can honestly report `BackendNotCompiled`.
        ebpf_compat_environment_available: false,
        ebpf_compat_probe_permission_required: false,
        ebpf_backend_compiled: false,
        amd_device_markers,
        sas_candidate_devices,
        usb_candidate_devices,
        at_spi_session_detected: at_spi_session_detected(),
        // Current GPUI does not expose an AccessKit/AT-SPI bridge.
        at_spi_backend_compiled: false,
        hotplug_inventory_available: Path::new("/sys/class/block").is_dir()
            && Path::new("/sys/class/net").is_dir(),
        intel_gpu_engine_pmu_devices,
        effective_perfmon_privilege: ebpf::effective_perfmon_privilege(),
        perf_event_paranoid,
    }
}

#[cfg(not(target_os = "linux"))]
fn collect_live_probe() -> RawCapabilityProbe {
    RawCapabilityProbe {
        supported_platform: false,
        hardware_build_profile: classify_hardware_build_profile(
            cfg!(feature = "hardware-all"),
            cfg!(feature = "nvidia"),
        ),
        init_evidence: InitRuntimeEvidence::UnsupportedPlatform,
        ata_candidate_devices: 0,
        nvme_namespace_devices: 0,
        nvidia_device_markers: 0,
        smartctl_available: false,
        nvme_cli_available: false,
        systemctl_available: false,
        openrc_tools_available: false,
        nvidia_backend_compiled: cfg!(feature = "nvidia"),
        kernel_btf_available: false,
        cgroup_v2_available: false,
        unprivileged_bpf_disabled: None,
        effective_bpf_privilege: false,
        ebpf_compat_environment_available: false,
        ebpf_compat_probe_permission_required: false,
        ebpf_backend_compiled: false,
        amd_device_markers: 0,
        sas_candidate_devices: 0,
        usb_candidate_devices: 0,
        at_spi_session_detected: false,
        at_spi_backend_compiled: false,
        hotplug_inventory_available: false,
        intel_gpu_engine_pmu_devices: 0,
        effective_perfmon_privilege: false,
        perf_event_paranoid: None,
    }
}

const fn classify_hardware_build_profile(
    hardware_all: bool,
    nvidia_backend_compiled: bool,
) -> HardwareBuildProfile {
    if hardware_all && nvidia_backend_compiled {
        HardwareBuildProfile::StandardAll
    } else {
        HardwareBuildProfile::DeveloperReduced
    }
}

#[cfg(target_os = "linux")]
fn classify_init_evidence(
    pid_one_comm: Option<&str>,
    openrc_runtime_active: bool,
) -> InitRuntimeEvidence {
    if pid_one_comm.is_some_and(|comm| comm.trim() == "systemd") {
        InitRuntimeEvidence::SystemdPid1
    } else if pid_one_comm.is_some_and(|comm| comm.trim() == "openrc-init") || openrc_runtime_active
    {
        InitRuntimeEvidence::OpenrcRuntime
    } else {
        InitRuntimeEvidence::UnknownPid1
    }
}

#[cfg(target_os = "linux")]
fn directory_names(path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

#[cfg(target_os = "linux")]
fn classify_live_block_connection(block_root: &Path, name: &str) -> StorageConnection {
    let path = block_root.join(name);
    let transport = read_trimmed(&path.join("device/transport"));
    let protocol = read_trimmed(&path.join("device/protocol"));
    let subsystem = std::fs::canonicalize(path.join("device/subsystem"))
        .ok()
        .and_then(|subsystem| {
            subsystem
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
        });
    let topology = std::fs::canonicalize(&path)
        .ok()
        .map(|value| value.to_string_lossy().into_owned());
    classify_storage_connection(
        name,
        transport.as_deref(),
        protocol.as_deref(),
        subsystem.as_deref(),
        topology.as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn at_spi_session_detected() -> bool {
    if std::env::var_os("AT_SPI_BUS_ADDRESS").is_some_and(|value| !value.is_empty()) {
        return true;
    }
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|runtime| runtime.join("at-spi"))
        .is_some_and(|directory| !directory_names(&directory).is_empty())
}

#[cfg(target_os = "linux")]
fn is_ata_candidate(name: &str) -> bool {
    name.strip_prefix("sd")
        .or_else(|| name.strip_prefix("hd"))
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_lowercase())
        })
}

#[cfg(target_os = "linux")]
fn is_nvme_namespace(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("nvme") else {
        return false;
    };
    let Some((controller, namespace)) = rest.split_once('n') else {
        return false;
    };
    !controller.is_empty()
        && controller.bytes().all(|byte| byte.is_ascii_digit())
        && !namespace.is_empty()
        && namespace.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(target_os = "linux")]
fn count_drm_vendor(drm_root: &Path, expected_vendor: &str) -> usize {
    directory_names(drm_root)
        .into_iter()
        .filter(|name| {
            name.strip_prefix("card")
                .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .filter(|name| {
            std::fs::read_to_string(drm_root.join(name).join("device/vendor"))
                .is_ok_and(|vendor| vendor.trim().eq_ignore_ascii_case(expected_vendor))
        })
        .count()
}

#[cfg(target_os = "linux")]
fn intel_gpu_engine_pmu_devices() -> usize {
    let root = Path::new("/sys/bus/event_source/devices");
    directory_names(root)
        .into_iter()
        .filter(|name| name == "i915" || name.starts_with("xe_"))
        .filter(|name| root.join(name).join("type").is_file())
        .count()
}

#[cfg(target_os = "linux")]
fn command_available(program: &str, path_entries: &[PathBuf]) -> bool {
    path_entries
        .iter()
        .map(|directory| directory.join(OsString::from(program)))
        .any(|candidate| is_executable_file(&candidate))
}

#[cfg(target_os = "linux")]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn unix_time_millis(now: std::time::SystemTime) -> u64 {
    taskmanager_core::core::time::unix_millis(now)
}

#[cfg(test)]
#[path = "../../tests/headless/engine/runtime_evidence.rs"]
mod tests;
