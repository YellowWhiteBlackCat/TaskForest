//! Safe audited Windows Named Pipe listener boundary.
//!
//! Exposes a safe, RAII-governed Named Pipe listener on Windows that accepts
//! incoming client connections as standard `std::fs::File` streams.
//!
//! On non-Windows targets, all operations safely return `WindowsApiError::Unsupported`.

use super::WindowsApiError;

#[cfg(windows)]
use std::fs::File;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::io::FromRawHandle;
#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
#[cfg(windows)]
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
#[cfg(windows)]
use windows::core::HSTRING;

/// Audited Named Pipe listener wrapping `CreateNamedPipeW` / `ConnectNamedPipe`.
#[derive(Debug)]
pub struct NamedPipeListener {
    pipe_name: String,
}

impl NamedPipeListener {
    /// Bind a new named pipe server listener at the designated pipe name.
    ///
    /// Pipe name must follow the Windows format: `\\.\pipe\<name>`.
    pub fn bind(pipe_name: impl Into<String>) -> Result<Self, WindowsApiError> {
        let name = pipe_name.into();
        if !name.starts_with(r"\\.\pipe\") || name.len() < 10 {
            return Err(WindowsApiError::InvalidInput);
        }
        Ok(Self { pipe_name: name })
    }

    /// Access the bound pipe path.
    #[must_use]
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    /// Accept the next incoming client connection, returning an owned `std::fs::File`.
    #[cfg(windows)]
    pub fn accept(&self) -> io::Result<File> {
        let h_name = HSTRING::from(self.pipe_name.as_str());

        // SAFETY: CreateNamedPipeW is called with bounded buffer sizes and standard flags.
        // The returned handle is validated against INVALID_HANDLE_VALUE before use.
        let handle = unsafe {
            CreateNamedPipeW(
                &h_name,
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                None,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: ConnectNamedPipe is called on a valid named pipe handle.
        // ERROR_PIPE_CONNECTED indicates a client connected between Create and Connect, which is valid.
        let connected = unsafe { ConnectNamedPipe(handle, None) };
        if let Err(err) = connected {
            let code = err.code();
            if code != ERROR_PIPE_CONNECTED.to_hresult() {
                // SAFETY: close handle on failure before returning
                unsafe {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                }
                return Err(io::Error::from_raw_os_error(code.0));
            }
        }

        // SAFETY: The handle is valid, connected, and exclusively owned.
        // File::from_raw_handle takes ownership of the handle and closes it on drop.
        let file = unsafe { File::from_raw_handle(handle.0 as *mut _) };
        Ok(file)
    }

    /// Non-Windows stub returning unsupported error.
    #[cfg(not(windows))]
    pub fn accept(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Named pipes are only supported on Windows",
        ))
    }
}
