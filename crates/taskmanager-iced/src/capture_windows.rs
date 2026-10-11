//! Windows.Graphics.Capture helper for the Iced product binary.
//!
//! The runner starts the real `taskforest-i --demo` product process and waits
//! for its frame, target, and PID markers. This helper process then captures
//! the uniquely matching HWND by that PID and the product's actual window
//! title. It never captures a monitor or guesses a window from a scenario
//! name.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use taskmanager_assets::product;
use taskmanager_iced::theme_binding::font_weight;
use taskmanager_theme::color::Weight;
use taskmanager_theme::platform::WeightCompensationAxis;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::{Frame, ImageFormat};
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

struct CapturedFrame {
    width: u32,
    height: u32,
    png_path: PathBuf,
}

struct OneFrameCapture {
    png_path: PathBuf,
    size: Arc<Mutex<Option<(u32, u32)>>>,
}

impl GraphicsCaptureApiHandler for OneFrameCapture {
    type Flags = (PathBuf, Arc<Mutex<Option<(u32, u32)>>>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            png_path: context.flags.0,
            size: context.flags.1,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if let Ok(mut size) = self.size.lock() {
            *size = Some((frame.width(), frame.height()));
        }
        frame
            .buffer()?
            .save_as_image(&self.png_path, ImageFormat::Png)?;
        control.stop();
        Ok(())
    }
}

/// Capture the exact Iced process whose PID came from its own marker file.
pub(super) fn run(out: &Path) -> Result<(), String> {
    let expected_pid = std::env::var("TM_ICED_CAPTURE_TARGET_PID")
        .map_err(|_| {
            "missing TM_ICED_CAPTURE_TARGET_PID from the Iced readiness marker".to_owned()
        })?
        .parse::<u32>()
        .map_err(|error| format!("invalid Iced target PID: {error}"))?;
    if expected_pid == 0 {
        return Err("Iced target PID must be positive".to_owned());
    }
    std::fs::create_dir_all(out).map_err(|error| {
        format!(
            "creating Iced capture directory {} failed: {error}",
            out.display()
        )
    })?;

    let target = find_target_window(expected_pid)?;
    let title = target
        .title()
        .map_err(|error| format!("reading Iced window title failed: {error}"))?;
    let process_name = target
        .process_name()
        .map_err(|error| format!("reading Iced process name failed: {error}"))?;
    let window_rect = target
        .rect()
        .map_err(|error| format!("reading Iced window rectangle failed: {error}"))?;
    let window_width = positive_extent(window_rect.left, window_rect.right, "window width")?;
    let window_height = positive_extent(window_rect.top, window_rect.bottom, "window height")?;

    let frame = capture_one_frame(target, out)?;
    let weight_axis = WeightCompensationAxis::target();
    let weight_450 = font_weight(Weight(450.0));
    let weight_name = format!("{weight_450:?}");
    let weight_value = iced_weight_value(&weight_name)?;
    let axis_name = match weight_axis {
        WeightCompensationAxis::FreeType => "freetype",
        WeightCompensationAxis::DirectWrite => "directwrite-policy",
    };

    let target_metadata = format!(
        "capture_api=windows.graphics.capture (windows-capture 1.5.0)\n\
         target_pid={expected_pid}\n\
         window_process_name={process_name}\n\
         window_title={title}\n\
         window_rect_size={window_width}x{window_height}\n\
         window_size={}x{}\n\
         renderer_active=unknown\n\
         text_shaping=cosmic-text Advanced Auto default; widget overrides may vary\n\
         text_rasterizer=cryoglyph for WGPU; cosmic-text+swash for tiny-skia fallback\n\
         text_rasterizer_active=unknown\n\
         native_text_api=unknown\n\
         font_weight_axis_policy={axis_name}\n\
         font_weight_450_name={weight_name}\n\
         font_weight_450_value={weight_value}\n",
        frame.width, frame.height
    );
    std::fs::write(out.join("capture-target.txt"), target_metadata)
        .map_err(|error| format!("writing Iced capture target metadata failed: {error}"))?;
    if !frame.png_path.is_file() {
        return Err("Windows.Graphics.Capture did not write capture.png".to_owned());
    }
    Ok(())
}

fn find_target_window(expected_pid: u32) -> Result<Window, String> {
    let windows = Window::enumerate()
        .map_err(|error| format!("enumerating Windows.Graphics.Capture targets failed: {error}"))?;
    let matches: Vec<_> = windows
        .into_iter()
        .filter(|window| {
            window.process_id().ok() == Some(expected_pid)
                && window
                    .title()
                    .is_ok_and(|title| title == product::ICED_NAME)
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one TaskForestI window for marker PID {expected_pid}, found {}",
            matches.len()
        ));
    }
    matches
        .into_iter()
        .next()
        .ok_or_else(|| "the exact Iced capture target disappeared".to_owned())
}

fn positive_extent(start: i32, end: i32, name: &str) -> Result<u32, String> {
    u32::try_from(i64::from(end) - i64::from(start))
        .ok()
        .filter(|size| *size > 0)
        .ok_or_else(|| format!("Windows reported an invalid {name}: start={start}, end={end}"))
}

fn capture_one_frame(target: Window, out: &Path) -> Result<CapturedFrame, String> {
    let png_path = out.join("capture.png");
    let size = Arc::new(Mutex::new(None));
    let settings = Settings::new(
        target,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (png_path.clone(), size.clone()),
    );
    OneFrameCapture::start(settings)
        .map_err(|error| format!("Windows.Graphics.Capture session failed: {error}"))?;
    let (width, height) = size
        .lock()
        .map_err(|_| "Iced capture size lock was poisoned".to_owned())?
        .ok_or_else(|| "the Iced capture handler produced no frame size".to_owned())?;
    Ok(CapturedFrame {
        width,
        height,
        png_path,
    })
}

fn iced_weight_value(name: &str) -> Result<u16, String> {
    match name {
        "Thin" => Ok(100),
        "ExtraLight" => Ok(200),
        "Light" => Ok(300),
        "Normal" => Ok(400),
        "Medium" => Ok(500),
        "Semibold" => Ok(600),
        "Bold" => Ok(700),
        "ExtraBold" => Ok(800),
        "Black" => Ok(900),
        _ => Err(format!("unrecognized Iced font weight result: {name}")),
    }
}
