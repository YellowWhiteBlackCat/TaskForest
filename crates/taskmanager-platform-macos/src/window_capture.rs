//! macOS native window capture adapter.
//!
//! macOS desktop window capture will provide native ScreenCaptureKit / CLI
//! fallbacks when in-process frame capture is unavailable. Until that seam is
//! wired, it reports the typed `Unsupported` failure cleanly.

use std::path::Path;

use taskmanager_platform_contract::{
    NativeWindowCapture, WindowCaptureFailure, WindowCaptureFailureKind, WindowCaptureReceipt,
};

/// macOS native window capture implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacosWindowCapture;

impl NativeWindowCapture for MacosWindowCapture {
    fn capture_active_window(
        &self,
        _output: &Path,
    ) -> Result<WindowCaptureReceipt, WindowCaptureFailure> {
        Err(WindowCaptureFailure::new(
            WindowCaptureFailureKind::Unsupported,
            "native macOS desktop window capture is not yet implemented",
        ))
    }
}
