//! Process-wide opt-in frame-timing sink shared by the renderer frontends.
//!
//! `zhuge-ui-core` owns the recorder, the observation vocabulary and the wire
//! record. The frontend hooks keep their own env names, identity and copy;
//! this module owns the sink state they previously duplicated: one opt-in
//! constructor and one `observe` that feeds a native frame to the owner and
//! appends one typed JSONL line. A normal launch constructs no sink and opens
//! no file.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use zhuge_ui_core::frame_timing::record::{FrameTimingIdentity, FrameTimingRecord};
use zhuge_ui_core::frame_timing::{FrameTimingObservation, FrameTimingRecorder};
use zhuge_ui_core::frame_timing::{budget_from_var, sink_from_var};

/// The env variable that names the sink path — one address for the opt-in
/// contract every frontend binds.
pub const SINK_ENV: &str = "TM_FRAME_TIMING_SINK";
const BUDGET_ENV: &str = "TM_FRAME_TIMING_BUDGET_MS";

/// The bounded sample window the graphical frontends share.
pub const DEFAULT_MAX_SAMPLES: usize = 240;

/// The stutter budget the graphical frontends share (60 Hz delivery).
pub const DEFAULT_GRAPHICAL_BUDGET_MS: f32 = 16.0;

/// True only when the product asked for a frame-timing sink through
/// [`SINK_ENV`]. A normal launch constructs no sink and opens no file.
#[must_use]
pub fn sink_requested() -> bool {
    sink_from_var(std::env::var_os(SINK_ENV)).is_some()
}

static SHARED_SINK: OnceLock<Mutex<Option<FrameTimingSink>>> = OnceLock::new();

/// One frontend's opt-in frame-timing sink: the bounded recorder plus the
/// JSONL path and the capture receipts bound to stuttered frames.
///
/// The sink records only what the frontend's native frame hook observes. The
/// GPUI render hook and the Iced frame pump cannot observe execution time, so
/// [`FrameTimingSink::observe`] keeps that fact unobserved (`null` in the
/// record) instead of fabricating a zero.
#[derive(Debug)]
pub struct FrameTimingSink {
    recorder: FrameTimingRecorder,
    identity: FrameTimingIdentity,
    path: PathBuf,
    receipts: Vec<String>,
    last_frame: Option<Instant>,
    frame_id: u64,
}

impl FrameTimingSink {
    /// Construct the sink from one explicit product binding.
    ///
    /// `receipts` holds the frontend's capture-receipt paths; the owner
    /// attaches them to stuttered frames only.
    #[must_use]
    pub fn new(
        identity: FrameTimingIdentity,
        path: PathBuf,
        receipts: Vec<String>,
        max_samples: usize,
        budget_ms: f32,
    ) -> Self {
        Self {
            recorder: FrameTimingRecorder::new(max_samples, budget_ms),
            identity,
            path,
            receipts,
            last_frame: None,
            frame_id: 0,
        }
    }

    /// Construct the sink only when the product opted in through the shared
    /// frame-timing environment (`TM_FRAME_TIMING_SINK` with the shared
    /// budget default). `receipt_env` names the frontend's own receipt
    /// variable.
    #[must_use]
    fn from_env(
        identity: FrameTimingIdentity,
        receipt_env: &str,
        max_samples: usize,
        default_budget_ms: f32,
    ) -> Option<Self> {
        let path = sink_from_var(std::env::var_os(SINK_ENV))?;
        let receipts = std::env::var_os(receipt_env)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| vec![path.to_string_lossy().into_owned()])
            .unwrap_or_default();
        Some(Self::new(
            identity,
            path,
            receipts,
            max_samples,
            budget_from_var(std::env::var_os(BUDGET_ENV), default_budget_ms),
        ))
    }

    /// Feed one frame whose hook cannot observe execution time.
    ///
    /// The first frame has no interval and carries `first_frame_reason`;
    /// every later frame carries `steady_frame_reason`. The owner decides
    /// stutter as it records, binds the capture receipts to a stuttered
    /// frame, and appends one JSONL line.
    pub fn observe(&mut self, now: Instant, first_frame_reason: &str, steady_frame_reason: &str) {
        let interval = self.last_frame.map(|previous| now.duration_since(previous));
        let observation = FrameTimingObservation::new(self.frame_id, interval, None);
        let stuttered = self.recorder.record_frame(observation);
        let reason = Some(
            match interval {
                None => first_frame_reason,
                Some(_) => steady_frame_reason,
            }
            .to_owned(),
        );
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
                .open(&self.path)
        {
            let _ = writeln!(file, "{line}");
        }
        self.last_frame = Some(now);
        self.frame_id = self.frame_id.saturating_add(1);
    }
}

/// Feed one native frame to the process's shared opt-in sink.
///
/// The first call constructs the sink from the product binding and the shared
/// frame-timing environment; a normal launch constructs no sink and every
/// later call is a lock-and-record. One process hosts one frontend, so one
/// binding serves the whole process.
pub fn observe_opt_in_frame(
    identity: FrameTimingIdentity,
    receipt_env: &str,
    max_samples: usize,
    default_budget_ms: f32,
    now: Instant,
    first_frame_reason: &str,
    steady_frame_reason: &str,
) {
    let lock = SHARED_SINK.get_or_init(|| {
        Mutex::new(FrameTimingSink::from_env(
            identity,
            receipt_env,
            max_samples,
            default_budget_ms,
        ))
    });
    let Ok(mut guard) = lock.lock() else {
        return;
    };
    if let Some(sink) = guard.as_mut() {
        sink.observe(now, first_frame_reason, steady_frame_reason);
    }
}

#[cfg(test)]
#[path = "../tests/headless/frame_timing_sink_tests.rs"]
mod frame_timing_sink_tests;
