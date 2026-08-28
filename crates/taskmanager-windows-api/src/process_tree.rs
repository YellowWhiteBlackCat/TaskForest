//! Atomic Windows process-tree ownership for bounded command execution.
//!
//! The caller creates the direct child with `CREATE_SUSPENDED`. This boundary
//! assigns that inert process to a kill-on-close Job Object before resuming its
//! initial thread, so no descendant can escape between spawn and assignment.

use super::WindowsApiError;

#[cfg(windows)]
const MAX_THREAD_SNAPSHOT_ENTRIES: usize = 1_000_000;

#[cfg(windows)]
pub struct WindowsProcessJob {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(not(windows))]
pub struct WindowsProcessJob;

impl WindowsProcessJob {
    /// Terminate every process assigned to this job.
    pub fn terminate(&self) -> Result<(), WindowsApiError> {
        #[cfg(windows)]
        {
            // SAFETY: `handle` is an owned live Job Object handle.
            unsafe {
                windows::Win32::System::JobObjects::TerminateJobObject(self.handle, 1)
                    .map_err(|_| WindowsApiError::QueryFailed)
            }
        }
        #[cfg(not(windows))]
        {
            Err(WindowsApiError::Unsupported)
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsProcessJob {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE makes this a final fail-safe for every descendant.
        // SAFETY: `handle` is owned by this value and closed exactly once.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}

/// Atomically establish tree ownership for a suspended std child, then resume
/// its initial thread.
pub fn assign_and_resume_suspended_process(pid: u32) -> Result<WindowsProcessJob, WindowsApiError> {
    #[cfg(windows)]
    {
        assign_and_resume_suspended_process_windows(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn assign_and_resume_suspended_process_windows(
    pid: u32,
) -> Result<WindowsProcessJob, WindowsApiError> {
    use std::mem::size_of;

    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, OpenThread, PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread,
        THREAD_SUSPEND_RESUME,
    };

    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }

    // SAFETY: unnamed job, no security-attribute pointer retained.
    let job = unsafe { CreateJobObjectW(None, None) }.map_err(|_| WindowsApiError::QueryFailed)?;
    let job = WindowsProcessJob { handle: job };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: the pointer and byte count describe a live `limits` value for
    // the synchronous SetInformationJobObject call.
    unsafe {
        SetInformationJobObject(
            job.handle,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .map_err(|_| WindowsApiError::ResourceLimit)?,
        )
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;

    // SAFETY: PID is nonzero and the returned handle is owned locally.
    let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }
        .map_err(|_| WindowsApiError::PermissionDenied)?;
    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: owned handle closed exactly once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    let process = HandleGuard(process);
    // SAFETY: both handles are live; the suspended process has not executed.
    unsafe { AssignProcessToJobObject(job.handle, process.0) }
        .map_err(|_| WindowsApiError::QueryFailed)?;

    // Find the initial suspended thread. It cannot create another thread until
    // resumed, so the first owner match is authoritative for this transition.
    // SAFETY: system-wide thread snapshot has no borrowed inputs.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map_err(|_| WindowsApiError::QueryFailed)?;
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(WindowsApiError::QueryFailed);
    }
    let snapshot = HandleGuard(snapshot);
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>())
            .map_err(|_| WindowsApiError::ResourceLimit)?,
        ..THREADENTRY32::default()
    };
    // SAFETY: snapshot and sized output are live for each synchronous call.
    let mut present = unsafe { Thread32First(snapshot.0, &mut entry) }.is_ok();
    let mut thread_id = None;
    let mut inspected = 0usize;
    while present {
        inspected = inspected
            .checked_add(1)
            .ok_or(WindowsApiError::ResourceLimit)?;
        if inspected > MAX_THREAD_SNAPSHOT_ENTRIES {
            return Err(WindowsApiError::ResourceLimit);
        }
        if entry.th32OwnerProcessID == pid {
            thread_id = Some(entry.th32ThreadID);
            break;
        }
        entry.dwSize = u32::try_from(size_of::<THREADENTRY32>())
            .map_err(|_| WindowsApiError::ResourceLimit)?;
        // SAFETY: same live snapshot and sized output.
        present = unsafe { Thread32Next(snapshot.0, &mut entry) }.is_ok();
    }
    let thread_id = thread_id.ok_or(WindowsApiError::IdentityChanged)?;
    // SAFETY: thread ID belongs to the still-suspended target process.
    let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, thread_id) }
        .map_err(|_| WindowsApiError::PermissionDenied)?;
    let thread = HandleGuard(thread);
    // SAFETY: owned thread handle has THREAD_SUSPEND_RESUME access.
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(job)
}
