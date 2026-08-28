//! Extended process telemetry insights (modules, memory counters, handles,
//! isolation, GUI resources, users, environment/cwd).

use super::*;

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
        query_process_environment_windows(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

// ---------------------------------------------------------------------------
// Host-independent layout arithmetic and pure walkers.
// ---------------------------------------------------------------------------

/// Pointer width of the compiled target in bytes; valid for the target
/// process because cross-bitness reads are refused before any walk runs.
const POINTER_BYTES: usize = core::mem::size_of::<usize>();

/// Round a byte offset up to the pointer alignment of the target.
const fn align_to_pointer(value: usize) -> usize {
    (value + POINTER_BYTES - 1) & !(POINTER_BYTES - 1)
}

/// Byte offset of `PebBaseAddress` inside a serialized
/// `PROCESS_BASIC_INFORMATION` (after the 4-byte `ExitStatus`).
const fn basic_information_peb_field() -> usize {
    align_to_pointer(4)
}

/// Serialized `PROCESS_BASIC_INFORMATION` size: `ExitStatus`, `PebBaseAddress`,
/// `AffinityMask`, `BasePriority` (each padded to the pointer alignment),
/// `UniqueProcessId`, `InheritedFromUniqueProcessId`.
const fn basic_information_bytes() -> usize {
    2 * align_to_pointer(4) + 4 * POINTER_BYTES
}

/// Byte offset of `ProcessParameters` inside a serialized `PEB`: three flag
/// bytes padded to the pointer alignment, then `Mutant`, `ImageBaseAddress`,
/// and `Ldr` — pointer-sized each (0x20 on 64-bit, 0x10 on 32-bit).
const fn peb_process_parameters_field() -> usize {
    4 * POINTER_BYTES
}

/// Serialized `UNICODE_STRING` size: two u16 lengths, padding, one pointer
/// (16 bytes on 64-bit, 8 on 32-bit — two pointer widths either way).
const fn unicode_string_bytes() -> usize {
    2 * POINTER_BYTES
}

/// Byte offset of the `Buffer` pointer inside one serialized
/// `UNICODE_STRING`.
const fn unicode_string_buffer_field() -> usize {
    POINTER_BYTES
}

/// Byte offset of `CurrentDirectory.DosPath` inside a serialized
/// `RTL_USER_PROCESS_PARAMETERS`: after the 16 reserved bytes,
/// `ConsoleHandle`, `ConsoleFlags` (both padded together), and the three
/// standard handles (0x38 on 64-bit, 0x24 on 32-bit).
const fn params_current_directory_field() -> usize {
    align_to_pointer(16 + POINTER_BYTES + 4) + 3 * POINTER_BYTES
}

/// Byte offset of the `Environment` pointer: after `CurrentDirectory`
/// (DosPath plus its `Handle`) and the `DllPath`/`ImagePathName`/`CommandLine`
/// `UNICODE_STRING`s (0x80 on 64-bit, 0x48 on 32-bit).
const fn params_environment_field() -> usize {
    params_current_directory_field()
        + unicode_string_bytes()
        + POINTER_BYTES
        + 3 * unicode_string_bytes()
}

/// The environment and cwd facts lifted from one serialized
/// `RTL_USER_PROCESS_PARAMETERS` block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessParametersFields {
    /// `CurrentDirectory.DosPath.Length` in bytes (NUL excluded).
    dos_path_bytes: u16,
    /// `CurrentDirectory.DosPath.Buffer` address in the target.
    dos_path_pointer: usize,
    /// `Environment` block address in the target (0 when the process has no
    /// environment block).
    environment_pointer: usize,
}

/// Parse the cwd/environment fields out of a serialized
/// `RTL_USER_PROCESS_PARAMETERS` block. A block shorter than the fields is a
/// typed `QueryFailed`, never an out-of-bounds read.
fn parse_process_parameters(buffer: &[u8]) -> Result<ProcessParametersFields, WindowsApiError> {
    let dos_path_bytes = read_le_u16(buffer, params_current_directory_field())?;
    Ok(ProcessParametersFields {
        dos_path_bytes,
        dos_path_pointer: read_le_usize(
            buffer,
            params_current_directory_field() + unicode_string_buffer_field(),
        )?,
        environment_pointer: read_le_usize(buffer, params_environment_field())?,
    })
}

/// True when the block's bytes end with the UTF-16 empty-string terminator:
/// the NUL closing the last `KEY=VALUE` string followed by the block's own
/// terminating NUL (four zero bytes on a unit boundary).
fn ends_with_utf16_terminator(bytes: &[u8]) -> bool {
    let units_end = bytes.len() & !1;
    let tail = bytes.get(units_end.saturating_sub(4)..units_end);
    tail == Some(&[0, 0, 0, 0])
}

/// Split a bounded UTF-16 environment block into ordered `KEY=VALUE` pairs.
///
/// Segments without a `=` separator (or with an empty key) are provider noise
/// and skipped. When `terminated` is false the read stopped without the
/// empty-string terminator, so the trailing segment after the last NUL is
/// clipped and counts as truncated; entries beyond the entry cap are counted,
/// not retained. Mirrors the Linux collector's bounds policy.
fn parse_environment_block(bytes: &[u8], terminated: bool) -> (Vec<(String, String)>, u32) {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect();
    let units = units.as_slice();
    let mut segments: Vec<&[u16]> = Vec::new();
    let mut start = 0_usize;
    for (index, &unit) in units.iter().enumerate() {
        if unit == 0 {
            segments.push(&units[start..index]);
            start = index + 1;
        }
    }
    let tail = &units[start.min(units.len())..];
    let mut truncated_count = 0_u32;
    // The tail is complete only when the block terminator proved the whole
    // block read, or the bytes themselves closed the segment with a NUL.
    if terminated || units.last() == Some(&0) {
        if !tail.is_empty() {
            segments.push(tail);
        }
    } else if !tail.is_empty() {
        // A clipped final entry: dropped and reported, never half-parsed.
        truncated_count += 1;
    }

    let mut entries = Vec::new();
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        let text = String::from_utf16_lossy(segment);
        let Some((key, value)) = text.split_once('=') else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if entries.len() >= MAX_PROCESS_ENVIRONMENT_ENTRIES {
            truncated_count = truncated_count.saturating_add(1);
            continue;
        }
        entries.push((key.to_string(), value.to_string()));
    }
    (entries, truncated_count)
}

fn read_le_u16(buffer: &[u8], offset: usize) -> Result<u16, WindowsApiError> {
    let bytes = buffer
        .get(offset..offset.saturating_add(2))
        .ok_or(WindowsApiError::QueryFailed)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_usize(buffer: &[u8], offset: usize) -> Result<usize, WindowsApiError> {
    let end = offset
        .checked_add(POINTER_BYTES)
        .ok_or(WindowsApiError::QueryFailed)?;
    let bytes = buffer
        .get(offset..end)
        .ok_or(WindowsApiError::QueryFailed)?;
    let mut raw = [0_u8; core::mem::size_of::<usize>()];
    raw.copy_from_slice(bytes);
    Ok(usize::from_le_bytes(raw))
}

// ---------------------------------------------------------------------------
// Windows FFI section.
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn query_process_environment_windows(
    pid: u32,
) -> Result<WindowsProcessEnvironmentBlock, WindowsApiError> {
    use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, IsWow64Process, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }

    // Same-user processes answer this open; protected or higher-integrity
    // owners refuse it as a typed permission failure, and a vanished pid is
    // an identity change — the same access semantics as the other insights.
    let process = open_process(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;

    // Cross-bitness guard: a WOW64 target has a second PEB whose layout
    // differs from this build's, so the walk refuses to guess it. Equal WOW64
    // status on both sides implies equal pointer widths in every host mix
    // (64/64, 32/32, and 32-bit Windows where both answers are false).
    let mut self_wow64 = windows::core::BOOL::default();
    let mut target_wow64 = windows::core::BOOL::default();
    // SAFETY: `GetCurrentProcess` returns the current pseudo-handle, and
    // `self_wow64` is a matching writable output for this synchronous call.
    unsafe { IsWow64Process(GetCurrentProcess(), &mut self_wow64) }
        .map_err(|_| WindowsApiError::QueryFailed)?;
    // SAFETY: `process.0` is an owned valid process handle, and
    // `target_wow64` is a matching writable output.
    unsafe { IsWow64Process(process.0, &mut target_wow64) }
        .map_err(|_| WindowsApiError::QueryFailed)?;
    if self_wow64.as_bool() != target_wow64.as_bool() {
        return Err(WindowsApiError::Unsupported);
    }

    let mut basic_information = [0_u8; basic_information_bytes()];
    let mut returned_bytes = 0_u32;
    // SAFETY: `process.0` is an owned valid handle, `basic_information` is a
    // writable allocation whose length is passed in the matching parameter,
    // and `returned_bytes` receives the kernel's byte count; the query opens
    // no handles and retains no caller pointers.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessBasicInformation,
            basic_information.as_mut_ptr().cast(),
            u32::try_from(basic_information.len()).map_err(|_| WindowsApiError::QueryFailed)?,
            &mut returned_bytes,
        )
    };
    if status.0 == STATUS_ACCESS_DENIED {
        return Err(WindowsApiError::PermissionDenied);
    }
    if status.0 < 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    let returned_bytes =
        usize::try_from(returned_bytes).map_err(|_| WindowsApiError::QueryFailed)?;
    let minimum_returned = basic_information_peb_field()
        .checked_add(POINTER_BYTES)
        .ok_or(WindowsApiError::QueryFailed)?;
    if returned_bytes < minimum_returned || returned_bytes > basic_information.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let peb_base = read_le_usize(&basic_information, basic_information_peb_field())?;
    if peb_base == 0 {
        // No PEB means no native environment source; an empty "healthy"
        // result would fabricate one.
        return Err(WindowsApiError::QueryFailed);
    }

    let mut peb_bytes = [0_u8; peb_process_parameters_field() + POINTER_BYTES];
    if read_target_bytes(process.0, peb_base, &mut peb_bytes)? != peb_bytes.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let parameters = read_le_usize(&peb_bytes, peb_process_parameters_field())?;
    if parameters == 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut parameters_bytes = vec![0_u8; params_environment_field() + POINTER_BYTES];
    if read_target_bytes(process.0, parameters, &mut parameters_bytes)? != parameters_bytes.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    let fields = parse_process_parameters(&parameters_bytes)?;

    let working_directory =
        read_working_directory(process.0, fields.dos_path_bytes, fields.dos_path_pointer);

    let (block, terminated) = read_environment_block(process.0, fields.environment_pointer)?;
    if block.is_empty() && !terminated {
        // Nothing of the block could be read, so no entry count — not even
        // zero — is provable; a fabricated empty environment is worse than
        // the typed failure.
        return Err(WindowsApiError::QueryFailed);
    }
    let (entries, truncated_count) = parse_environment_block(&block, terminated);

    Ok(WindowsProcessEnvironmentBlock {
        working_directory,
        entries,
        truncated_count,
    })
}

/// Read the target's `CurrentDirectory.DosPath` string. Every guard failure —
/// null pointer, empty/odd/overlong length, or a short read — is an honest
/// `None` cwd, never a fabricated or truncated path.
#[cfg(windows)]
fn read_working_directory(
    process: windows::Win32::Foundation::HANDLE,
    dos_path_bytes: u16,
    dos_path_pointer: usize,
) -> Option<String> {
    let byte_len = usize::from(dos_path_bytes);
    if dos_path_pointer == 0 || byte_len == 0 || byte_len % 2 != 0 {
        return None;
    }
    if byte_len > MAX_WORKING_DIRECTORY_BYTES {
        return None;
    }
    let mut buffer = vec![0_u8; byte_len];
    let read = read_target_bytes(process, dos_path_pointer, &mut buffer).ok()?;
    if read != byte_len {
        return None;
    }
    let units = buffer
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect::<Vec<_>>();
    Some(String::from_utf16_lossy(&units))
}

/// Read the UTF-16 environment block in bounded chunks until the
/// empty-string terminator is seen, the byte budget runs out, or the reads
/// stop succeeding. The second return value says whether the terminator was
/// proven; without it the caller treats the tail as clipped.
#[cfg(windows)]
fn read_environment_block(
    process: windows::Win32::Foundation::HANDLE,
    environment_pointer: usize,
) -> Result<(Vec<u8>, bool), WindowsApiError> {
    let mut block = Vec::new();
    if environment_pointer == 0 {
        // A null Environment pointer is the process's own fact: it has no
        // environment block, which is a provable empty, not a read failure.
        return Ok((block, true));
    }
    let mut address = environment_pointer;
    while block.len() < MAX_PROCESS_ENVIRONMENT_BYTES {
        let want = ENVIRONMENT_READ_CHUNK_BYTES.min(MAX_PROCESS_ENVIRONMENT_BYTES - block.len());
        let mut chunk = [0_u8; ENVIRONMENT_READ_CHUNK_BYTES];
        // A failed chunk means the walk reached past the block's committed
        // pages (or the block is unreadable); the bytes kept so far are the
        // honest bounded prefix, marked un-terminated so the parser drops a
        // clipped tail instead of trusting it.
        let Ok(read) = read_target_bytes(process, address, &mut chunk[..want]) else {
            return Ok((block, false));
        };
        if read == 0 {
            return Ok((block, false));
        }
        if read > want || !read.is_multiple_of(2) {
            return Err(WindowsApiError::QueryFailed);
        }
        block.extend_from_slice(&chunk[..read]);
        address = address
            .checked_add(read)
            .ok_or(WindowsApiError::QueryFailed)?;
        if ends_with_utf16_terminator(&block) {
            return Ok((block, true));
        }
    }
    Ok((block, false))
}

/// Read exactly `buffer.len()`-at-most bytes from the target's address space.
/// `Ok(n)` with `n < buffer.len()` is a short read; `Err` is a failed read.
#[cfg(windows)]
fn read_target_bytes(
    process: windows::Win32::Foundation::HANDLE,
    address: usize,
    buffer: &mut [u8],
) -> Result<usize, WindowsApiError> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

    if buffer.is_empty() {
        return Ok(0);
    }
    let mut read = 0_usize;
    // SAFETY: `process` is an owned valid handle carrying PROCESS_VM_READ;
    // `address` came from the target's own PEB structures; `buffer` is a
    // writable allocation of exactly `buffer.len()` bytes outliving this
    // synchronous call, and `read` receives the kernel's byte count.
    let ok = unsafe {
        ReadProcessMemory(
            process,
            core::ptr::with_exposed_provenance(address),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            Some(&mut read),
        )
    };
    if ok.is_err() {
        return Err(WindowsApiError::QueryFailed);
    }
    if read > buffer.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(read)
}

#[cfg(test)]
#[path = "../../tests/headless/windows_api_process_environment.rs"]
mod tests;
