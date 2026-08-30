//! Small audited Windows API boundary with a safe, platform-neutral surface.
//!
//! The Windows adapter is deliberately kept as an audited `unsafe` boundary.
//! Calls that have no suitable published safe wrapper live here instead, where
//! the raw Win32 ABI is contained and reviewed. This includes bounded
//! processor topology/cache and NIC metadata queries. No handle, pointer, or
//! UTF-16 buffer crosses this crate's public API.

#![deny(unsafe_op_in_unsafe_fn)]
// This boundary crate compiles on every host so Linux CI can run the
// contract suite against its typed surface, but most private helpers are
// consumed only by `#[cfg(windows)]` call sites — off-Windows builds
// therefore legitimately see them as dead code.
#![cfg_attr(not(windows), allow(dead_code))]

use std::fmt;

mod disk;
mod display;
mod event_log;
mod gpu;
mod icons;
mod job_control;
mod known_folders;
mod memory_info;
mod msg_pump;
mod network;
mod npu;
mod open_files;
mod pdh;
mod power;
mod process;
mod process_network;
mod process_tree;
mod runas;
mod sessions;
mod single_instance;
mod smbios;
mod task_scheduler;
mod thermal;
mod time_zones;
mod topology;
mod wsl;

/// Failure of a native Windows query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsApiError {
    /// The current target is not Windows, so the native source is dormant.
    Unsupported,
    /// The Windows API returned failure or an invalid result.
    QueryFailed,
    /// Windows returned text that could not be decoded as UTF-16.
    InvalidText,
    /// A native result exceeded a fixed resource bound.
    ResourceLimit,
    /// The caller supplied an empty or overlong native identifier.
    InvalidInput,
    /// Windows rejected the operation for the current token.
    PermissionDenied,
    /// The target disappeared or changed identity before the operation.
    IdentityChanged,
}

impl fmt::Display for WindowsApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str("Windows API is unavailable on this target"),
            Self::QueryFailed => formatter.write_str("Windows API query failed"),
            Self::InvalidText => formatter.write_str("Windows API returned invalid UTF-16 text"),
            Self::ResourceLimit => formatter.write_str("Windows API result exceeded its bound"),
            Self::InvalidInput => formatter.write_str("Windows API input is invalid"),
            Self::PermissionDenied => formatter.write_str("Windows API permission was denied"),
            Self::IdentityChanged => formatter.write_str("Windows target identity changed"),
        }
    }
}

impl std::error::Error for WindowsApiError {}

pub use disk::{
    WindowsDiskBusType, WindowsDiskDeviceInfo, WindowsDiskMediaType, WindowsDiskPerformance,
    WindowsDiskSmartInfo, query_disk_device_info, query_disk_performance, query_disk_smart_info,
};
pub use display::{WindowsMonitorDescriptor, enumerate_display_monitors};
pub use event_log::{
    MAX_EVENT_LOG_ENTRIES_PER_QUERY, WindowsEventLogEntry, WindowsEventLogQuery, query_event_log,
};
pub use gpu::{
    MAX_GPU_ADAPTERS, WindowsGpuAdapter, WindowsGpuAdapterInventory, WindowsPciAddress,
    enumerate_gpu_adapters,
};
pub use icons::extract_process_icon_bmp;
pub use job_control::{WindowsJobLimitRequest, apply_process_job_limits, clear_process_job_limits};
pub use known_folders::{KnownFolder, known_folder_path};
pub use memory_info::query_memory_compression_used_bytes;
pub use msg_pump::{MAX_PUMPED_MESSAGES_PER_CALL, pump_pending_messages};
pub use network::{WindowsAdapterType, WindowsNetworkAdapter, enumerate_network_adapters};
pub use npu::{WindowsComputeAccelerator, enumerate_compute_accelerators};
pub use open_files::{WindowsOpenHandleEntry, WindowsOpenHandleKind, query_process_open_files};
pub use pdh::{
    WindowsCpuFrequencySample, WindowsGpuAdapterMemorySample, WindowsGpuEngineDetail,
    WindowsGpuEngineInstanceSample, WindowsGpuEngineSample, WindowsGpuProcessMemorySample,
    query_cpu_dynamic_frequencies, query_gpu_adapter_memory, query_gpu_engine_instances,
    query_gpu_engine_utilization, query_gpu_process_memory,
};
pub use power::{
    WindowsProcessorPowerInfo, WindowsSystemPowerStatus, active_power_scheme_name,
    effective_power_overlay_name, power_overlay_label, query_processor_power_information,
    query_system_power_status,
};
pub use process::{
    MAX_PROCESS_ENVIRONMENT_BYTES, MAX_PROCESS_ENVIRONMENT_ENTRIES, ProcessPriorityClass,
    WindowsIntegrityLevel, WindowsProcessEnvironmentBlock, WindowsProcessGuiResources,
    WindowsProcessIsolation, WindowsProcessMemoryCounters, WindowsProcessModule,
    WindowsThreadDetail, WindowsThreadInfo, enumerate_all_process_thread_counts, process_affinity,
    process_creation_time_100ns, process_gui_resources, process_handle_count, process_is_elevated,
    process_isolation, process_memory_counters, process_modules, process_priority, process_threads,
    query_process_environment, query_process_thread_details, query_process_user,
    resume_process_threads, set_process_affinity_exact, set_process_priority_exact,
    suspend_process_threads, terminate_process_exact,
};
pub use process_network::{
    WindowsProcessConnection, WindowsTcpState, WindowsTransportProtocol,
    query_process_network_connections,
};
pub use process_tree::{WindowsProcessJob, assign_and_resume_suspended_process};
pub use runas::{RunasLaunchOutcome, interactive_session_available, run_elevated_and_wait};
pub use sessions::{
    WindowsSession, WindowsSessionState, enumerate_sessions, lock_workstation, logoff_session,
};
pub use single_instance::{InstanceEvent, InstanceMutex, signal_named_event};
pub use smbios::{query_smbios_processor_max_mhz, raw_smbios_table};
pub use task_scheduler::{WindowsStartupTask, enumerate_startup_tasks};
pub use thermal::{WindowsThermalZoneReading, query_acpi_thermal_zones};
pub use time_zones::{
    WindowsTimeZoneRules, WindowsTransitionRule, WindowsYearZoneRule, query_time_zone_rules,
};
pub use topology::{
    WindowsCoreBreakdown, WindowsCpuType, WindowsProcessorTopology, processor_topology,
};
pub use wsl::{WindowsWslDistro, query_wsl_distributions};

/// The only service configuration mutation exposed by this boundary. Passing
/// only the start type to `ChangeServiceConfigW` makes Windows preserve the
/// existing binary path, launch arguments, dependencies, account and display
/// name instead of reconstructing them in the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceStartMode {
    Automatic,
    Disabled,
}

/// Process/thread/memory totals returned by the native performance information API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SystemPerformance {
    pub process_count: u32,
    pub thread_count: u32,
    pub handle_count: u32,
    pub commit_total_pages: usize,
    pub commit_limit_pages: usize,
    pub physical_total_pages: usize,
    pub physical_available_pages: usize,
    pub system_cache_pages: usize,
    pub kernel_paged_pages: usize,
    pub kernel_nonpaged_pages: usize,
    pub page_size_bytes: usize,
}

/// Read process, thread, and memory performance totals without spawning a shell.
#[must_use = "inspect the native performance query result"]
pub fn system_performance() -> Result<SystemPerformance, WindowsApiError> {
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use windows::Win32::System::ProcessStatus::{
            K32GetPerformanceInfo, PERFORMANCE_INFORMATION,
        };

        let mut information = PERFORMANCE_INFORMATION {
            cb: u32::try_from(size_of::<PERFORMANCE_INFORMATION>())
                .map_err(|_| WindowsApiError::QueryFailed)?,
            ..Default::default()
        };
        let succeeded = {
            // SAFETY: `information` is a valid, writable
            // `PERFORMANCE_INFORMATION` value whose `cb` field matches its
            // allocated size; the generated binding does not retain the
            // pointer after this synchronous call.
            unsafe { K32GetPerformanceInfo(&mut information, information.cb) }.as_bool()
        };
        if !succeeded {
            return Err(WindowsApiError::QueryFailed);
        }
        Ok(SystemPerformance {
            process_count: information.ProcessCount,
            thread_count: information.ThreadCount,
            handle_count: information.HandleCount,
            commit_total_pages: information.CommitTotal,
            commit_limit_pages: information.CommitLimit,
            physical_total_pages: information.PhysicalTotal,
            physical_available_pages: information.PhysicalAvailable,
            system_cache_pages: information.SystemCache,
            kernel_paged_pages: information.KernelPaged,
            kernel_nonpaged_pages: information.KernelNonpaged,
            page_size_bytes: information.PageSize,
        })
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Read the user's Windows locale using the native globalization API.
#[must_use = "inspect the native locale query result"]
pub fn user_locale_name() -> Result<String, WindowsApiError> {
    #[cfg(windows)]
    {
        // LOCALE_NAME_MAX_LENGTH is 85, including the terminating NUL.
        let mut buffer = [0_u16; 85];
        let length = {
            // SAFETY: `buffer` is a writable fixed-size UTF-16 buffer with the
            // documented maximum locale-name capacity; Windows writes at most
            // its declared length and terminates the returned name.
            unsafe { windows::Win32::Globalization::GetUserDefaultLocaleName(&mut buffer) }
        };
        decode_locale_name(&buffer, length)
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Change only a Windows service's start mode without shelling out or
/// reserializing its other SCM configuration fields.
#[must_use = "inspect the native service configuration result"]
pub fn set_service_start_mode(
    service_name: &str,
    mode: ServiceStartMode,
) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        set_service_start_mode_windows(service_name, mode)
    }
    #[cfg(not(windows))]
    {
        let _ = (service_name, mode);
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
const MAX_SERVICE_NAME_UTF16: usize = 256;

#[cfg(windows)]
fn encode_service_name(service_name: &str) -> Result<Vec<u16>, WindowsApiError> {
    if service_name.is_empty() || service_name.contains('\0') {
        return Err(WindowsApiError::InvalidInput);
    }
    let mut encoded = Vec::new();
    encoded
        .try_reserve(MAX_SERVICE_NAME_UTF16.saturating_add(1))
        .map_err(|_| WindowsApiError::QueryFailed)?;
    for unit in service_name.encode_utf16() {
        if encoded.len() >= MAX_SERVICE_NAME_UTF16 {
            return Err(WindowsApiError::InvalidInput);
        }
        encoded.push(unit);
    }
    encoded.push(0);
    Ok(encoded)
}

#[cfg(windows)]
struct ServiceHandle(windows::Win32::System::Services::SC_HANDLE);

#[cfg(windows)]
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by OpenSCManagerW/OpenServiceW and
        // is owned exclusively by this RAII guard; Drop runs at most once.
        let _ = unsafe { windows::Win32::System::Services::CloseServiceHandle(self.0) };
    }
}

#[cfg(windows)]
fn set_service_start_mode_windows(
    service_name: &str,
    mode: ServiceStartMode,
) -> Result<(), WindowsApiError> {
    use windows::Win32::System::Services::{
        ChangeServiceConfigW, ENUM_SERVICE_TYPE, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT,
        SERVICE_CHANGE_CONFIG, SERVICE_ERROR, SERVICE_NO_CHANGE,
    };
    use windows::core::PCWSTR;

    let encoded_name = encode_service_name(service_name)?;
    let manager = {
        // SAFETY: null machine/database names request the local SCM; the
        // requested access is read-only connection access and the generated
        // binding does not retain any caller pointer.
        unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
            .map_err(|_| WindowsApiError::QueryFailed)?
    };
    let manager = ServiceHandle(manager);
    let service = {
        // SAFETY: `encoded_name` is a bounded, NUL-terminated UTF-16 buffer
        // alive for this synchronous call; `manager` owns a valid SCM handle.
        unsafe {
            OpenServiceW(
                manager.0,
                PCWSTR(encoded_name.as_ptr()),
                SERVICE_CHANGE_CONFIG,
            )
        }
        .map_err(|_| WindowsApiError::QueryFailed)?
    };
    let service = ServiceHandle(service);
    let start_type = match mode {
        ServiceStartMode::Automatic => windows::Win32::System::Services::SERVICE_AUTO_START,
        ServiceStartMode::Disabled => windows::Win32::System::Services::SERVICE_DISABLED,
    };
    // SAFETY: the service handle is valid and owned by `service`; every
    // nullable string parameter is intentionally null so the SCM preserves
    // the existing field, and no pointer escapes this synchronous call.
    unsafe {
        ChangeServiceConfigW(
            service.0,
            ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            start_type,
            SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    }
    .map_err(|_| WindowsApiError::QueryFailed)
}

/// Decode the native API's length-prefixed UTF-16 result without trusting the
/// external length as a slice bound. Windows documents a terminating NUL, but
/// a malformed/mock result must still become a typed error rather than a panic.
#[cfg(any(windows, test))]
fn decode_locale_name(buffer: &[u16], length: i32) -> Result<String, WindowsApiError> {
    if length <= 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    let length = usize::try_from(length).map_err(|_| WindowsApiError::QueryFailed)?;
    if length > buffer.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    String::from_utf16(&buffer[..length.saturating_sub(1)])
        .map_err(|_| WindowsApiError::InvalidText)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_lib.rs"]
mod tests;
