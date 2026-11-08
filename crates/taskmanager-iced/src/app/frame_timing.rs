//! Opt-in frame-timing sink for the Iced frontend.
//!
//! `TM_FRAME_TIMING_SINK` names a JSONL file. A normal launch constructs no
//! recorder, subscribes to no extra frame pump and opens no file. When the
//! product opts in, the subscription adds the native `frames()` stream and
//! each `Message::Frame(Instant)` appends one typed record.
//!
//! The sink lives outside `IcedApp`: the application state owns product
//! semantics, while this module owns only a soak-only observation whose
//! lifetime is the process. A mutex keeps the opt-in state process-global
//! without adding an I/O field to the renderer state.

use std::time::Instant;

use taskmanager_app_host::{
    DEFAULT_GRAPHICAL_BUDGET_MS, DEFAULT_MAX_SAMPLES, observe_opt_in_frame, sink_requested,
};
use zhuge_ui_core::frame_timing::record::FrameTimingIdentity;

const MARKER_ENV: &str = "TM_ICED_CAPTURE_MARKER_FILE";

/// True only when the product explicitly asked for a frame-timing sink.
pub(crate) fn active() -> bool {
    sink_requested()
}

/// Feed one native Iced frame to the frame-timing owner.
///
/// The owner decides stutter; this frontend only hands over the observation and
/// keeps the receipt. `observe_frame` (not `record_frame`) so the frontend never
/// reads like a second authority for the same fact.
pub(crate) fn observe_frame(now: Instant) {
    observe_opt_in_frame(
        FrameTimingIdentity::new("taskforest", "taskforest-iced", "iced"),
        MARKER_ENV,
        DEFAULT_MAX_SAMPLES,
        DEFAULT_GRAPHICAL_BUDGET_MS,
        now,
        "first frame has no interval; iced frame hook does not observe execution time",
        "iced frame hook does not observe execution time",
    );
}
