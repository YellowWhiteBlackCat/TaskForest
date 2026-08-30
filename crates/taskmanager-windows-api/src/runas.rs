//! Audited `runas` elevation call group (ADR-035 stage 2).
//!
//! This is the ONE place the Windows UAC crossing touches raw Win32: the
//! `ShellExecuteExW("runas")` launch with `SEE_MASK_NOCLOSEPROCESS`, the
//! bounded `WaitForSingleObject`, `GetExitCodeProcess`, and `CloseHandle`.
//! No handle, pointer, or UTF-16 buffer crosses this module's public API —
//! callers pass owned strings and a deadline and receive a typed
//! [`RunasLaunchOutcome`]. The elevated child's reply contract is NOT read
//! here: the caller owns the one-shot reply channel and classifies it with
//! the shared escalation reader, so protocol data is never implicitly
//! interpreted from an exit code.

use std::time::Duration;

use super::WindowsApiError;

/// `ERROR_INVALID_PARAMETER` (87): the boundary refused to launch because an
/// input was unusable (empty program path, interior NUL, missing process
/// handle). Classifies as a neutral non-user failure downstream.
const WIN32_ERROR_INVALID_PARAMETER: u32 = 87;

/// What one bounded elevated launch observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunasLaunchOutcome {
    /// The elevated process was launched and the bounded wait observed its
    /// completion; `exit_code` is auxiliary diagnostics — the reply channel,
    /// not the exit code, carries the helper contract.
    Completed {
        /// The process exit code reported by `GetExitCodeProcess`.
        exit_code: u32,
    },
    /// `ShellExecuteExW("runas", …)` failed. `win32_error` is the raw Win32
    /// code when Windows reported one (for example `ERROR_CANCELLED` 1223
    /// for the user's "No" and `ERROR_FILE_NOT_FOUND` 2 for a missing
    /// helper), the raw HRESULT bits for a plain COM failure, or the
    /// boundary-assigned `ERROR_INVALID_PARAMETER` for unusable input.
    LaunchFailed {
        /// The raw error code from the failed launch.
        win32_error: u32,
    },
    /// The bounded wait reached its deadline while the consent or helper was
    /// still outstanding. The process handle is closed and the crossing is
    /// abandoned — the helper is one-shot, so nothing is terminated.
    DeadlineExceeded,
    /// The target is not Windows, so the native call group does not exist.
    Unsupported,
}

/// Whether the calling process sits in an interactive session.
///
/// Session 0 is the services session: a UAC consent cannot be shown there,
/// so the honest transport fact is "no consent available", never a user
/// refusal. A query failure is returned as a typed error for the same
/// reason — the caller must not guess.
#[must_use = "inspect the native session result"]
pub fn interactive_session_available() -> Result<bool, WindowsApiError> {
    #[cfg(windows)]
    {
        interactive_session_available_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Launch `program` elevated with the `runas` verb and wait, bounded.
///
/// `parameters` is the pre-built command line for the elevated child (the
/// caller builds it with the escalation crate's pure builder). The launch
/// shows the OS-native UAC consent; `ERROR_CANCELLED` from the user's "No"
/// arrives as [`RunasLaunchOutcome::LaunchFailed`] with win32 code 1223 and
/// is classified by the escalation transport-fact table, not here.
#[must_use = "inspect the native runas launch result"]
pub fn run_elevated_and_wait(
    program: &str,
    parameters: &str,
    deadline: Duration,
) -> RunasLaunchOutcome {
    #[cfg(windows)]
    {
        run_elevated_and_wait_windows(program, parameters, deadline)
    }
    #[cfg(not(windows))]
    {
        let _ = (program, parameters, deadline);
        RunasLaunchOutcome::Unsupported
    }
}

/// Recover the Win32 error code carried by a `windows`-crate failure.
///
/// `HRESULT_FROM_WIN32` maps small Win32 codes into facility 7 with the
/// severity bit set; any other failure keeps its raw HRESULT bits, which can
/// never collide with the two codes the transport-fact table classifies
/// (2 and 1223 both lack the severity bit), so an unattributable failure
/// stays unattributable.
#[cfg(any(windows, test))]
fn win32_code_from_hresult(code: i32) -> u32 {
    let bits = u32::from_ne_bytes(code.to_ne_bytes());
    if bits & 0x8000_0000 != 0 && (bits >> 16) & 0x1FFF == 7 {
        bits & 0xFFFF
    } else {
        bits
    }
}

#[cfg(windows)]
const MAX_RUNAS_UTF16: usize = 32 * 1024;

/// Encode one NUL-terminated UTF-16 string, rejecting empty input, interior
/// NULs, and overlong values so the boundary never passes an unbounded or
/// unterminated buffer to the shell.
#[cfg(windows)]
fn encode_utf16(text: &str) -> Option<Vec<u16>> {
    if text.is_empty() || text.contains('\0') {
        return None;
    }
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if units.len() >= MAX_RUNAS_UTF16 {
        return None;
    }
    units.push(0);
    Some(units)
}

#[cfg(windows)]
struct ProcessHandle(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: `SEE_MASK_NOCLOSEPROCESS` handed this handle to this guard
        // exclusively; Drop closes it exactly once and no other code uses it
        // afterwards.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
fn run_elevated_and_wait_windows(
    program: &str,
    parameters: &str,
    deadline: Duration,
) -> RunasLaunchOutcome {
    use std::mem::size_of;

    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::PCWSTR;

    let invalid_parameter = RunasLaunchOutcome::LaunchFailed {
        win32_error: WIN32_ERROR_INVALID_PARAMETER,
    };
    let Some(mut verb) = encode_utf16("runas") else {
        return invalid_parameter;
    };
    let Some(mut file) = encode_utf16(program) else {
        return invalid_parameter;
    };
    let Some(mut parameters_utf16) = encode_utf16(parameters) else {
        return invalid_parameter;
    };
    let cb_size = u32::try_from(size_of::<SHELLEXECUTEINFOW>());
    let Ok(cb_size) = cb_size else {
        return invalid_parameter;
    };
    // A deadline beyond the wait API's 32-bit millisecond range saturates at
    // that range: the crossing stays bounded either way.
    let wait_millis = u32::try_from(deadline.as_millis()).unwrap_or(u32::MAX);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: cb_size,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_mut_ptr()),
        lpFile: PCWSTR(file.as_mut_ptr()),
        lpParameters: PCWSTR(parameters_utf16.as_mut_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(error) = {
        // SAFETY: `info` is a valid, writable `SHELLEXECUTEINFOW` whose
        // `cbSize` matches its allocated size; every string parameter points
        // at a NUL-terminated UTF-16 buffer owned by this frame and alive for
        // the whole synchronous call, and the shell does not retain them.
        unsafe { ShellExecuteExW(&mut info) }
    } {
        return RunasLaunchOutcome::LaunchFailed {
            win32_error: win32_code_from_hresult(error.code().0),
        };
    }
    if info.hProcess.is_invalid() {
        return invalid_parameter;
    }
    let process = ProcessHandle(info.hProcess);
    let wait = {
        // SAFETY: `process.0` is a valid, exclusively owned process handle
        // from `SEE_MASK_NOCLOSEPROCESS`; the call is synchronous and does
        // not retain the handle.
        unsafe { WaitForSingleObject(process.0, wait_millis) }
    };
    if wait == windows::Win32::Foundation::WAIT_TIMEOUT {
        return RunasLaunchOutcome::DeadlineExceeded;
    }
    if wait != WAIT_OBJECT_0 {
        // WAIT_FAILED or an unexpected wait value: the authorization did not
        // complete and the raw wait bits cannot be attributed to the user.
        return RunasLaunchOutcome::LaunchFailed {
            win32_error: wait.0,
        };
    }
    let mut exit_code = 0_u32;
    if let Err(error) = {
        // SAFETY: `exit_code` is a valid writable u32 out-param and
        // `process.0` is the exclusively owned, signaled process handle.
        unsafe { GetExitCodeProcess(process.0, &mut exit_code) }
    } {
        return RunasLaunchOutcome::LaunchFailed {
            win32_error: win32_code_from_hresult(error.code().0),
        };
    }
    RunasLaunchOutcome::Completed { exit_code }
}

#[cfg(windows)]
fn interactive_session_available_windows() -> Result<bool, WindowsApiError> {
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::Threading::GetCurrentProcessId;

    let mut session_id = 0_u32;
    let query = {
        // SAFETY: `session_id` is a valid writable u32 out-param for this
        // synchronous call and the pid is the current process's own id, so
        // no unvalidated external identifier crosses the boundary.
        unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) }
    };
    query.map_err(|_| WindowsApiError::QueryFailed)?;
    Ok(session_id != 0)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_runas.rs"]
mod tests;
