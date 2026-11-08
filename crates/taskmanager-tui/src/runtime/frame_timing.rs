//! Opt-in frame-timing sink for the TUI's dirty-draw frame loop.
//!
//! `TM_FRAME_TIMING_SINK` names a JSONL file. When it is absent the runtime
//! constructs no recorder and opens no file. When present, one
//! [`FrameTimingRecord`] is appended for each real `terminal.draw`: idle
//! cycles that skip drawing are not frames and are never reported as one.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use taskmanager_app_host::{DEFAULT_MAX_SAMPLES, SINK_ENV};
use zhuge_ui_core::frame_timing::record::{FrameTimingIdentity, FrameTimingRecord};
use zhuge_ui_core::frame_timing::{FrameTimingObservation, FrameTimingRecorder};
use zhuge_ui_core::frame_timing::{budget_from_var, sink_from_var};

const BUDGET_ENV: &str = "TM_FRAME_TIMING_BUDGET_MS";
const MARKER_ENV: &str = "TM_TUI_CAPTURE_MARKER_FILE";
const STUTTER_THRESHOLD_MS: f32 = 100.0;

/// One opt-in recorder, owned by the runtime loop.
pub(crate) struct FrameTimingSink {
    recorder: FrameTimingRecorder,
    identity: FrameTimingIdentity,
    sink: PathBuf,
    receipts: Vec<String>,
    last_frame_start: Option<Instant>,
    frame_id: u64,
}

impl FrameTimingSink {
    /// Construct the sink only when the product opted in.
    pub(crate) fn from_env() -> Option<Self> {
        let sink = sink_from_var(std::env::var_os(SINK_ENV))?;
        let budget_ms = budget_from_var(std::env::var_os(BUDGET_ENV), STUTTER_THRESHOLD_MS);
        Some(Self {
            recorder: FrameTimingRecorder::new(DEFAULT_MAX_SAMPLES, budget_ms),
            identity: FrameTimingIdentity::new("taskforest", "taskforest-tui", "tui"),
            sink,
            receipts: std::env::var_os(MARKER_ENV)
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| vec![path.to_string_lossy().into_owned()])
                .unwrap_or_default(),
            last_frame_start: None,
            frame_id: 0,
        })
    }

    /// Record one real draw. `work_started_at` is taken just before the
    /// terminal backend draws, so `execution_ms` is this frontend's work.
    pub(crate) fn record_draw(&mut self, work_started_at: Instant) {
        let frame_end = Instant::now();
        let interval = self
            .last_frame_start
            .map(|previous| frame_end.duration_since(previous));
        let execution = frame_end.duration_since(work_started_at);
        let observation = FrameTimingObservation::new(self.frame_id, interval, Some(execution));
        // The owner answers "did this frame stutter?" as it records it. Diffing
        // `stutter_count()` re-derives that decision a second time, and the two
        // answers diverge the moment the threshold rule changes.
        let stuttered = self.recorder.record_frame(observation);
        let reason = interval
            .is_none()
            .then(|| "first drawn frame has no interval".to_owned());
        let record = FrameTimingRecord::new(
            self.identity.clone(),
            observation,
            &self.recorder,
            reason,
            stuttered,
            if stuttered {
                self.receipts.clone()
            } else {
                Vec::new()
            },
        );
        if let Ok(line) = record.encode()
            && let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.sink)
        {
            let _ = writeln!(file, "{line}");
        }
        self.last_frame_start = Some(frame_end);
        self.frame_id = self.frame_id.saturating_add(1);
    }
}

// One short path mount keeps the test body in the headless test tree (the
// workspace layout policy forbids inline `#[cfg(test)] mod tests` here).
#[cfg(test)]
#[path = "../../tests/headless/runtime/frame_timing_tests.rs"]
mod frame_timing_tests;
