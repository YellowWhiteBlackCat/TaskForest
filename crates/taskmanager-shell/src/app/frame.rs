//! Telemetry-frame lifecycle and the typed side outputs of one platform fold.

use taskmanager_application::{
    CorrelatedDesktopAppearanceEvent, CorrelatedPowerSupplyEvent, CorrelatedProcessEvent,
    CorrelatedSensorEvent, CorrelatedSetupScriptEvent, CorrelatedShellEvent,
    CorrelatedSystemTelemetryOutcome, ServiceUpdate, SessionControlOutcome, StartupControlOutcome,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessBatchResult};
use taskmanager_core::core::storage::StorageDeviceTarget;
use taskmanager_platform_contract::{OperationFailure, RequestId};

use super::ProcessControlFeedback;

/// Persistent lifecycle of the visible telemetry frame.
///
/// `Collecting` means no complete immutable frame has been published yet;
/// `Ready` means at least one frame is available for rendering. Partial
/// domain arrivals update the pending projection but do not create a third
/// visible state: the last committed frame remains the render truth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TelemetryFrameState {
    #[default]
    Collecting,
    Ready,
}

impl TelemetryFrameState {
    #[must_use]
    pub const fn is_collecting(self) -> bool {
        matches!(self, Self::Collecting)
    }

    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Per-batch transition emitted by the shared fold.
///
/// This is intentionally an enum instead of a pair of booleans such as
/// separate acceptance and commit flags: those fields would describe an
/// implicit three-state machine (`Rejected`, `AcceptedPartial`, `Committed`)
/// and would permit invalid combinations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameCommit {
    #[default]
    Unchanged,
    Committed,
}

impl FrameCommit {
    #[must_use]
    pub const fn is_committed(self) -> bool {
        matches!(self, Self::Committed)
    }

    /// Combine per-projection results into the one tick-level transition.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        if self.is_committed() || other.is_committed() {
            Self::Committed
        } else {
            Self::Unchanged
        }
    }
}

/// Typed per-domain change report produced by
/// [`super::SystemProjectionStore::apply_platform_batch`]. Renderers use these flags
/// for entity-granular invalidation (GPUI), dirty-cycle decisions (TUI), and
/// redraw scheduling (Iced); the flags are the only contract between the
/// shared data fold and the frontend view layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchFoldChanges {
    pub telemetry: bool,
    pub hardware: bool,
    pub processes: bool,
    pub services: bool,
    pub startup: bool,
    pub sessions: bool,
    /// Sensor, power, or other dynamic-device data changed.
    pub dynamic_devices: bool,
    pub containers: bool,
    pub directory_usage: bool,
    pub gpu_engine_rows: bool,
    pub npu_inventory: bool,
    pub desktop_appearance: bool,
    pub storage_health: bool,
    pub smart: bool,
    pub process_insights: bool,
    pub process_affinity: bool,
    pub process_batch: bool,
    pub smart_self_test: bool,
    pub startup_evidence: bool,
    /// `system_revision` advanced (telemetry, hardware/value-source status,
    /// containers, sensors or power).
    pub system: bool,
    /// The shared fold committed a complete render snapshot in this batch.
    /// This is deliberately separate from [`Self::telemetry`]: telemetry
    /// events may arrive one domain at a time while the immutable render frame
    /// must remain on the previous committed revision.
    pub frame_commit: FrameCommit,
    /// A new complete snapshot was recorded (deduped by timestamp); frontends
    /// feed their rolling history/read models once per tick. This is a history
    /// watermark, not the frame-commit signal above.
    pub snapshot_recorded: bool,
}

impl BatchFoldChanges {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !(self.telemetry
            || self.hardware
            || self.processes
            || self.services
            || self.startup
            || self.sessions
            || self.dynamic_devices
            || self.containers
            || self.directory_usage
            || self.gpu_engine_rows
            || self.npu_inventory
            || self.desktop_appearance
            || self.storage_health
            || self.smart
            || self.process_insights
            || self.process_affinity
            || self.process_batch
            || self.smart_self_test
            || self.startup_evidence
            || self.system
            || self.frame_commit.is_committed()
            || self.snapshot_recorded)
    }
}

/// Typed side outputs of one shared fold: what changed, what the frontend
/// must route (notifications, refresh requests, service-log updates), and
/// which correlated outcomes were accepted (for per-frontend feedback).
#[derive(Clone, Debug, Default)]
pub struct BatchFoldOutput {
    pub changes: BatchFoldChanges,
    /// Long-lived background activity. This never competes with a typed
    /// point-of-action notice in [`super::FeedbackState`].
    pub activity: Option<String>,
    pub system_telemetry_outcomes: Vec<CorrelatedSystemTelemetryOutcome>,
    /// Accepted process-list snapshots retained for the optional durable
    /// application-history sink. Control completions are intentionally not
    /// included.
    pub process_events: Vec<CorrelatedProcessEvent>,
    pub sensor_events: Vec<CorrelatedSensorEvent>,
    pub power_supply_events: Vec<CorrelatedPowerSupplyEvent>,
    pub desktop_appearance_events: Vec<CorrelatedDesktopAppearanceEvent>,
    pub failures: Vec<OperationFailure>,
    pub service_log_updates: Vec<ServiceUpdate>,
    pub service_updates: Vec<ServiceUpdate>,
    pub process_feedback: Option<ProcessControlFeedback>,
    pub process_affinity_results: Vec<ProcessAffinityResult>,
    pub batch_results: Vec<(RequestId, ProcessBatchResult)>,
    pub smart_self_test_results: Vec<SmartSelfTestResult>,
    pub startup_control_outcomes: Vec<StartupControlOutcome>,
    pub session_control_outcomes: Vec<SessionControlOutcome>,
    pub service_control_outcomes: Vec<taskmanager_application::ServiceControlOutcome>,
    pub network_capture_escalations: Vec<RequestId>,
    pub shell_events: Vec<CorrelatedShellEvent>,
    pub setup_script_events: Vec<CorrelatedSetupScriptEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessAffinityResult {
    pub request_id: RequestId,
    pub target: FrozenProcessIdentity,
    pub cpus: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartSelfTestResult {
    pub request_id: RequestId,
    pub target: StorageDeviceTarget,
}
