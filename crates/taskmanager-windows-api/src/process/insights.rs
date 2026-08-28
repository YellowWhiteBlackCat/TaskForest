//! Extended process telemetry insights (modules, memory counters, handles,
//! isolation, GUI resources, users, environment/cwd).

use super::*;

// ---------------------------------------------------------------------------
// Per-process environment and working directory.
//
// Windows has no `/proc/<pid>/{environ,cwd}`; the honest source is the target
// PEB itself: `NtQueryInformationProcess(ProcessBasicInformation)` yields the
// PEB address, the PEB's `ProcessParameters` points at
// `RTL_USER_PROCESS_PARAMETERS`, whose `CurrentDirectory.DosPath`
// `UNICODE_STRING` and `Environment` pointer are then read with bounded
// `ReadProcessMemory` calls. The `windows` crate ships the query and read
// functions but not the `PROCESS_BASIC_INFORMATION`/`PEB`/parameters structs
// (they sit behind a feature this crate does not enable), so the layouts are
// walked as raw bytes with width-derived offsets — the audited
// `memory_info.rs`/`open_files.rs` precedent. Every offset below derives from
// the target pointer width, and the WOW64 check below guarantees that width
// equals this build's before any layout arithmetic runs.
// ---------------------------------------------------------------------------

/// Hard cap on raw environment-block bytes read per process. Mirrors the core
/// contract's `MAX_ENVIRONMENT_BYTES` budget (the contract is the authority;
/// this copy exists because the boundary crate depends on no other crate).
/// Note the Windows block is UTF-16, so the budget covers raw block bytes.
pub const MAX_PROCESS_ENVIRONMENT_BYTES: usize = 16 * 1024;
/// Hard cap on environment entries retained per process; mirrors the core
/// contract's `MAX_ENVIRONMENT_ENTRIES`. Entries beyond it are counted in
/// `truncated_count`, never silently dropped.
pub const MAX_PROCESS_ENVIRONMENT_ENTRIES: usize = 256;
/// `ReadProcessMemory` chunk for the environment block. Chunked (instead of
/// one budget-sized read) so a read that steps past the block's committed
/// pages cannot fail the whole bounded collection.
#[cfg(windows)]
const ENVIRONMENT_READ_CHUNK_BYTES: usize = 4096;
/// Ceiling for the `CurrentDirectory.DosPath` byte length. It covers the full
/// 32 767-unit Windows path namespace; a longer "length" is a corrupt
/// `UNICODE_STRING` and yields an honest `None` cwd instead of a truncation.
const MAX_WORKING_DIRECTORY_BYTES: usize = 32 * 1024;
/// `STATUS_ACCESS_DENIED` (0xC0000022) from `NtQueryInformationProcess`.
#[cfg(windows)]
const STATUS_ACCESS_DENIED: i32 = -1_073_741_790;

/// Bounded environment facts for one process: the working directory as
/// observed (an honest `None` when the native source exposes none) plus the
/// bounded `KEY=VALUE` entries in source order. `truncated_count` reports the
/// entries the byte/entry budgets dropped.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WindowsProcessEnvironmentBlock {
    pub working_directory: Option<String>,
    pub entries: Vec<(String, String)>,
    pub truncated_count: u32,
}

/// Query one process's working directory and environment block through its
/// PEB.
///
/// Typed failures: `PermissionDenied` when `OpenProcess` or the query is
/// refused (other users' or protected processes), `IdentityChanged` when the
/// pid no longer names a process, `Unsupported` for a cross-bitness target —
/// a WOW64 (32-bit) target seen from a 64-bit build (or the reverse) has a
/// second, differently laid out PEB whose offsets this walk refuses to guess.
#[must_use = "inspect the process environment result"]
pub fn query_process_environment(
    pid: u32,
) -> Result<WindowsProcessEnvironmentBlock, WindowsApiError> {
    #[cfg(windows)]
    {
        environment::query_process_environment_windows(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

mod environment;

const MAX_PROCESS_MODULES: usize = 2048;

#[cfg(windows)]
const MAX_SID_SUB_AUTHORITIES: usize = 15;

#[cfg(windows)]
fn sid_pointer_is_within_buffer(
    buffer: &[u64],
    returned_bytes: usize,
    sid: windows::Win32::Security::PSID,
) -> bool {
    use std::mem::{align_of, size_of, size_of_val};
    use windows::Win32::Security::SID;

    if sid.0.is_null() || returned_bytes > size_of_val(buffer) {
        return false;
    }
    let start = buffer.as_ptr() as usize;
    let Some(end) = start.checked_add(returned_bytes) else {
        return false;
    };
    let address = sid.0 as usize;
    let Some(header_end) = address.checked_add(2) else {
        return false;
    };
    if address < start || header_end > end || !address.is_multiple_of(align_of::<SID>()) {
        return false;
    }

    // SAFETY: The two-byte SID header was proven to be within the returned
    // native buffer. Reading the byte fields does not require a wider slice.
    let (revision, sub_authority_count) = unsafe {
        let bytes = sid.0.cast::<u8>();
        (*bytes, *bytes.add(1))
    };
    if revision != 1 || usize::from(sub_authority_count) > MAX_SID_SUB_AUTHORITIES {
        return false;
    }
    let Some(sub_authority_bytes) = usize::from(sub_authority_count).checked_mul(size_of::<u32>())
    else {
        return false;
    };
    let sid_bytes = 8_usize.checked_add(sub_authority_bytes);
    let Some(sid_bytes) = sid_bytes else {
        return false;
    };
    address
        .checked_add(sid_bytes)
        .is_some_and(|sid_end| sid_end <= end)
}

/// Query process isolation, elevation and integrity level facts.
#[must_use = "inspect the process isolation result"]
pub fn process_isolation(pid: u32) -> Result<WindowsProcessIsolation, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use std::mem::size_of;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{
            GetTokenInformation, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_MANDATORY_LABEL,
            TOKEN_QUERY, TokenElevation, TokenElevationType, TokenElevationTypeFull,
            TokenIntegrityLevel, TokenIsAppContainer,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        // SAFETY: OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION.
        let proc_handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(map_windows_error)?;
        let _proc_guard = ProcessHandle(proc_handle);

        let mut token = HANDLE::default();
        // SAFETY: OpenProcessToken with TOKEN_QUERY.
        let token_ok = unsafe { OpenProcessToken(proc_handle, TOKEN_QUERY, &mut token) }.is_ok();
        if !token_ok || token.is_invalid() {
            return Err(WindowsApiError::PermissionDenied);
        }
        let _token_guard = ProcessHandle(token);

        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        // SAFETY: `token` is valid and `elevation` is a matching writable structure.
        let elevation_queried = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some(core::ptr::from_mut(&mut elevation).cast()),
                u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
                &mut return_length,
            )
        };
        if elevation_queried.is_err() {
            let mut elevation_type = TOKEN_ELEVATION_TYPE::default();
            // SAFETY: `token` is valid and `elevation_type` is a matching writable structure.
            let type_queried = unsafe {
                GetTokenInformation(
                    token,
                    TokenElevationType,
                    Some(core::ptr::from_mut(&mut elevation_type).cast()),
                    u32::try_from(size_of::<TOKEN_ELEVATION_TYPE>()).unwrap_or(0),
                    &mut return_length,
                )
            };
            if type_queried.is_ok() {
                elevation.TokenIsElevated = u32::from(elevation_type == TokenElevationTypeFull);
            }
        }

        let mut is_app_container_dword = 0u32;
        // SAFETY: `token` is valid and `is_app_container_dword` is a matching writable DWORD.
        let _ = unsafe {
            GetTokenInformation(
                token,
                TokenIsAppContainer,
                Some(core::ptr::from_mut(&mut is_app_container_dword).cast()),
                u32::try_from(size_of::<u32>()).unwrap_or(0),
                &mut return_length,
            )
        };

        let mut integrity_buffer = [0_u64; 16];
        let mut integrity_level = None;
        let mut integrity_return_length = 0_u32;
        // SAFETY: `token` is valid and `integrity_buffer` is an aligned,
        // initialized writable byte buffer large enough for the query.
        let integrity_queried = unsafe {
            GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(integrity_buffer.as_mut_ptr().cast()),
                u32::try_from(std::mem::size_of_val(&integrity_buffer)).unwrap_or(0),
                &mut integrity_return_length,
            )
        };

        let integrity_bytes = usize::try_from(integrity_return_length).unwrap_or(usize::MAX);
        if integrity_queried.is_ok()
            && integrity_bytes >= size_of::<TOKEN_MANDATORY_LABEL>()
            && integrity_bytes <= std::mem::size_of_val(&integrity_buffer)
        {
            let label = integrity_buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
            // SAFETY: the returned length proves the outer token structure is
            // initialized and the u64 buffer provides its required alignment.
            let sid_ptr = unsafe { (*label).Label.Sid };
            if sid_pointer_is_within_buffer(&integrity_buffer, integrity_bytes, sid_ptr) {
                use windows::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount};
                // SAFETY: the SID header and complete variable-length SID were
                // proven to be inside the initialized token buffer.
                let count_ptr = unsafe { GetSidSubAuthorityCount(sid_ptr) };
                if !count_ptr.is_null() {
                    // SAFETY: count_ptr belongs to the validated SID.
                    let count = unsafe { *count_ptr };
                    if count > 0 {
                        let sub_auth_ptr = {
                            // SAFETY: the validated SID contains `count` DWORD
                            // sub-authorities and this index is in range.
                            unsafe { GetSidSubAuthority(sid_ptr, (count - 1) as u32) }
                        };
                        if !sub_auth_ptr.is_null() {
                            // SAFETY: the Windows helper returned a pointer to
                            // the validated SID's in-buffer sub-authority.
                            let rid = unsafe { *sub_auth_ptr };
                            integrity_level = Some(match rid {
                                0..=0x0FFF => WindowsIntegrityLevel::Untrusted,
                                0x1000..=0x1FFF => WindowsIntegrityLevel::Low,
                                0x2000..=0x2FFF => WindowsIntegrityLevel::Medium,
                                0x3000..=0x3FFF => WindowsIntegrityLevel::High,
                                0x4000..=0x4FFF => WindowsIntegrityLevel::System,
                                0x5000..=0x5FFF => WindowsIntegrityLevel::ProtectedProcess,
                                _ => WindowsIntegrityLevel::ProtectedProcess,
                            });
                        }
                    }
                }
            }
        }

        Ok(WindowsProcessIsolation {
            is_elevated: elevation.TokenIsElevated != 0,
            is_app_container: is_app_container_dword != 0,
            integrity_level,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Enumerate loaded modules and mapped executable files for a process.
#[must_use = "inspect the process modules result"]
pub fn process_modules(pid: u32) -> Result<Vec<WindowsProcessModule>, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use std::mem::size_of;
        use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW,
            TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
        };

        let snapshot = {
            // SAFETY: CreateToolhelp32Snapshot is safe to call with TH32CS_SNAPMODULE.
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) }
        };
        let snapshot = match snapshot {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return Err(WindowsApiError::PermissionDenied),
        };

        struct SnapshotGuard(HANDLE);
        impl Drop for SnapshotGuard {
            fn drop(&mut self) {
                // SAFETY: `self.0` is a valid open ToolHelp snapshot handle.
                let _ = unsafe { CloseHandle(self.0) };
            }
        }
        let _guard = SnapshotGuard(snapshot);

        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };

        let mut modules = Vec::new();
        // SAFETY: `snapshot` is valid, `entry` is initialized with size_of::<MODULEENTRY32W>().
        let mut ok = unsafe { Module32FirstW(snapshot, &mut entry) }.is_ok();
        while ok {
            let name_len = entry
                .szModule
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szModule.len());
            let module_name = String::from_utf16_lossy(&entry.szModule[..name_len]);

            let path_len = entry
                .szExePath
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExePath.len());
            let file_path = String::from_utf16_lossy(&entry.szExePath[..path_len]);

            if modules.len() == MAX_PROCESS_MODULES {
                return Err(WindowsApiError::ResourceLimit);
            }
            modules.push(WindowsProcessModule {
                module_name,
                file_path,
                base_address: entry.modBaseAddr as usize,
                module_size: entry.modBaseSize,
            });

            entry.dwSize = size_of::<MODULEENTRY32W>() as u32;
            // SAFETY: `snapshot` is valid, `entry` is re-initialized with size.
            ok = unsafe { Module32NextW(snapshot, &mut entry) }.is_ok();
        }

        Ok(modules)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Query detailed memory counters (working set, paged/non-paged pool, commit charge) for a process.
#[must_use = "inspect the process memory counters result"]
pub fn process_memory_counters(pid: u32) -> Result<WindowsProcessMemoryCounters, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use std::mem::size_of;
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::{
            PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
        };

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)
            .or_else(|_| open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION))
            .or_else(|_| open_process(pid, PROCESS_QUERY_INFORMATION))?;

        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };

        // SAFETY: `process.0` is a valid open process handle, `counters` is initialized with size.
        let ok = unsafe {
            GetProcessMemoryInfo(
                process.0,
                core::ptr::from_mut(&mut counters).cast(),
                counters.cb,
            )
        };
        if ok.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }

        Ok(WindowsProcessMemoryCounters {
            page_fault_count: counters.PageFaultCount,
            peak_working_set_size_bytes: counters.PeakWorkingSetSize,
            working_set_size_bytes: counters.WorkingSetSize,
            quota_peak_paged_pool_usage_bytes: counters.QuotaPeakPagedPoolUsage,
            quota_paged_pool_usage_bytes: counters.QuotaPagedPoolUsage,
            quota_peak_non_paged_pool_usage_bytes: counters.QuotaPeakNonPagedPoolUsage,
            quota_non_paged_pool_usage_bytes: counters.QuotaNonPagedPoolUsage,
            pagefile_usage_bytes: counters.PagefileUsage,
            peak_pagefile_usage_bytes: counters.PeakPagefileUsage,
            private_usage_bytes: counters.PrivateUsage,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Query the total number of open handles for a process.
#[must_use = "inspect the process handle count result"]
pub fn process_handle_count(pid: u32) -> Result<u32, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            GetProcessHandleCount, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut handle_count = 0u32;
        // SAFETY: `process.0` is a valid open process handle, `handle_count` is a valid writable out-pointer.
        let ok = unsafe { GetProcessHandleCount(process.0, &mut handle_count) };
        if ok.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }
        Ok(handle_count)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Query GDI and USER object counts for a process.
#[must_use = "inspect the process GUI resources result"]
pub fn process_gui_resources(pid: u32) -> Result<WindowsProcessGuiResources, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

        // SAFETY: GetGuiResources is exported by user32.dll with WINAPI calling convention.
        unsafe extern "system" {
            fn GetGuiResources(hProcess: windows::Win32::Foundation::HANDLE, uiFlags: u32) -> u32;
        }
        const GR_GDIOBJECTS: u32 = 0;
        const GR_USEROBJECTS: u32 = 1;

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        // SAFETY: `process.0` is a valid open process handle; GetGuiResources returns 0 on failure or for non-GUI processes.
        let gdi_object_count = unsafe { GetGuiResources(process.0, GR_GDIOBJECTS) };
        // SAFETY: `process.0` is a valid open process handle.
        let user_object_count = unsafe { GetGuiResources(process.0, GR_USEROBJECTS) };

        Ok(WindowsProcessGuiResources {
            gdi_object_count,
            user_object_count,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Enumerate thread counts for all running processes in a single snapshot.
#[must_use = "inspect process thread count mapping"]
pub fn enumerate_all_process_thread_counts()
-> Result<std::collections::HashMap<u32, u32>, WindowsApiError> {
    #[cfg(windows)]
    {
        enumerate_all_process_thread_counts_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn enumerate_all_process_thread_counts_windows()
-> Result<std::collections::HashMap<u32, u32>, WindowsApiError> {
    use std::collections::HashMap;
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };

    // SAFETY: CreateToolhelp32Snapshot with TH32CS_SNAPTHREAD takes a thread snapshot.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    let snapshot = match snapshot {
        Ok(h) if h != INVALID_HANDLE_VALUE => h,
        _ => return Err(WindowsApiError::QueryFailed),
    };

    struct SnapshotGuard(HANDLE);
    impl Drop for SnapshotGuard {
        fn drop(&mut self) {
            // SAFETY: Valid snapshot handle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    let _guard = SnapshotGuard(snapshot);

    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>())
            .map_err(|_| WindowsApiError::QueryFailed)?,
        ..THREADENTRY32::default()
    };

    let mut map: HashMap<u32, u32> = HashMap::with_capacity(512);
    let mut inspected = 0usize;

    // SAFETY: Snapshot handle is valid, entry is sized.
    let mut ok = unsafe { Thread32First(snapshot, &mut entry) }.is_ok();
    while ok {
        inspected = inspected
            .checked_add(1)
            .ok_or(WindowsApiError::ResourceLimit)?;
        if inspected > 1_000_000 {
            return Err(WindowsApiError::ResourceLimit);
        }
        let count = map.entry(entry.th32OwnerProcessID).or_insert(0);
        *count = count.checked_add(1).ok_or(WindowsApiError::ResourceLimit)?;

        entry.dwSize =
            u32::try_from(size_of::<THREADENTRY32>()).map_err(|_| WindowsApiError::QueryFailed)?;
        // SAFETY: Snapshot handle is valid.
        ok = unsafe { Thread32Next(snapshot, &mut entry) }.is_ok();
    }

    Ok(map)
}

/// Query the owner username of a process.
#[must_use = "inspect process user result"]
pub fn query_process_user(pid: u32) -> Result<String, WindowsApiError> {
    #[cfg(windows)]
    {
        query_process_user_windows(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_process_user_windows(pid: u32) -> Result<String, WindowsApiError> {
    if pid == 0 || pid == 4 {
        return Ok("SYSTEM".to_string());
    }
    use std::mem::size_of;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{
        GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION.
    let proc_handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(map_windows_error)?;
    let _proc_guard = ProcessHandle(proc_handle);

    let mut token = HANDLE::default();
    // SAFETY: OpenProcessToken with TOKEN_QUERY.
    let token_ok = unsafe { OpenProcessToken(proc_handle, TOKEN_QUERY, &mut token) }.is_ok();
    if !token_ok || token.is_invalid() {
        return Err(WindowsApiError::PermissionDenied);
    }
    let _token_guard = ProcessHandle(token);

    let mut token_user_buf = [0_u64; 16];
    let mut return_len = 0u32;
    // SAFETY: GetTokenInformation with TokenUser and an aligned writable
    // buffer whose size is passed exactly.
    let info_ok = unsafe {
        GetTokenInformation(
            token,
            windows::Win32::Security::TokenUser,
            Some(token_user_buf.as_mut_ptr().cast()),
            u32::try_from(std::mem::size_of_val(&token_user_buf)).unwrap_or(0),
            &mut return_len,
        )
    }
    .is_ok();

    let token_user_bytes = usize::try_from(return_len).unwrap_or(usize::MAX);
    if !info_ok
        || token_user_bytes < size_of::<TOKEN_USER>()
        || token_user_bytes > std::mem::size_of_val(&token_user_buf)
    {
        return Err(WindowsApiError::QueryFailed);
    }

    let token_user = token_user_buf.as_ptr().cast::<TOKEN_USER>();
    // SAFETY: the returned length proves the outer TOKEN_USER is initialized
    // and token_user_buf is aligned for the repr(C) structure.
    let sid = unsafe { (*token_user).User.Sid };
    if !sid_pointer_is_within_buffer(&token_user_buf, token_user_bytes, sid) {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut name_buf = [0u16; 128];
    let mut name_len = name_buf.len() as u32;
    let mut domain_buf = [0u16; 128];
    let mut domain_len = domain_buf.len() as u32;
    let mut sid_type = SID_NAME_USE::default();

    // SAFETY: LookupAccountSidW with valid buffers.
    let lookup_ok = unsafe {
        LookupAccountSidW(
            windows::core::PCWSTR::null(),
            sid,
            Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
            &mut name_len,
            Some(windows::core::PWSTR(domain_buf.as_mut_ptr())),
            &mut domain_len,
            &mut sid_type,
        )
    }
    .is_ok();

    if !lookup_ok || name_len == 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let name_len = usize::try_from(name_len).map_err(|_| WindowsApiError::QueryFailed)?;
    let domain_len = usize::try_from(domain_len).map_err(|_| WindowsApiError::QueryFailed)?;
    if name_len > name_buf.len() || domain_len > domain_buf.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let user_name = String::from_utf16_lossy(&name_buf[..name_len]);
    Ok(user_name)
}

#[cfg(test)]
#[path = "../../tests/headless/windows_api_process_environment.rs"]
mod tests;
