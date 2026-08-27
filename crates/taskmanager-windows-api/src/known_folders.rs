//! Bounded Known Folder resolution for user-scoped Windows data.
//!
//! `SHGetKnownFolderPath` returns a task-allocator-owned UTF-16 buffer. The
//! allocation and pointer never leave this module; the public API returns only
//! an absolute `PathBuf`.

use std::path::PathBuf;

use super::WindowsApiError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownFolder {
    RoamingAppData,
    LocalAppData,
    Startup,
}

#[must_use = "inspect the native Known Folder result"]
pub fn known_folder_path(folder: KnownFolder) -> Result<PathBuf, WindowsApiError> {
    #[cfg(windows)]
    {
        known_folder_path_windows(folder)
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
const MAX_KNOWN_FOLDER_UTF16: usize = 32 * 1024;

#[cfg(windows)]
struct CoTaskMemWide(windows::core::PWSTR);

#[cfg(windows)]
impl Drop for CoTaskMemWide {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: `SHGetKnownFolderPath` returns this pointer from the
            // COM task allocator, and this guard owns it until Drop.
            unsafe {
                windows::Win32::System::Com::CoTaskMemFree(Some(self.0.0.cast_const().cast()));
            }
        }
    }
}

#[cfg(windows)]
fn known_folder_path_windows(folder: KnownFolder) -> Result<PathBuf, WindowsApiError> {
    use windows::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_RoamingAppData, FOLDERID_Startup, KNOWN_FOLDER_FLAG,
        SHGetKnownFolderPath,
    };

    let folder_id = match folder {
        KnownFolder::RoamingAppData => FOLDERID_RoamingAppData,
        KnownFolder::LocalAppData => FOLDERID_LocalAppData,
        KnownFolder::Startup => FOLDERID_Startup,
    };
    let pointer = {
        // SAFETY: `folder_id` is a static system GUID, flags request the
        // current user, and Windows returns an allocator-owned NUL-terminated
        // UTF-16 path which is immediately placed under the RAII guard.
        unsafe { SHGetKnownFolderPath(&folder_id, KNOWN_FOLDER_FLAG(0), None) }
    }
    .map_err(|_| WindowsApiError::QueryFailed)?;
    let owned = CoTaskMemWide(pointer);
    let path = decode_known_folder_path(owned.0.0)?;
    let path = PathBuf::from(path);
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(WindowsApiError::InvalidText);
    }
    Ok(path)
}

#[cfg(windows)]
fn decode_known_folder_path(pointer: *mut u16) -> Result<String, WindowsApiError> {
    if pointer.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let mut length = 0usize;
    while length < MAX_KNOWN_FOLDER_UTF16 {
        let value = {
            // SAFETY: the API owns a valid UTF-16 buffer, and the loop never
            // reads beyond the fixed maximum accepted by this boundary.
            unsafe { *pointer.add(length) }
        };
        if value == 0 {
            let units = {
                // SAFETY: `pointer` is valid for the NUL-terminated prefix
                // just scanned, and `length` is within the fixed bound.
                unsafe { std::slice::from_raw_parts(pointer, length) }
            };
            return String::from_utf16(units).map_err(|_| WindowsApiError::InvalidText);
        }
        length += 1;
    }
    Err(WindowsApiError::ResourceLimit)
}

#[cfg(all(test, windows))]
#[path = "../tests/headless/windows_api_known_folders.rs"]
mod tests;
