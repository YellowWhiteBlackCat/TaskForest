use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;

use taskmanager_application::i18n::Language;

use crate::terminal::TuiTerminalProfile;

pub(super) struct CaptureProfileReceipt {
    path: Option<OsString>,
    last_frame: Option<(TuiTerminalProfile, Language)>,
    marked: bool,
}

impl CaptureProfileReceipt {
    pub(super) fn new(path: Option<&OsStr>) -> Self {
        Self {
            path: path.map(OsStr::to_os_string),
            last_frame: None,
            marked: false,
        }
    }

    pub(super) fn record_frame(&mut self, profile: TuiTerminalProfile, language: Language) {
        self.last_frame = Some((profile, language));
    }

    pub(super) const fn pending(&self) -> bool {
        self.path.is_some() && !self.marked
    }

    pub(super) fn write_frame_ready(&mut self, page: &str) -> io::Result<()> {
        if let (Some(path), Some((profile, language))) = (self.path.as_deref(), self.last_frame) {
            write_frame_ready(path, page, profile, language)?;
            self.marked = true;
        }
        Ok(())
    }
}

fn write_frame_ready(
    path: &OsStr,
    page: &str,
    profile: TuiTerminalProfile,
    language: Language,
) -> io::Result<()> {
    fs::write(
        path,
        format!(
            "TUI_CAPTURE_MARKER event=demo_data_ready mode=demo\n\
             TUI_CAPTURE_MARKER event=frame_ready page={page}\n\
             TUI_CAPTURE_MARKER event=terminal_profile language={} glyphs={} color={}\n",
            language_name(language),
            profile.glyphs_name(),
            profile.color_name(),
        ),
    )
}

const fn language_name(language: Language) -> &'static str {
    match language {
        Language::En => "en",
        Language::Zh => "zh",
    }
}
