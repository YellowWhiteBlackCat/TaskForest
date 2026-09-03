//! Linux current-window screenshot adapter.
//!
//! The first one-shot backend uses KDE Spectacle's native Wayland capture
//! implementation through a fixed argv.  This is deliberately behind a small
//! backend seam: a future Portal Screenshot backend can replace it when the
//! compositor advertises an active-window target, and the continuous
//! ScreenCast/PipeWire backend can share the same output validation without
//! changing application state ownership.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use taskmanager_platform_contract::{
    WindowCaptureBackend, WindowCaptureFailure, WindowCaptureFailureKind, WindowCaptureReceipt,
};
use taskmanager_platform_portable::{BoundedCommandError, run_with_timeout};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PNG_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DIMENSION: u32 = 32_768;
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

use taskmanager_platform_contract::NativeWindowCapture;

/// Linux native window capture using KDE Spectacle.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxWindowCapture;

impl NativeWindowCapture for LinuxWindowCapture {
    fn capture_active_window(
        &self,
        output: &Path,
    ) -> Result<WindowCaptureReceipt, WindowCaptureFailure> {
        SpectacleActiveWindowBackend.capture(output)
    }
}

trait OneShotWindowCaptureBackend {
    fn capture(&self, output: &Path) -> Result<WindowCaptureReceipt, WindowCaptureFailure>;
}

struct SpectacleActiveWindowBackend;

impl OneShotWindowCaptureBackend for SpectacleActiveWindowBackend {
    fn capture(&self, output: &Path) -> Result<WindowCaptureReceipt, WindowCaptureFailure> {
        let mut command = Command::new("spectacle");
        command.args([
            "--activewindow",
            "--background",
            "--nonotify",
            "--no-decoration",
            "--no-shadow",
            "--output",
        ]);
        command.arg(output);

        let result =
            run_with_timeout(&mut command, CAPTURE_TIMEOUT).map_err(classify_command_error)?;
        if !result.status.success() {
            let detail = String::from_utf8_lossy(&result.stderr);
            return Err(WindowCaptureFailure::new(
                WindowCaptureFailureKind::ProviderFault,
                if detail.trim().is_empty() {
                    format!("spectacle exited unsuccessfully: {}", result.status)
                } else {
                    format!("spectacle failed: {}", detail.trim())
                },
            ));
        }

        let (width, height) = inspect_png(output)?;
        Ok(WindowCaptureReceipt::new(
            width,
            height,
            WindowCaptureBackend::SpectacleActiveWindow,
        ))
    }
}

/// Capture the active Wayland window into an already allocated staging path.
///
/// This is a fixed executable invocation, not a command-interpreter shell-out:
/// the executable and all arguments are fixed except for the caller-owned
/// staging path. The caller must atomically publish the validated staging file
/// after this returns.
pub fn capture_current_window_png(
    output: &Path,
) -> Result<WindowCaptureReceipt, WindowCaptureFailure> {
    LinuxWindowCapture.capture_active_window(output)
}

fn classify_command_error(error: BoundedCommandError) -> WindowCaptureFailure {
    let (kind, detail) = match error {
        BoundedCommandError::Spawn(error) if error.kind() == io::ErrorKind::NotFound => (
            WindowCaptureFailureKind::ProviderUnavailable,
            "KDE Spectacle is not installed".to_owned(),
        ),
        BoundedCommandError::Spawn(error) if error.kind() == io::ErrorKind::PermissionDenied => (
            WindowCaptureFailureKind::PermissionDenied,
            format!("cannot execute KDE Spectacle: {error}"),
        ),
        BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => (
            WindowCaptureFailureKind::TimedOut,
            "KDE Spectacle did not finish the active-window capture before the deadline".to_owned(),
        ),
        other => (
            WindowCaptureFailureKind::ProviderFault,
            format!("KDE Spectacle invocation failed: {other:?}"),
        ),
    };
    WindowCaptureFailure::new(kind, detail)
}

fn inspect_png(path: &Path) -> Result<(u32, u32), WindowCaptureFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        WindowCaptureFailure::new(
            WindowCaptureFailureKind::Io,
            format!("inspect screenshot output: {error}"),
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::InvalidImage,
            "screenshot backend did not produce a regular PNG file",
        ));
    }
    if metadata.len() < 33 || metadata.len() > MAX_PNG_BYTES {
        return Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::InvalidImage,
            format!(
                "screenshot PNG has an invalid size: {} bytes",
                metadata.len()
            ),
        ));
    }

    let mut header = [0_u8; 33];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| {
            WindowCaptureFailure::new(
                WindowCaptureFailureKind::Io,
                format!("read screenshot PNG header: {error}"),
            )
        })?;

    if header[..8] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::InvalidImage,
            "screenshot output is not a PNG with an IHDR header",
        ));
    }
    let ihdr_length = match <[u8; 4]>::try_from(&header[8..12]) {
        Ok(bytes) => u32::from_be_bytes(bytes),
        Err(_) => {
            return Err(WindowCaptureFailure::new(
                WindowCaptureFailureKind::InvalidImage,
                "screenshot PNG header is truncated",
            ));
        }
    };
    if ihdr_length != 13 {
        return Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::InvalidImage,
            "screenshot PNG has an invalid IHDR length",
        ));
    }

    let width = match <[u8; 4]>::try_from(&header[16..20]) {
        Ok(bytes) => u32::from_be_bytes(bytes),
        Err(_) => 0,
    };
    let height = match <[u8; 4]>::try_from(&header[20..24]) {
        Ok(bytes) => u32::from_be_bytes(bytes),
        Err(_) => 0,
    };
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::InvalidImage,
            format!("screenshot PNG dimensions are invalid: {width}x{height}"),
        ));
    }
    Ok((width, height))
}

#[cfg(test)]
#[path = "../tests/headless/window_capture.rs"]
mod tests;
