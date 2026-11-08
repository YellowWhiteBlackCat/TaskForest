//! Capture-only deterministic primary-window frame export.
//!
//! `TM_BEVY_SYNC_FRAME_DIR` asks the shared owner in `zhuge-ui-kit-bevy` to
//! export one native PNG per rendered frame of the primary window. The capture
//! contract — request pacing, frame file naming and the capture-run exit —
//! is owned there; this module names the product's environment variables and
//! nothing else.

use zhuge_ui_kit_bevy::frame_capture::{DEFAULT_FRAME_COUNT, SyncFrameEnv};

pub(crate) const SYNC_FRAME_ENV: SyncFrameEnv = SyncFrameEnv {
    directory_var: "TM_BEVY_SYNC_FRAME_DIR",
    count_var: "TM_BEVY_SYNC_FRAME_COUNT",
    default_frame_count: DEFAULT_FRAME_COUNT,
};
