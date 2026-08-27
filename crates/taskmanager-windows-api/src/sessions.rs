//! Bounded Windows Terminal Services (WTS) session access.
//!
//! This replaces text parsing of `query session`/`logoff`. WTS allocations,
//! UTF-16 buffers, and the native session array are private to this module;
//! callers receive owned strings and typed session states only.

use super::WindowsApiError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsSessionState {
    Active,
    Connected,
    Disconnected,
    Other(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsSession {
    pub session_id: u32,
    pub session_name: Option<String>,
    pub user_name: Option<String>,
    pub state: WindowsSessionState,
}

#[must_use = "inspect the native WTS session result"]
pub fn enumerate_sessions() -> Result<Vec<WindowsSession>, WindowsApiError> {
    #[cfg(windows)]
    {
        enumerate_sessions_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[must_use = "inspect the native WTS logoff result"]
pub fn logoff_session(session_id: u32) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        if session_id == 0 {
            return Err(WindowsApiError::InvalidInput);
        }
        {
            // SAFETY: the local-server `None` handle is valid for this
            // synchronous WTS operation and the session id is validated.
            unsafe {
                windows::Win32::System::RemoteDesktop::WTSLogoffSession(None, session_id, false)
            }
        }
        .map_err(map_windows_error)
    }
    #[cfg(not(windows))]
    {
        let _ = session_id;
        Err(WindowsApiError::Unsupported)
    }
}

/// Lock the calling workstation's interactive session via `LockWorkStation`.
///
/// This is a single unprivileged user32 call that always targets the session
/// the process runs in — the correct semantics for a desktop application; it
/// cannot lock an arbitrary WTS session id, so no session parameter exists.
#[must_use = "inspect the native workstation lock result"]
pub fn lock_workstation() -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        // SAFETY: `LockWorkStation` takes no pointers and affects only the
        // calling session; the BOOL result maps failure honestly.
        unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|error| {
            use windows::Win32::Foundation::ERROR_ACCESS_DENIED;

            if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
                WindowsApiError::PermissionDenied
            } else {
                WindowsApiError::QueryFailed
            }
        })
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
const MAX_WTS_SESSIONS: u32 = 4096;

#[cfg(windows)]
const MAX_WTS_WIDE_BYTES: u32 = 64 * 1024;

#[cfg(windows)]
const MAX_WTS_SESSION_NAME_UNITS: usize = 256;

#[cfg(windows)]
struct WtsMemory(*mut core::ffi::c_void);

#[cfg(windows)]
impl WtsMemory {
    fn new(pointer: *mut core::ffi::c_void) -> Option<Self> {
        (!pointer.is_null()).then_some(Self(pointer))
    }
}

#[cfg(windows)]
impl Drop for WtsMemory {
    fn drop(&mut self) {
        // SAFETY: WTS allocated this pointer and this guard frees it exactly
        // once after all borrowed fields have gone out of scope.
        unsafe { windows::Win32::System::RemoteDesktop::WTSFreeMemory(self.0) };
    }
}

#[cfg(windows)]
fn enumerate_sessions_windows() -> Result<Vec<WindowsSession>, WindowsApiError> {
    use std::ptr::null_mut;
    use windows::Win32::System::RemoteDesktop::{WTS_SESSION_INFOW, WTSEnumerateSessionsW};

    let mut sessions: *mut WTS_SESSION_INFOW = null_mut();
    let mut count = 0u32;
    {
        // SAFETY: the output pointers refer to writable local variables; the
        // local-server handle is represented by `None`; WTS owns the returned
        // array until the RAII guard below frees it.
        unsafe { WTSEnumerateSessionsW(None, 0, 1, &mut sessions, &mut count) }
    }
    .map_err(map_windows_error)?;
    let _allocation = WtsMemory::new(sessions.cast());
    if count > MAX_WTS_SESSIONS {
        return Err(WindowsApiError::ResourceLimit);
    }
    if count > 0 && sessions.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let row_count = usize::try_from(count).map_err(|_| WindowsApiError::ResourceLimit)?;
    let rows = if count == 0 {
        &[][..]
    } else {
        // SAFETY: WTS returned a non-null array containing exactly `count`
        // entries, and the count is bounded above before pointer arithmetic.
        unsafe { std::slice::from_raw_parts(sessions, row_count) }
    };
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let session_name = decode_nul_wide(row.pWinStationName.0, MAX_WTS_SESSION_NAME_UNITS)?;
        // A single optional WTS field must not block the complete session
        // list. Permission/query failures become an absent field; malformed
        // native text remains a typed failure from the boundary itself.
        let user_name = query_text(
            row.SessionId,
            windows::Win32::System::RemoteDesktop::WTSUserName,
        )
        .unwrap_or(None);
        result.push(WindowsSession {
            session_id: row.SessionId,
            session_name,
            user_name,
            state: WindowsSessionState::from_raw(row.State.0),
        });
    }
    Ok(result)
}

#[cfg(windows)]
impl WindowsSessionState {
    fn from_raw(value: i32) -> Self {
        use windows::Win32::System::RemoteDesktop::{WTSActive, WTSConnected, WTSDisconnected};
        match value {
            value if value == WTSActive.0 => Self::Active,
            value if value == WTSConnected.0 => Self::Connected,
            value if value == WTSDisconnected.0 => Self::Disconnected,
            other => Self::Other(other),
        }
    }
}

#[cfg(windows)]
fn query_text(
    session_id: u32,
    class: windows::Win32::System::RemoteDesktop::WTS_INFO_CLASS,
) -> Result<Option<String>, WindowsApiError> {
    use windows::Win32::System::RemoteDesktop::WTSQuerySessionInformationW;
    use windows::core::PWSTR;

    let mut buffer = PWSTR(std::ptr::null_mut());
    let mut bytes_returned = 0u32;
    {
        // SAFETY: the output pointers refer to local variables; WTS allocates
        // the returned buffer and the RAII guard frees it after decoding.
        unsafe {
            WTSQuerySessionInformationW(None, session_id, class, &mut buffer, &mut bytes_returned)
        }
    }
    .map_err(map_windows_error)?;
    let _allocation = WtsMemory::new(buffer.0.cast());
    if buffer.0.is_null() || bytes_returned == 0 {
        return Ok(None);
    }
    if bytes_returned > MAX_WTS_WIDE_BYTES || !bytes_returned.is_multiple_of(2) {
        return Err(WindowsApiError::ResourceLimit);
    }
    let units = usize::try_from(bytes_returned / 2).map_err(|_| WindowsApiError::ResourceLimit)?;
    let text = {
        // SAFETY: WTS reports the byte length of the allocated UTF-16 buffer;
        // the even-length and maximum-size checks above bound the slice.
        unsafe { std::slice::from_raw_parts(buffer.0, units) }
    };
    let end = text
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(text.len());
    String::from_utf16(&text[..end])
        .map(|text| (!text.is_empty()).then_some(text))
        .map_err(|_| WindowsApiError::InvalidText)
}

#[cfg(windows)]
fn decode_nul_wide(pointer: *mut u16, max_units: usize) -> Result<Option<String>, WindowsApiError> {
    if pointer.is_null() {
        return Ok(None);
    }
    let mut length = 0usize;
    while length < max_units {
        let value = {
            // SAFETY: the WTS session record supplies a NUL-terminated string
            // pointer and the scan is bounded by the native field contract.
            unsafe { *pointer.add(length) }
        };
        if value == 0 {
            let units = {
                // SAFETY: the prefix was just scanned within the fixed bound.
                unsafe { std::slice::from_raw_parts(pointer, length) }
            };
            return String::from_utf16(units)
                .map(|text| (!text.is_empty()).then_some(text))
                .map_err(|_| WindowsApiError::InvalidText);
        }
        length += 1;
    }
    Err(WindowsApiError::ResourceLimit)
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
#[path = "../tests/headless/windows_api_sessions.rs"]
mod tests;
