//! Windows native window capture adapter.
//!
//! Captures the active foreground window using Windows.Graphics.Capture via
//! the safe `windows-capture` crate.

use std::path::Path;

#[cfg(windows)]
use taskmanager_platform_contract::WindowCaptureBackend;
use taskmanager_platform_contract::{
    NativeWindowCapture, WindowCaptureFailure, WindowCaptureFailureKind, WindowCaptureReceipt,
};

/// Windows native window capture implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsWindowCapture;

#[cfg(windows)]
struct CaptureHandler {
    out: std::path::PathBuf,
    dims: std::sync::Arc<std::sync::Mutex<Option<(u32, u32)>>>,
}

#[cfg(windows)]
impl windows_capture::capture::GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = (
        std::path::PathBuf,
        std::sync::Arc<std::sync::Mutex<Option<(u32, u32)>>>,
    );
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: windows_capture::capture::Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            out: ctx.flags.0,
            dims: ctx.flags.1,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut windows_capture::frame::Frame,
        capture_control: windows_capture::graphics_capture_api::InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if let Ok(mut guard) = self.dims.lock() {
            *guard = Some((frame.width(), frame.height()));
        }
        frame
            .buffer()?
            .save_as_image(&self.out, windows_capture::frame::ImageFormat::Png)?;
        capture_control.stop();
        Ok(())
    }
}

impl NativeWindowCapture for WindowsWindowCapture {
    fn capture_active_window(
        &self,
        output: &Path,
    ) -> Result<WindowCaptureReceipt, WindowCaptureFailure> {
        #[cfg(windows)]
        {
            use windows_capture::capture::GraphicsCaptureApiHandler;

            let target_window = windows_capture::window::Window::foreground().map_err(|err| {
                WindowCaptureFailure::new(
                    WindowCaptureFailureKind::ProviderUnavailable,
                    format!("failed to locate foreground window: {err}"),
                )
            })?;

            let dims = std::sync::Arc::new(std::sync::Mutex::new(None));
            let settings = windows_capture::settings::Settings::new(
                target_window,
                windows_capture::settings::CursorCaptureSettings::WithoutCursor,
                windows_capture::settings::DrawBorderSettings::WithoutBorder,
                windows_capture::settings::SecondaryWindowSettings::Default,
                windows_capture::settings::MinimumUpdateIntervalSettings::Default,
                windows_capture::settings::DirtyRegionSettings::Default,
                windows_capture::settings::ColorFormat::Bgra8,
                (output.to_path_buf(), dims.clone()),
            );

            CaptureHandler::start(settings).map_err(|err| {
                WindowCaptureFailure::new(
                    WindowCaptureFailureKind::ProviderFault,
                    format!("windows graphics capture failed: {err}"),
                )
            })?;

            let (width, height) = dims.lock().ok().and_then(|g| *g).ok_or_else(|| {
                WindowCaptureFailure::new(
                    WindowCaptureFailureKind::InvalidImage,
                    "windows graphics capture did not record frame dimensions",
                )
            })?;

            Ok(WindowCaptureReceipt::new(
                width,
                height,
                WindowCaptureBackend::WindowsGraphicsCapture,
            ))
        }
        #[cfg(not(windows))]
        {
            let _ = output;
            Err(WindowCaptureFailure::new(
                WindowCaptureFailureKind::Unsupported,
                "Windows window capture is only available when compiling for Windows",
            ))
        }
    }
}
