//! Named single-instance primitives (mutex + auto-reset event), audited.
//!
//! Windows single-instance (borrowed from the tauri-plugin-single-instance
//! core): a named mutex gives atomic exclusivity — a second `CreateMutexW`
//! reports `ERROR_ALREADY_EXISTS` — and a named auto-reset event carries the
//! "activate the existing instance" handoff. The primary holds both handles
//! (RAII guards) and waits on the event on a helper thread; a secondary
//! signals the event and exits. No handle, pointer, or UTF-16 buffer crosses
//! this crate's public API.

use super::WindowsApiError;

/// Maximum length of a user-supplied instance name. Kernel object names are
/// bounded by `MAX_PATH`; this cap is far below it and keeps encoding cheap.
const MAX_INSTANCE_NAME_CHARS: usize = 64;

/// Validate and qualify a user-supplied instance name into a `Local\` kernel
/// object name. Rejects empty, overlong, non-ASCII, or backslash-containing
/// names.
fn qualify_name(name: &str) -> Result<String, WindowsApiError> {
    if name.is_empty() || name.chars().count() > MAX_INSTANCE_NAME_CHARS {
        return Err(WindowsApiError::InvalidInput);
    }
    if !name.is_ascii() || name.contains('\\') {
        return Err(WindowsApiError::InvalidInput);
    }
    Ok(format!("Local\\taskforest.{name}"))
}

/// RAII guard over a named mutex.
#[derive(Debug)]
pub struct InstanceMutex {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    handle: (),
}

// SAFETY: the handle is an opaque kernel object id; Win32 mutex handles are
// usable from any thread (WaitForSingleObject/ReleaseMutex are thread-safe),
// and the guard guarantees the handle is closed exactly once, on the thread
// that drops the last owner. No raw pointer is exposed by this type.
unsafe impl Send for InstanceMutex {}
// SAFETY: concurrent access is not performed through this type; a handle
// value may be freely shared, and kernel calls serialize internally.
unsafe impl Sync for InstanceMutex {}

impl InstanceMutex {
    /// Create (or open) the named mutex. `Ok((guard, true))` means another
    /// process already owns the instance.
    pub fn create(name: &str) -> Result<(Self, bool), WindowsApiError> {
        #[cfg(windows)]
        {
            create_windows(name)
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(WindowsApiError::Unsupported)
        }
    }
}

#[cfg(windows)]
fn create_windows(name: &str) -> Result<(InstanceMutex, bool), WindowsApiError> {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::HSTRING;

    let qualified = qualify_name(name)?;
    let name = HSTRING::from(&qualified);
    let handle = {
        // SAFETY: no security attributes and no initial ownership are
        // requested; the name is a bounded, NUL-terminated UTF-16 buffer
        // alive for this synchronous call. The returned handle is immediately
        // owned by the RAII guard.
        unsafe { CreateMutexW(None, false, &name) }
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    if handle.is_invalid() {
        return Err(WindowsApiError::QueryFailed);
    }
    let already_exists = {
        // SAFETY: GetLastError reflects the immediately preceding Win32 call.
        unsafe { GetLastError() }
    } == ERROR_ALREADY_EXISTS;
    Ok((InstanceMutex { handle }, already_exists))
}

impl Drop for InstanceMutex {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // SAFETY: `handle` is a valid mutex handle owned by this guard and is
            // not referenced elsewhere.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
        }
    }
}

/// RAII guard over a named auto-reset event.
#[derive(Debug)]
pub struct InstanceEvent {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    handle: (),
}

// SAFETY: the handle is an opaque kernel object id; Win32 event handles are
// usable from any thread (WaitForSingleObject/SetEvent are thread-safe), and
// the guard closes the handle exactly once on the dropping thread. No raw
// pointer is exposed by this type.
unsafe impl Send for InstanceEvent {}
// SAFETY: concurrent access is not performed through this type; a handle
// value may be freely shared, and kernel calls serialize internally.
unsafe impl Sync for InstanceEvent {}

impl InstanceEvent {
    /// Create the named auto-reset event (initially unsignaled).
    pub fn create(name: &str) -> Result<Self, WindowsApiError> {
        #[cfg(windows)]
        {
            create_event_windows(name)
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(WindowsApiError::Unsupported)
        }
    }

    /// Block until the event is signaled (auto-reset consumes the signal).
    pub fn wait(&self) -> Result<(), WindowsApiError> {
        #[cfg(windows)]
        {
            wait_windows(&self.handle)
        }
        #[cfg(not(windows))]
        {
            let _ = &self.handle;
            Err(WindowsApiError::Unsupported)
        }
    }
}

#[cfg(windows)]
fn create_event_windows(name: &str) -> Result<InstanceEvent, WindowsApiError> {
    use windows::Win32::System::Threading::CreateEventW;
    use windows::core::HSTRING;

    let qualified = qualify_name(name)?;
    let name = HSTRING::from(&qualified);
    let handle = {
        // SAFETY: null security attributes; auto-reset (manual_reset=false);
        // initially unsignaled. The handle is immediately owned by the RAII
        // guard.
        unsafe { CreateEventW(None, false, false, &name) }
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    if handle.is_invalid() {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(InstanceEvent { handle })
}

#[cfg(windows)]
fn wait_windows(handle: &windows::Win32::Foundation::HANDLE) -> Result<(), WindowsApiError> {
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::WaitForSingleObject;

    // SAFETY: `handle` is a valid event handle owned by the caller's guard
    // and remains valid for the duration of the wait.
    let result = unsafe { WaitForSingleObject(*handle, u32::MAX) };
    if result == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(WindowsApiError::QueryFailed)
    }
}

impl Drop for InstanceEvent {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // SAFETY: `handle` is a valid event handle owned by this guard and is
            // not referenced elsewhere.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
        }
    }
}

/// Signal the named event owned by another process (secondary → primary
/// handoff). Best-effort: fails with a typed error when no such event exists.
pub fn signal_named_event(name: &str) -> Result<(), WindowsApiError> {
    #[cfg(windows)]
    {
        signal_named_event_windows(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn signal_named_event_windows(name: &str) -> Result<(), WindowsApiError> {
    use windows::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};
    use windows::core::HSTRING;

    let qualified = qualify_name(name)?;
    let name = HSTRING::from(&qualified);
    let handle = {
        // SAFETY: only the modify right is requested; the name is a bounded,
        // NUL-terminated UTF-16 buffer alive for this call. The returned
        // handle is owned by the RAII drop guard below.
        unsafe { OpenEventW(EVENT_MODIFY_STATE, false, &name) }
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    struct Owned(windows::Win32::Foundation::HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the event handle opened above and owned here.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }
    let owned = Owned(handle);
    {
        // SAFETY: `owned.0` is a valid, still-open event handle.
        unsafe { SetEvent(owned.0) }
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    drop(owned);
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "../tests/headless/windows_api_single_instance.rs"]
mod tests;
