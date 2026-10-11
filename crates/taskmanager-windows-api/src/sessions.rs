//! Bounded Windows Terminal Services (WTS) session access.
//!
//! This replaces text parsing of `query session`/`logoff`. WTS allocations,
//! UTF-16 buffers, and the native session array are private to this module;
//! callers receive owned strings and typed session states only.

use super::WindowsApiError;
use std::num::{NonZeroU32, NonZeroU64};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsSessionState {
    Active,
    Connected,
    Disconnected,
    Other(i32),
}

/// WTS-owned per-logon discriminator, retained as raw 100 ns ticks since
/// 1601 UTC. This is equality evidence, not a display timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowsSessionIncarnation(NonZeroU64);

impl WindowsSessionIncarnation {
    /// Reject an absent WTS logon time instead of manufacturing an identity.
    #[must_use]
    pub const fn new(logon_time_100ns: u64) -> Option<Self> {
        match NonZeroU64::new(logon_time_100ns) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Raw WTS `LogonTime` ticks, suitable only for identity comparison.
    #[must_use]
    pub const fn logon_time_100ns(self) -> u64 {
        self.0.get()
    }
}

/// Exact WTS session identity used by native control calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowsSessionIdentity {
    session_id: NonZeroU32,
    incarnation: WindowsSessionIncarnation,
}

impl WindowsSessionIdentity {
    /// Build a native target only for nonzero WTS IDs with trusted logon data.
    #[must_use]
    pub const fn new(session_id: u32, incarnation: WindowsSessionIncarnation) -> Option<Self> {
        match NonZeroU32::new(session_id) {
            Some(session_id) => Some(Self {
                session_id,
                incarnation,
            }),
            None => None,
        }
    }

    /// The native WTS session identifier.
    #[must_use]
    pub const fn session_id(self) -> u32 {
        self.session_id.get()
    }

    /// The exact per-logon WTS discriminator captured for this session.
    #[must_use]
    pub const fn incarnation(self) -> WindowsSessionIncarnation {
        self.incarnation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsSession {
    pub session_id: u32,
    /// Query result for the session's WTS logon discriminator. Errors leave
    /// the row displayable but never create an actionable identity.
    pub incarnation: Result<WindowsSessionIncarnation, WindowsApiError>,
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

/// Submit a logoff only after re-reading the target's WTS LogonTime.
///
/// The WTS API receives only the numeric session id, so the requery and the
/// native action are not atomic. With `bWait=false`, `Ok(())` means WTS
/// accepted the request; it does not mean logoff has completed.
#[must_use = "inspect the native WTS logoff result"]
pub fn logoff_session(target: WindowsSessionIdentity) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        dispatch_logoff_after_revalidation(target, query_session_incarnation, |session_id| {
            // SAFETY: `None` is the local WTS server; session_id is
            // nonzero and its WTS LogonTime was just revalidated. The
            // native API call returns after submitting, not completing,
            // the logoff because bWait is false.
            unsafe {
                windows::Win32::System::RemoteDesktop::WTSLogoffSession(None, session_id, false)
            }
            .map_err(map_windows_error)
        })
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err(WindowsApiError::Unsupported)
    }
}

/// Lock the calling process's interactive WTS session via `LockWorkStation`.
///
/// `LockWorkStation` cannot address an arbitrary WTS session. The target must
/// match the current process's session ID and freshly queried WTS LogonTime.
/// A successful call only means that the asynchronous lock request started.
#[must_use = "inspect the native workstation lock result"]
pub fn lock_current_session(target: WindowsSessionIdentity) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        dispatch_lock_only_for_current_session(
            target,
            || {
                let current_session_id = current_process_session_id()?;
                let incarnation = query_session_incarnation(current_session_id)?;
                WindowsSessionIdentity::new(current_session_id, incarnation)
                    .ok_or(WindowsApiError::IdentityChanged)
            },
            lock_workstation_native,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
const MAX_WTS_SESSIONS: u32 = 4096;

#[cfg(windows)]
const MAX_WTS_WIDE_BYTES: u32 = 64 * 1024;

const MAX_WTS_SESSION_INFO_BYTES: u32 = 1024;

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
        let incarnation = query_session_incarnation(row.SessionId);
        result.push(WindowsSession {
            session_id: row.SessionId,
            incarnation,
            session_name,
            user_name,
            state: WindowsSessionState::from_raw(row.State.0),
        });
    }
    Ok(result)
}

#[cfg(windows)]
fn query_session_incarnation(
    session_id: u32,
) -> Result<WindowsSessionIncarnation, WindowsApiError> {
    use std::mem::size_of;
    use windows::Win32::System::RemoteDesktop::{
        WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
    };
    use windows::core::PWSTR;

    let mut buffer = PWSTR(std::ptr::null_mut());
    let mut bytes_returned = 0u32;
    {
        // SAFETY: output pointers refer to initialized local variables; WTS
        // owns the returned buffer until the RAII guard below frees it.
        unsafe {
            WTSQuerySessionInformationW(
                None,
                session_id,
                WTSSessionInfoEx,
                &mut buffer,
                &mut bytes_returned,
            )
        }
    }
    .map_err(map_windows_error)?;
    let _allocation = WtsMemory::new(buffer.0.cast()).ok_or(WindowsApiError::QueryFailed)?;
    let expected_bytes =
        u32::try_from(size_of::<WTSINFOEXW>()).map_err(|_| WindowsApiError::ResourceLimit)?;
    validate_wts_infoex_size(bytes_returned, expected_bytes)?;
    // SAFETY: WTS allocated this pointer, the returned size exactly matches
    // the generated WTSINFOEXW ABI, and unaligned read avoids assuming an
    // alignment stronger than WTS promises for the opaque byte buffer.
    let info = unsafe { buffer.0.cast::<WTSINFOEXW>().read_unaligned() };
    if info.Level != 1 {
        return Err(WindowsApiError::Unsupported);
    }
    // SAFETY: Level==1 selects WTSINFOEX_LEVEL1_W per the documented WTS ABI.
    let level1 = unsafe { info.Data.WTSInfoExLevel1 };
    parse_wts_session_incarnation(session_id, info.Level, level1.SessionId, level1.LogonTime)
}

#[cfg(windows)]
fn current_process_session_id() -> Result<u32, WindowsApiError> {
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::Threading::GetCurrentProcessId;

    // SAFETY: GetCurrentProcessId has no pointer arguments or preconditions.
    let process_id = unsafe { GetCurrentProcessId() };
    let mut session_id = 0u32;
    // SAFETY: session_id is writable for this synchronous Win32 call.
    unsafe { ProcessIdToSessionId(process_id, &mut session_id) }.map_err(map_windows_error)?;
    Ok(session_id)
}

#[cfg(windows)]
fn lock_workstation_native() -> Result<(), WindowsApiError> {
    // SAFETY: LockWorkStation has no pointer arguments; it acts only in the
    // calling process's interactive desktop session.
    unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|error| {
        use windows::Win32::Foundation::ERROR_ACCESS_DENIED;

        if error.code() == ERROR_ACCESS_DENIED.to_hresult() {
            WindowsApiError::PermissionDenied
        } else {
            WindowsApiError::QueryFailed
        }
    })
}

fn validate_wts_infoex_size(
    bytes_returned: u32,
    expected_bytes: u32,
) -> Result<(), WindowsApiError> {
    if bytes_returned > MAX_WTS_SESSION_INFO_BYTES {
        return Err(WindowsApiError::ResourceLimit);
    }
    if bytes_returned != expected_bytes || expected_bytes == 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(())
}

fn parse_wts_session_incarnation(
    requested_session_id: u32,
    level: u32,
    returned_session_id: u32,
    logon_time_100ns: i64,
) -> Result<WindowsSessionIncarnation, WindowsApiError> {
    if level != 1 {
        return Err(WindowsApiError::Unsupported);
    }
    if returned_session_id != requested_session_id {
        return Err(WindowsApiError::IdentityChanged);
    }
    let logon_time_100ns =
        u64::try_from(logon_time_100ns).map_err(|_| WindowsApiError::IdentityChanged)?;
    WindowsSessionIncarnation::new(logon_time_100ns).ok_or(WindowsApiError::IdentityChanged)
}

fn dispatch_logoff_after_revalidation(
    target: WindowsSessionIdentity,
    mut requery: impl FnMut(u32) -> Result<WindowsSessionIncarnation, WindowsApiError>,
    native_logoff: impl FnOnce(u32) -> Result<(), WindowsApiError>,
) -> Result<(), WindowsApiError> {
    let session_id = target.session_id();
    let current_incarnation = requery(session_id)?;
    if current_incarnation != target.incarnation() {
        return Err(WindowsApiError::IdentityChanged);
    }
    native_logoff(session_id)
}

fn dispatch_lock_only_for_current_session(
    target: WindowsSessionIdentity,
    requery_current: impl FnOnce() -> Result<WindowsSessionIdentity, WindowsApiError>,
    native_lock: impl FnOnce() -> Result<(), WindowsApiError>,
) -> Result<(), WindowsApiError> {
    let current = requery_current()?;
    if target.session_id() != current.session_id() {
        return Err(WindowsApiError::Unsupported);
    }
    if target.incarnation() != current.incarnation() {
        return Err(WindowsApiError::IdentityChanged);
    }
    native_lock()
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
    use windows::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_CALL_NOT_IMPLEMENTED, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED,
    };

    let code = error.code();
    if code == ERROR_ACCESS_DENIED.to_hresult() {
        WindowsApiError::PermissionDenied
    } else if code == ERROR_INVALID_PARAMETER.to_hresult() {
        WindowsApiError::IdentityChanged
    } else if code == ERROR_NOT_SUPPORTED.to_hresult()
        || code == ERROR_CALL_NOT_IMPLEMENTED.to_hresult()
    {
        WindowsApiError::Unsupported
    } else {
        WindowsApiError::QueryFailed
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_sessions.rs"]
mod tests;
