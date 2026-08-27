//! Session-scoped process resource limits via job objects.
//!
//! Windows 8+ nested jobs let an unprivileged process assign a process it
//! did not create (opened with `PROCESS_SET_QUOTA | PROCESS_TERMINATE`) into
//! a job of its own and set limits through
//! `SetInformationJobObject(JobObjectExtendedLimitInformation)`. Three
//! constraints are structural and must not be papered over:
//!
//! 1. Limits only ever *tighten* across a job hierarchy — a request that
//!    would loosen a parent job's limit fails and surfaces as a typed
//!    failure, never a silent partial apply.
//! 2. An anonymous job owned by another creator cannot be opened or edited;
//!    this module only ever manages jobs it created itself.
//! 3. The job lives exactly as long as this process keeps its handle: the
//!    limits are **session-scoped** (they evaporate when the app exits),
//!    unlike persistent Linux cgroup writes. Callers must present them that
//!    way; `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is never set.
//!
//! The internal registry keys jobs by `(pid, creation 100ns token)` so a
//! reused PID can never release or re-limit another process's job.

use crate::WindowsApiError;

/// Limits applied to one process through a boundary-owned job. `None`
/// fields leave that dimension untouched on re-apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsJobLimitRequest {
    /// Commit limit for the whole job in bytes
    /// (`JOB_OBJECT_LIMIT_JOB_MEMORY`).
    pub memory_limit_bytes: Option<u64>,
    /// Maximum process count in the job (`JOB_OBJECT_LIMIT_ACTIVE_PROCESS`).
    pub process_count_limit: Option<u32>,
    /// CPU rate as a whole-number percentage 1..100
    /// (`JOB_OBJECT_CPU_RATE_CONTROL_ENABLE`, weight-based rate). `None`
    /// leaves CPU untouched.
    pub cpu_rate_percent: Option<u32>,
}

#[cfg(windows)]
use std::collections::HashMap;

#[cfg(windows)]
use std::sync::{LazyLock, Mutex};

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

/// Owns one boundary-created job handle; dropping the last registry entry
/// destroys the job object and evaporates its limits (constraint 3).
#[cfg(windows)]
struct JobHandleGuard(HANDLE);

// SAFETY: a Win32 job handle is an owned kernel object reference, not a
// pointer into Rust memory; `CloseHandle` is documented as thread-safe, so
// moving the guard to whatever thread drops the registry entry is sound.
#[cfg(windows)]
unsafe impl Send for JobHandleGuard {}

#[cfg(windows)]
impl Drop for JobHandleGuard {
    fn drop(&mut self) {
        // SAFETY: the handle was created by `CreateJobObjectW` in this module
        // and is owned exclusively by the guard; Drop runs exactly once.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
static JOB_REGISTRY: LazyLock<Mutex<HashMap<(u32, u64), JobHandleGuard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Assign the target to a boundary-owned job (nested under any existing
/// jobs) and apply the limits. Re-applying to the same live target replaces
/// the boundary-owned job's limits.
#[cfg(windows)]
pub fn apply_process_job_limits(
    pid: u32,
    creation_filetime_100ns: u64,
    limits: &WindowsJobLimitRequest,
) -> Result<(), WindowsApiError> {
    use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
        PROCESS_TERMINATE,
    };

    if pid == 0 || creation_filetime_100ns == 0 {
        return Err(WindowsApiError::InvalidInput);
    }
    if let Some(percent) = limits.cpu_rate_percent
        && !(1..=100).contains(&percent)
    {
        return Err(WindowsApiError::InvalidInput);
    }

    // The minimum documented rights for job assignment plus the query right
    // needed to verify the frozen identity on this exact handle.
    let rights = PROCESS_ACCESS_RIGHTS(
        PROCESS_QUERY_LIMITED_INFORMATION.0 | PROCESS_SET_QUOTA.0 | PROCESS_TERMINATE.0,
    );
    // SAFETY: `pid` is validated non-zero; the returned handle is owned by
    // `ProcessGuard` and never crosses the module boundary.
    let process = unsafe { OpenProcess(rights, false, pid) }.map_err(open_process_failure)?;
    struct ProcessGuard(HANDLE);
    impl Drop for ProcessGuard {
        fn drop(&mut self) {
            // SAFETY: the handle is owned by this guard and closed once.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }
    let process = ProcessGuard(process);

    // Acting on the verified handle itself removes the PID-reuse window: the
    // assignment below can only ever affect the process whose creation token
    // was checked here.
    if process_creation_token(process.0)? != creation_filetime_100ns {
        return Err(WindowsApiError::IdentityChanged);
    }

    let key = (pid, creation_filetime_100ns);
    let mut registry = JOB_REGISTRY
        .lock()
        .map_err(|_| WindowsApiError::QueryFailed)?;

    // Re-apply replaces the limits on the already-owned job (read-modify-
    // write: absent dimensions keep their current value).
    if let Some(existing) = registry.get(&key) {
        return apply_limits_to_job(existing.0, limits);
    }

    // SAFETY: unnamed job with no security-attribute pointer; the returned
    // handle is either registered below or closed on the error path.
    let job = unsafe { CreateJobObjectW(None, None) }.map_err(|_| WindowsApiError::QueryFailed)?;
    let outcome = (|| -> Result<(), WindowsApiError> {
        apply_limits_to_job(job, limits)?;
        // SAFETY: both handles are live and owned by this call frame.
        unsafe { AssignProcessToJobObject(job, process.0) }.map_err(assign_failure)
    })();
    match outcome {
        Ok(()) => {
            registry.insert(key, JobHandleGuard(job));
            Ok(())
        }
        Err(error) => {
            // SAFETY: the job handle was created above and is not yet
            // registered anywhere else.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
            Err(error)
        }
    }
}

/// Drop the boundary-owned job tracking the target (limits evaporate with
/// the handle — see the module constraints). Returns whether a job for that
/// exact `(pid, creation token)` pair was actually released; `false` is a
/// real absence, not an error.
#[cfg(windows)]
pub fn clear_process_job_limits(
    pid: u32,
    creation_filetime_100ns: u64,
) -> Result<bool, WindowsApiError> {
    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }
    let mut registry = JOB_REGISTRY
        .lock()
        .map_err(|_| WindowsApiError::QueryFailed)?;
    // Removing the guard drops the last boundary-owned handle: the job is
    // destroyed and its limits evaporate, while the target survives because
    // KILL_ON_JOB_CLOSE is never set.
    Ok(registry.remove(&(pid, creation_filetime_100ns)).is_some())
}

/// Read the kernel creation-time token from an owned process handle,
/// composed exactly like the boundary's query-side primitive.
#[cfg(windows)]
fn process_creation_token(process: HANDLE) -> Result<u64, WindowsApiError> {
    use windows::Win32::Foundation::FILETIME;

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the handle is owned by the caller for this synchronous call
    // and all four FILETIME values are valid writable outputs.
    unsafe {
        windows::Win32::System::Threading::GetProcessTimes(
            process,
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    let value = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    (value != 0)
        .then_some(value)
        .ok_or(WindowsApiError::QueryFailed)
}

/// Apply the requested dimensions to one boundary-owned job. Dimensions the
/// request leaves `None` keep their current value: the current extended
/// limits are read back and merged, so a re-apply never clears a dimension
/// it does not mention.
#[cfg(windows)]
fn apply_limits_to_job(
    job: HANDLE,
    limits: &WindowsJobLimitRequest,
) -> Result<(), WindowsApiError> {
    use std::mem::size_of;

    use windows::Win32::System::JobObjects::{
        JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectCpuRateControlInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    };

    let extended_size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
        .map_err(|_| WindowsApiError::ResourceLimit)?;
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // SAFETY: the job handle is owned by the registry entry; `info` is a
    // correctly sized writable buffer for this information class and no
    // pointer escapes the synchronous call.
    unsafe {
        QueryInformationJobObject(
            Some(job),
            JobObjectExtendedLimitInformation,
            std::ptr::from_mut(&mut info).cast(),
            extended_size,
            None,
        )
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;

    if let Some(bytes) = limits.memory_limit_bytes {
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = bytes as usize;
    }
    if let Some(count) = limits.process_count_limit {
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = count;
    }
    // SAFETY: the pointer and byte count describe the live `info` value for
    // the duration of this synchronous call.
    unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&info).cast(),
            extended_size,
        )
    }
    .map_err(set_information_failure)?;

    if let Some(percent) = limits.cpu_rate_percent {
        let mut rate = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        rate.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE;
        let rate_size = u32::try_from(size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>())
            .map_err(|_| WindowsApiError::ResourceLimit)?;
        // SAFETY: writing the rate variant of the documented union; the
        // value is a validated whole-number percentage 1..100 and the field
        // is a plain `u32`, so the assignment itself cannot invalidate any
        // union invariant.
        rate.Anonymous.CpuRate = percent;
        // SAFETY: the pointer and byte count describe the live `rate` value
        // for the duration of this synchronous call.
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectCpuRateControlInformation,
                std::ptr::from_ref(&rate).cast(),
                rate_size,
            )
        }
        .map_err(set_information_failure)?;
    }
    Ok(())
}

/// Map `OpenProcess` failures: a denied open is a permission fact, and the
/// documented error for a nonexistent PID is an identity fact.
#[cfg(windows)]
fn open_process_failure(error: windows::core::Error) -> WindowsApiError {
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

/// Map assignment failures. `ERROR_ACCESS_DENIED` here covers both an
/// inaccessible target and a job hierarchy that cannot nest this assignment
/// (pre-Windows-8 semantics or a broken hierarchy): both are honest refusals
/// of the whole request, never a partial apply.
#[cfg(windows)]
fn assign_failure(error: windows::core::Error) -> WindowsApiError {
    use windows::Win32::Foundation::ERROR_ACCESS_DENIED;

    if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
        WindowsApiError::PermissionDenied
    } else {
        WindowsApiError::QueryFailed
    }
}

/// Map limit application failures: a rejected parameter means the request
/// was not representable; anything else is a native query failure.
#[cfg(windows)]
fn set_information_failure(error: windows::core::Error) -> WindowsApiError {
    use windows::Win32::Foundation::ERROR_INVALID_PARAMETER;

    if error.code() == ERROR_INVALID_PARAMETER.to_hresult() {
        WindowsApiError::InvalidInput
    } else {
        WindowsApiError::QueryFailed
    }
}

/// Non-Windows hosts keep the lane dormant with the typed fallback.
#[cfg(not(windows))]
pub fn apply_process_job_limits(
    _pid: u32,
    _creation_filetime_100ns: u64,
    _limits: &WindowsJobLimitRequest,
) -> Result<(), WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

/// Non-Windows arm of [`clear_process_job_limits`].
#[cfg(not(windows))]
pub fn clear_process_job_limits(
    _pid: u32,
    _creation_filetime_100ns: u64,
) -> Result<bool, WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_job_control.rs"]
mod tests;
