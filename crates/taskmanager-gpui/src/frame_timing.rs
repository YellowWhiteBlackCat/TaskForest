//! Opt-in frame-timing sink for the GPUI frontend.
//!
//! `TM_FRAME_TIMING_SINK` names a JSONL file. A normal launch constructs no
//! recorder and opens no file. When the product opts in, every
//! `RootView::render` appends one typed record. GPUI's `render` is the native
//! paint hook, so the interval between renders is the delivered cadence;
//! execution time is not observable from that hook and stays `null` with an
//! explicit reason rather than being fabricated as zero.

use std::time::Instant;

use taskmanager_app_host::{
    DEFAULT_GRAPHICAL_BUDGET_MS, DEFAULT_MAX_SAMPLES, observe_opt_in_frame,
};
use zhuge_ui_core::frame_timing::record::FrameTimingIdentity;

const RECEIPT_ENV: &str = "TM_GPUI_FRAME_TIMING_RECEIPT";

/// Feed one native GPUI paint to the frame-timing owner.
///
/// The name deliberately does not repeat `FrameTimingRecorder::record_frame`:
/// that is the owner's face, and a frontend that also calls itself `record_frame`
/// makes it look like two places decide what a recorded frame means.
pub(crate) fn observe_frame(now: Instant) {
    observe_opt_in_frame(
        FrameTimingIdentity::new("taskforest", "taskforest-gpui", "gpui"),
        RECEIPT_ENV,
        DEFAULT_MAX_SAMPLES,
        DEFAULT_GRAPHICAL_BUDGET_MS,
        now,
        "first frame has no interval; gpui render hook does not observe execution time",
        "gpui render hook does not observe execution time",
    );
}
