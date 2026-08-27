//! Exact-process identity, termination, elevation and priority primitives.
//!
//! The adapter must never authorize a destructive action from PID alone. This
//! module opens the process with the minimum query/terminate rights, reads the
//! kernel creation time, compares it with the frozen identity, and terminates
//! that same owned handle. No raw handle crosses the public API.

use super::WindowsApiError;

/// Windows process priority classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessPriorityClass {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

/// Windows security token integrity levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsIntegrityLevel {
    Untrusted,
    Low,
    Medium,
    High,
    System,
    ProtectedProcess,
}

/// Windows process security context and isolation facts.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WindowsProcessIsolation {
    pub is_elevated: bool,
    pub is_app_container: bool,
    pub integrity_level: Option<WindowsIntegrityLevel>,
}

/// A loaded module (DLL/executable) mapped into a process address space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsProcessModule {
    pub module_name: String,
    pub file_path: String,
    pub base_address: usize,
    pub module_size: u32,
}

/// Basic snapshot of a Windows thread from ToolHelp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsThreadInfo {
    pub tid: u32,
    pub base_priority: i32,
    pub delta_priority: i32,
}

/// Extended per-thread facts: the `GetThreadDescription` name and cumulative
/// CPU time from `GetThreadTimes`. Both fields are honest absences (`None`)
/// when the documented API yields nothing for that thread.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsThreadDetail {
    pub tid: u32,
    pub name: Option<String>,
    pub cpu_time_secs: Option<f64>,
}

/// Detailed memory usage counters from K32GetProcessMemoryInfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WindowsProcessMemoryCounters {
    pub page_fault_count: u32,
    pub peak_working_set_size_bytes: usize,
    pub working_set_size_bytes: usize,
    pub quota_peak_paged_pool_usage_bytes: usize,
    pub quota_paged_pool_usage_bytes: usize,
    pub quota_peak_non_paged_pool_usage_bytes: usize,
    pub quota_non_paged_pool_usage_bytes: usize,
    pub pagefile_usage_bytes: usize,
    pub peak_pagefile_usage_bytes: usize,
    pub private_usage_bytes: usize,
}

/// GUI and GDI/USER object counts from GetGuiResources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WindowsProcessGuiResources {
    pub gdi_object_count: u32,
    pub user_object_count: u32,
}

#[must_use = "inspect the native process creation-time result"]
pub fn process_creation_time_100ns(pid: u32) -> Result<u64, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        let process = open_process(
            pid,
            windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
        )?;
        creation_time(&process)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Check if a process is running with elevated (administrator) privileges.
#[must_use = "inspect the process elevation result"]
pub fn process_is_elevated(pid: u32) -> Result<bool, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use std::mem::size_of;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{
            GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
        };
        use windows::Win32::System::Threading::{
            OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut token = HANDLE::default();
        let opened = {
            // SAFETY: `process` is a valid open process handle and `token` is a writable output.
            unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) }.is_ok()
        };
        if !opened {
            return Err(WindowsApiError::PermissionDenied);
        }
        struct TokenGuard(HANDLE);
        impl Drop for TokenGuard {
            fn drop(&mut self) {
                // SAFETY: `token` is an owned handle from OpenProcessToken.
                let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
            }
        }
        let _guard = TokenGuard(token);

        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        let queried = {
            // SAFETY: `token` is valid, `elevation` is writable with matching size.
            unsafe {
                GetTokenInformation(
                    token,
                    TokenElevation,
                    Some(std::ptr::from_mut(&mut elevation).cast()),
                    u32::try_from(size_of::<TOKEN_ELEVATION>())
                        .map_err(|_| WindowsApiError::QueryFailed)?,
                    &mut return_length,
                )
            }
            .is_ok()
        };

        if !queried {
            return Err(WindowsApiError::QueryFailed);
        }

        Ok(elevation.TokenIsElevated != 0)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Query the priority class of a process.
#[must_use = "inspect the process priority result"]
pub fn process_priority(pid: u32) -> Result<ProcessPriorityClass, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, GetPriorityClass,
            HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
            PROCESS_QUERY_LIMITED_INFORMATION, REALTIME_PRIORITY_CLASS,
        };

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let class = {
            // SAFETY: `process` is a valid handle with PROCESS_QUERY_LIMITED_INFORMATION.
            unsafe { GetPriorityClass(process.0) }
        };

        match class {
            x if x == IDLE_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::Idle),
            x if x == BELOW_NORMAL_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::BelowNormal),
            x if x == NORMAL_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::Normal),
            x if x == ABOVE_NORMAL_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::AboveNormal),
            x if x == HIGH_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::High),
            x if x == REALTIME_PRIORITY_CLASS.0 => Ok(ProcessPriorityClass::Realtime),
            _ => Err(WindowsApiError::QueryFailed),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Set the priority class of a process after verifying identity.
#[must_use = "inspect the process priority mutation result"]
pub fn set_process_priority_exact(
    pid: u32,
    expected_creation_time_100ns: u64,
    priority: ProcessPriorityClass,
) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 || expected_creation_time_100ns == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
            IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_ACCESS_RIGHTS,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, REALTIME_PRIORITY_CLASS,
            SetPriorityClass,
        };

        let native_class = match priority {
            ProcessPriorityClass::Idle => IDLE_PRIORITY_CLASS,
            ProcessPriorityClass::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            ProcessPriorityClass::Normal => NORMAL_PRIORITY_CLASS,
            ProcessPriorityClass::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            ProcessPriorityClass::High => HIGH_PRIORITY_CLASS,
            ProcessPriorityClass::Realtime => REALTIME_PRIORITY_CLASS,
        };

        let rights =
            PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_LIMITED_INFORMATION.0 | PROCESS_SET_INFORMATION.0);
        let process = open_process(pid, rights)?;
        let actual = creation_time(&process)?;
        if actual != expected_creation_time_100ns {
            return Err(WindowsApiError::IdentityChanged);
        }

        let succeeded = {
            // SAFETY: `process` is a verified matching handle and `native_class` is a valid priority class.
            unsafe { SetPriorityClass(process.0, native_class) }.is_ok()
        };

        if succeeded {
            Ok(())
        } else {
            Err(WindowsApiError::PermissionDenied)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, expected_creation_time_100ns, priority);
        Err(WindowsApiError::Unsupported)
    }
}

/// Retrieve the CPU affinity mask for a process, returned as active CPU core indices.
#[must_use = "inspect the process affinity result"]
pub fn process_affinity(pid: u32) -> Result<Vec<u32>, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            GetProcessAffinityMask, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut proc_mask: usize = 0;
        let mut sys_mask: usize = 0;
        let ok = {
            // SAFETY: `process.0` is a valid handle and `proc_mask`/`sys_mask` are valid writable outputs.
            unsafe { GetProcessAffinityMask(process.0, &mut proc_mask, &mut sys_mask) }.is_ok()
        };
        if !ok {
            return Err(WindowsApiError::QueryFailed);
        }
        let mut cpus = Vec::new();
        for i in 0..usize::BITS {
            if (proc_mask & (1usize << i)) != 0 {
                cpus.push(i);
            }
        }
        Ok(cpus)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Set the CPU affinity mask for a process after revalidating its kernel creation time.
#[must_use = "inspect the process set-affinity result"]
pub fn set_process_affinity_exact(
    pid: u32,
    expected_creation_time_100ns: u64,
    cpus: &[u32],
) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 || expected_creation_time_100ns == 0 || cpus.is_empty() {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
            SetProcessAffinityMask,
        };

        let rights =
            PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_LIMITED_INFORMATION.0 | PROCESS_SET_INFORMATION.0);
        let process = open_process(pid, rights)?;
        let actual = creation_time(&process)?;
        if actual != expected_creation_time_100ns {
            return Err(WindowsApiError::IdentityChanged);
        }

        let mut mask: usize = 0;
        for &cpu in cpus {
            if cpu < usize::BITS {
                mask |= 1usize << cpu;
            }
        }
        if mask == 0 {
            return Err(WindowsApiError::InvalidInput);
        }

        let ok = {
            // SAFETY: `process` owns a valid handle with PROCESS_SET_INFORMATION and creation time revalidated.
            unsafe { SetProcessAffinityMask(process.0, mask) }.is_ok()
        };
        if ok {
            Ok(())
        } else {
            Err(WindowsApiError::PermissionDenied)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, expected_creation_time_100ns, cpus);
        Err(WindowsApiError::Unsupported)
    }
}

/// Enumerate active threads for a specific PID using ToolHelp32.
#[must_use = "inspect the process threads result"]
pub fn process_threads(pid: u32) -> Result<Vec<WindowsThreadInfo>, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use std::mem::size_of;
        use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        };

        let snapshot = {
            // SAFETY: CreateToolhelp32Snapshot is safe to call with TH32CS_SNAPTHREAD and pid 0 to capture threads.
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        };
        let snapshot = match snapshot {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return Err(WindowsApiError::QueryFailed),
        };

        struct SnapshotGuard(HANDLE);
        impl Drop for SnapshotGuard {
            fn drop(&mut self) {
                // SAFETY: `self.0` is a valid open ToolHelp snapshot handle.
                let _ = unsafe { CloseHandle(self.0) };
            }
        }
        let _guard = SnapshotGuard(snapshot);

        let mut entry = THREADENTRY32 {
            dwSize: u32::try_from(size_of::<THREADENTRY32>())
                .map_err(|_| WindowsApiError::QueryFailed)?,
            ..THREADENTRY32::default()
        };

        let mut threads = Vec::new();
        const MAX_THREADS_PER_PROCESS: usize = 4096;

        let mut has_next = {
            // SAFETY: `snapshot` is a valid handle and `entry` is properly initialized with dwSize.
            unsafe { Thread32First(snapshot, &mut entry) }.is_ok()
        };

        while has_next {
            if entry.th32OwnerProcessID == pid {
                threads.push(WindowsThreadInfo {
                    tid: entry.th32ThreadID,
                    base_priority: entry.tpBasePri,
                    delta_priority: entry.tpDeltaPri,
                });
                if threads.len() >= MAX_THREADS_PER_PROCESS {
                    break;
                }
            }
            has_next = {
                // SAFETY: `snapshot` is a valid handle and `entry` is properly initialized with dwSize.
                unsafe { Thread32Next(snapshot, &mut entry) }.is_ok()
            };
        }

        Ok(threads)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Query extended thread facts (name via `GetThreadDescription`, cumulative
/// CPU time via `GetThreadTimes`) for every thread of a process. Threads that
/// exit between the ToolHelp snapshot and the per-thread open are skipped; a
/// failed per-thread fact is `None`, never a fabricated value.
#[must_use = "inspect the process thread details result"]
pub fn query_process_thread_details(pid: u32) -> Result<Vec<WindowsThreadDetail>, WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::Foundation::{CloseHandle, FILETIME, HLOCAL, LocalFree};
        use windows::Win32::System::Threading::{
            GetThreadDescription, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
        };

        struct ThreadHandle(windows::Win32::Foundation::HANDLE);
        impl Drop for ThreadHandle {
            fn drop(&mut self) {
                // SAFETY: the handle was returned by OpenThread and is owned
                // exclusively by this RAII guard; Drop runs at most once.
                let _ = unsafe { CloseHandle(self.0) };
            }
        }

        let threads = process_threads(pid)?;
        let mut details = Vec::with_capacity(threads.len());
        for thread in threads {
            let handle = {
                // SAFETY: the tid comes from a live ToolHelp snapshot entry and
                // the returned handle is immediately owned by the RAII guard.
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, thread.tid) }
            };
            let Ok(handle) = handle else {
                // The thread exited between the snapshot and this open; it has
                // no name or time left to report, so the row is skipped rather
                // than fabricated.
                continue;
            };
            let handle = ThreadHandle(handle);

            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let times_ok = {
                // SAFETY: all four FILETIME values are valid writable outputs
                // and the thread handle remains owned for this synchronous call.
                unsafe {
                    GetThreadTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user)
                }
            }
            .is_ok();
            let cpu_time_secs = times_ok.then(|| {
                let kernel_100ns =
                    (u64::from(kernel.dwHighDateTime) << 32) | u64::from(kernel.dwLowDateTime);
                let user_100ns =
                    (u64::from(user.dwHighDateTime) << 32) | u64::from(user.dwLowDateTime);
                (kernel_100ns + user_100ns) as f64 / 1e7
            });

            let name = {
                // SAFETY: the handle is valid and owns THREAD_QUERY_LIMITED_INFORMATION,
                // which is the documented access GetThreadDescription requires.
                match unsafe { GetThreadDescription(handle.0) } {
                    Ok(description) => {
                        let decoded = decode_thread_description(description.as_ptr());
                        // SAFETY: GetThreadDescription allocates its buffer with
                        // the local allocator; the documented contract requires
                        // exactly one LocalFree, owned here.
                        unsafe { LocalFree(Some(HLOCAL(description.as_ptr().cast()))) };
                        decoded
                    }
                    Err(_) => None,
                }
            };

            details.push(WindowsThreadDetail {
                tid: thread.tid,
                name,
                cpu_time_secs,
            });
        }

        Ok(details)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
const MAX_THREAD_DESCRIPTION_UTF16: usize = 512;

#[cfg(windows)]
fn decode_thread_description(pointer: *const u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let length = {
        let mut length = 0_usize;
        // SAFETY: GetThreadDescription documents a NUL-terminated buffer; the
        // hard bound keeps a malformed allocation from running off the end.
        unsafe {
            while *pointer.add(length) != 0 && length < MAX_THREAD_DESCRIPTION_UTF16 {
                length += 1;
            }
        }
        length
    };
    if length == 0 {
        return None;
    }
    let wide = {
        // SAFETY: the loop above proved `length` code units readable.
        unsafe { std::slice::from_raw_parts(pointer, length) }
    };
    String::from_utf16(wide).ok()
}

/// Suspend every thread of a process through the documented per-thread path:
/// a ToolHelp32 thread snapshot of the pid, `OpenThread(THREAD_SUSPEND_RESUME)`
/// per thread, then `SuspendThread`. Returns the number of threads suspended.
///
/// The undocumented `NtSuspendProcess` is deliberately not used (ADR-018): the
/// two mechanisms are not interchangeable and per-thread suspend is the one
/// Windows documents.
#[must_use = "inspect the native thread suspension result"]
pub fn suspend_process_threads(pid: u32) -> Result<u32, WindowsApiError> {
    #[cfg(windows)]
    {
        set_process_threads_suspended(pid, true)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

/// Resume every thread of a process through the documented per-thread path
/// (`OpenThread(THREAD_SUSPEND_RESUME)` + `ResumeThread`). Returns the number
/// of threads resumed.
#[must_use = "inspect the native thread resumption result"]
pub fn resume_process_threads(pid: u32) -> Result<u32, WindowsApiError> {
    #[cfg(windows)]
    {
        set_process_threads_suspended(pid, false)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn set_process_threads_suspended(pid: u32, suspend: bool) -> Result<u32, WindowsApiError> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME,
    };

    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }

    struct ThreadGuard(HANDLE);
    impl Drop for ThreadGuard {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by OpenThread and is owned
            // exclusively by this RAII guard; Drop runs at most once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    // A live Windows process always owns at least one thread, so an empty
    // snapshot means the target exited (or its pid was reused) — the same
    // classification the exact-identity helpers give a vanished process.
    let threads = process_threads(pid)?;
    if threads.is_empty() {
        return Err(WindowsApiError::IdentityChanged);
    }

    let mut suspended_count = 0_u32;
    let mut access_denied = false;
    for thread in threads {
        let handle = {
            // SAFETY: the tid comes from a live ToolHelp snapshot entry and the
            // returned handle is immediately owned by the RAII guard.
            unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, thread.tid) }
        };
        let handle = match handle {
            Ok(handle) => ThreadGuard(handle),
            Err(error) => {
                // A thread that exited between the snapshot and this open fails
                // the open; only a denial against a live thread is remembered,
                // because that is the one failure a total failure must surface.
                if map_windows_error(error) == WindowsApiError::PermissionDenied {
                    access_denied = true;
                }
                continue;
            }
        };
        let previous_suspend_count = if suspend {
            // SAFETY: the owned handle carries THREAD_SUSPEND_RESUME; the
            // previous suspend count is intentionally ignored.
            unsafe { SuspendThread(handle.0) }
        } else {
            // SAFETY: the owned handle carries THREAD_SUSPEND_RESUME; the
            // previous suspend count is intentionally ignored.
            unsafe { ResumeThread(handle.0) }
        };
        // Both APIs report failure as 0xFFFFFFFF; any other value means the
        // transition was applied.
        if previous_suspend_count != u32::MAX {
            suspended_count += 1;
        }
    }

    if suspended_count > 0 {
        return Ok(suspended_count);
    }
    if access_denied {
        return Err(WindowsApiError::PermissionDenied);
    }
    Err(WindowsApiError::QueryFailed)
}

#[must_use = "inspect the native process termination result"]
pub fn terminate_process_exact(
    pid: u32,
    expected_creation_time_100ns: u64,
) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        if pid == 0 || expected_creation_time_100ns == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        use windows::Win32::System::Threading::{
            PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };
        let rights =
            PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_LIMITED_INFORMATION.0 | PROCESS_TERMINATE.0);
        let process = open_process(pid, rights)?;
        let actual = creation_time(&process)?;
        if actual != expected_creation_time_100ns {
            return Err(WindowsApiError::IdentityChanged);
        }
        {
            // SAFETY: `process` owns a valid handle whose creation time was
            // revalidated immediately above; terminating this exact handle
            // cannot target a PID-reused process.
            unsafe { windows::Win32::System::Threading::TerminateProcess(process.0, 1) }
        }
        .map_err(map_windows_error)
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, expected_creation_time_100ns);
        Err(WindowsApiError::Unsupported)
    }
}

mod insights;
pub use insights::*;

#[cfg(windows)]
struct ProcessHandle(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is returned by OpenProcess and is owned by this
        // guard; no other code can close it through the public API.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
fn open_process(
    pid: u32,
    rights: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS,
) -> Result<ProcessHandle, WindowsApiError> {
    let handle = {
        // SAFETY: `pid` is validated non-zero, the requested rights are a
        // fixed minimum set, and the returned handle is immediately owned by
        // the RAII guard.
        unsafe { windows::Win32::System::Threading::OpenProcess(rights, false, pid) }
    }
    .map_err(map_windows_error)?;
    Ok(ProcessHandle(handle))
}

#[cfg(windows)]
fn creation_time(process: &ProcessHandle) -> Result<u64, WindowsApiError> {
    use windows::Win32::Foundation::FILETIME;

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    {
        // SAFETY: all four FILETIME values are valid writable outputs, and
        // the process handle remains owned and alive for this synchronous call.
        unsafe {
            windows::Win32::System::Threading::GetProcessTimes(
                process.0,
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        }
    }
    .map_err(map_windows_error)?;
    let value = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    (value != 0)
        .then_some(value)
        .ok_or(WindowsApiError::QueryFailed)
}

#[cfg(windows)]
fn map_windows_error(error: windows::core::Error) -> WindowsApiError {
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};

    let code = error.code();
    if code == ERROR_ACCESS_DENIED.to_hresult() {
        WindowsApiError::PermissionDenied
    } else if code == ERROR_INVALID_PARAMETER.to_hresult() {
        WindowsApiError::IdentityChanged
    } else {
        WindowsApiError::QueryFailed
    }
}

#[cfg(all(test, windows))]
#[path = "../tests/headless/process.rs"]
mod tests;
