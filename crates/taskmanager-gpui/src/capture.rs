//! Windows-only evidence mode (`--capture-window <out>`): opens the real app
//! window, waits for it to paint, captures its own composited frames once via
//! Windows.Graphics.Capture (through the mature safe `windows-capture` crate),
//! and writes `capture.png` + `capture-metadata.txt` + `capture-manifest.tsv`
//! into the output directory before exiting 0.
//!
//! This is the in-process self-capture route: no external tool, no PowerShell,
//! no new audited boundary crate. The pixels come from the real DirectX
//! compositor frame of the actual app window — not the test renderer, which is
//! a no-op in gpui 0.2.2 (`src/platform/test/window.rs: draw() {}`), so this
//! is the only in-process path that yields real pixels.
//!
//! `zed-scap` 0.0.8-zed (gpui's optional screen-capture dependency) does not
//! compile against either published `windows-capture` version, so this module
//! talks to `windows-capture` directly (its `GraphicsCaptureApiHandler` +
//! `Frame::buffer().save_as_image`); the Windows ledger records the choice.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{App, Application, AsyncApp};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use sha2::{Digest, Sha256};
use tracing::{error, info};
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::{Frame, ImageFormat};
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::assets::TaskManagerAssets;

/// The evidence-mode entry point; `src/frontend.rs` routes here on
/// Windows + ui-gpui. Runs the real app to completion and returns the capture
/// outcome (errors are typed strings; success writes the evidence files).
pub fn run(out: &Path) -> Result<(), String> {
    let out = out.to_path_buf();
    let outcome: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let outcome_task = outcome.clone();
    let host = taskmanager_app_host::NativeAppHost::production();
    let config_client = host
        .config_client()
        .map_err(|error| format!("configuration runtime unavailable: {error}"))?;
    let snapshot_export_client = host
        .snapshot_export_client()
        .map_err(|error| format!("snapshot export runtime unavailable: {error}"))?;
    let window_capture_client = host
        .window_capture_client()
        .map_err(|error| format!("window capture runtime unavailable: {error}"))?;
    let diagnostic_bundle_client = host
        .diagnostic_bundle_client()
        .map_err(|error| format!("diagnostic bundle runtime unavailable: {error}"))?;
    let service_log_export_client = host
        .diagnostic_bundle_client()
        .map_err(|error| format!("service log export runtime unavailable: {error}"))?;
    let native_locale_name = host.native_locale_name();
    let local_time_rules = host.local_time_rules();
    let platform_factory = host.clone();
    let history_factory = host.clone();
    Application::new()
        .with_assets(TaskManagerAssets)
        .run(move |cx: &mut App| {
            if let Err(composition_error) = crate::gpui_app::init(
                cx,
                move || platform_factory.spawn_client(),
                crate::gpui_app::StartupRuntime {
                    config_client,
                    snapshot_export_client,
                    window_capture_client,
                    diagnostic_bundle_client,
                    service_log_export_client,
                    history_connector: history_factory.history_frontend_connector(),
                },
                crate::gpui_app::StartupEnvironment {
                    native_locale_name,
                    local_time_rules,
                    custom_app_id: None,
                    presentation: taskmanager_app_host::WindowPresentation::standalone(),
                },
            ) {
                error!(%composition_error, "native platform composition failed");
                record_outcome(
                    &outcome_task,
                    Err(format!(
                        "native platform composition failed: {composition_error}"
                    )),
                );
                cx.quit();
                return;
            }
            info!("capture mode: window created; waiting for painted frames");
            let task = cx.spawn(async move |cx: &mut AsyncApp| {
                let result = capture_after_first_frames(&out, cx).await;
                record_outcome(&outcome_task, result);
                let _ = cx.update(|cx| cx.quit());
            });
            task.detach();
        });
    match outcome.lock() {
        Ok(guard) => guard
            .clone()
            .unwrap_or_else(|| Err("capture mode ended without an outcome".to_owned())),
        Err(_) => Err("capture outcome lock poisoned".to_owned()),
    }
}

/// One captured, CPU-readable frame.
struct CapturedFrame {
    width: u32,
    height: u32,
    /// Raw PNG bytes (written by the capture handler, re-read for hashing).
    png: Vec<u8>,
    /// Absolute path to the written `capture.png`.
    png_path: PathBuf,
}

async fn capture_after_first_frames(out: &Path, cx: &mut AsyncApp) -> Result<(), String> {
    wait_for_window(cx).await?;
    // Let the window paint several frames (the initial draw plus telemetry-
    // driven redraws) so the composited grab shows steady-state content.
    wait(Duration::from_millis(2000), cx).await;
    let hwnd = window_hwnd(cx).await?;
    eprintln!("capture mode: resolved HWND = {hwnd:#x} ({hwnd})");
    let out = out.to_path_buf();
    let frame = cx
        .background_executor()
        .spawn(async move { capture_window_blocking(hwnd, out) })
        .await
        .map_err(|error| format!("window capture failed: {error}"))?;
    write_evidence(&frame)?;
    info!(
        "capture mode: wrote evidence into {}",
        frame.png_path.display()
    );
    Ok(())
}

async fn wait_for_window(cx: &mut AsyncApp) -> Result<(), String> {
    for _ in 0..100 {
        let count = cx.update(|cx| cx.windows().len()).unwrap_or(0);
        if count > 0 {
            return Ok(());
        }
        wait(Duration::from_millis(100), cx).await;
    }
    Err("no application window appeared within 10s".to_owned())
}

/// Resolve the first application window's native `HWND` value (as an isize) so
/// the capture target can be bound to our own window.
async fn window_hwnd(cx: &mut AsyncApp) -> Result<isize, String> {
    for _ in 0..20 {
        let found = cx
            .update(|cx| {
                let mut hwnd = None;
                for handle in cx.windows() {
                    let _ = handle.update(cx, |_, window, _| {
                        let Ok(raw) = window.window_handle() else {
                            return;
                        };
                        if let RawWindowHandle::Win32(win32) = raw.as_raw() {
                            hwnd = Some(win32.hwnd.get());
                        }
                    });
                    if hwnd.is_some() {
                        break;
                    }
                }
                hwnd
            })
            .unwrap_or(None);
        if let Some(hwnd) = found {
            return Ok(hwnd);
        }
        wait(Duration::from_millis(100), cx).await;
    }
    Err("could not resolve the application window's native handle".to_owned())
}

/// Capture handler: skips the first couple of transition frames, then saves
/// the steady-state frame as PNG and stops the capture session.
struct SelfCaptureHandler {
    out: PathBuf,
    size: Arc<Mutex<Option<(u32, u32)>>>,
}

impl GraphicsCaptureApiHandler for SelfCaptureHandler {
    type Flags = (PathBuf, Arc<Mutex<Option<(u32, u32)>>>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            out: ctx.flags.0,
            size: ctx.flags.1,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if let Ok(mut guard) = self.size.lock() {
            *guard = Some((frame.width(), frame.height()));
        }
        frame
            .buffer()?
            .save_as_image(self.out.join("capture.png"), ImageFormat::Png)?;
        capture_control.stop();
        Ok(())
    }
}

/// Blocking capture of OUR OWN window (runs on the background executor, never
/// the UI thread). Windows.Graphics.Capture needs no picker prompt for a
/// window the process itself owns.
fn capture_window_blocking(hwnd: isize, out: PathBuf) -> Result<CapturedFrame, String> {
    std::fs::create_dir_all(&out).map_err(|error| {
        format!(
            "creating output directory {} failed: {error}",
            out.display()
        )
    })?;

    let size = Arc::new(Mutex::new(None));
    let target_window = Window::enumerate()
        .ok()
        .and_then(|windows| {
            windows.into_iter().find(|w| {
                w.as_raw_hwnd() == (hwnd as *mut std::ffi::c_void)
                    || w.title().is_ok_and(|t| {
                        t.contains("任务森林") || t.contains(taskmanager_assets::product::GPUI_NAME)
                    })
            })
        })
        .unwrap_or_else(|| Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void));

    let settings = Settings::new(
        target_window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (out.clone(), size.clone()),
    );

    SelfCaptureHandler::start(settings)
        .map_err(|error| format!("running the window capture session failed: {error}"))?;
    let png_path = out.join("capture.png");
    let png_bytes = std::fs::read(&png_path).map_err(|error| {
        format!(
            "reading the captured PNG {} failed: {error}",
            png_path.display()
        )
    })?;
    let (width, height) = match size.lock() {
        Ok(guard) => {
            (*guard).ok_or_else(|| "capture handler never reported the frame size".to_owned())?
        }
        Err(_) => return Err("capture size lock poisoned".to_owned()),
    };
    Ok(CapturedFrame {
        width,
        height,
        png: png_bytes,
        png_path,
    })
}

/// Write `capture-metadata.txt` and `capture-manifest.tsv` (the PNG itself is
/// already written by the capture handler). The app writes only app-side
/// facts; the runner script merges git head/worktree state into its receipt.
fn write_evidence(frame: &CapturedFrame) -> Result<(), String> {
    let png_sha256 = hex_digest(&frame.png);
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let metadata = format!(
        "mode=capture-window\n\
         timestamp={timestamp}\n\
         png=capture.png\n\
         png_sha256={png_sha256}\n\
         window_size={}x{}\n\
         capture_api=windows.graphics.capture (windows-capture)\n",
        frame.width, frame.height
    );
    let out = frame
        .png_path
        .parent()
        .ok_or_else(|| "capture PNG path has no parent directory".to_owned())?;
    std::fs::write(out.join("capture-metadata.txt"), metadata)
        .map_err(|error| format!("writing capture metadata failed: {error}"))?;

    let manifest = format!(
        "timestamp\tgit_head\tworktree\twindow_size\tpng_sha256\tpng\n\
         {timestamp}\t(runner merges git head)\t-\t{}x{}\t{png_sha256}\tcapture.png\n",
        frame.width, frame.height
    );
    std::fs::write(out.join("capture-manifest.tsv"), manifest)
        .map_err(|error| format!("writing the capture manifest failed: {error}"))?;
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn wait(duration: Duration, cx: &mut AsyncApp) {
    let _ = cx.background_executor().timer(duration).await;
}

fn record_outcome(outcome: &Mutex<Option<Result<(), String>>>, result: Result<(), String>) {
    if let Ok(mut guard) = outcome.lock() {
        *guard = Some(result);
    }
}
