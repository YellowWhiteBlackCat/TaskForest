//! Capture-only marker lifecycle owned by the Iced frontend boundary.

use std::path::PathBuf;

pub(super) struct CaptureState {
    pub(super) marker: Option<PathBuf>,
    pub(super) emitted: bool,
}

impl CaptureState {
    pub(super) const fn new(marker: Option<PathBuf>) -> Self {
        Self {
            marker,
            emitted: false,
        }
    }
}
