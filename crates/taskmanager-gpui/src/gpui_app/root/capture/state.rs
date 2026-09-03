//! Explicit capture lifecycle state owned by the capture coordinator.

use super::ProcessDetailsSection;
use super::scenarios::CaptureScenario;
use crate::gpui_app::process_insights::ProcessInsightsState;
use crate::gpui_app::system_health_view::SmartSelfTestConfirmationRequest;
use taskmanager_core::core::process::{ProcessBatchIntent, ProcessLiveKey};
use taskmanager_core::core::{AlertEvent, SmartSelfTestObservation};

#[derive(Debug, PartialEq)]
pub enum CaptureProcessAction {
    ApplicationSelection(ProcessLiveKey),
    Batch(ProcessBatchIntent),
    Properties(ProcessLiveKey, ProcessDetailsSection),
    Insights {
        identity: ProcessLiveKey,
        state: ProcessInsightsState,
    },
}

#[derive(Debug, Default)]
pub enum SystemHealthCaptureOutcome {
    #[default]
    NotReady,
    Ready,
    ReadyWithConfirmation(SmartSelfTestConfirmationRequest),
}

/// The NPU evidence marker is emitted only after its typed fixture has entered
/// the canonical projection and the Graphics section has actually been laid
/// out and scrolled into the per-window viewport.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemNpuCaptureState {
    #[default]
    AwaitingFixture,
    AwaitingLayout,
    ScrollScheduled,
    Ready,
}

impl SystemNpuCaptureState {
    pub(crate) const fn needs_fixture(self) -> bool {
        matches!(self, Self::AwaitingFixture)
    }
}

impl SystemHealthCaptureOutcome {
    pub const fn ready(&self) -> bool {
        matches!(self, Self::Ready | Self::ReadyWithConfirmation(_))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowCaptureSchedule {
    #[default]
    Inactive,
    AwaitingFrame,
    FrameScheduled,
    Settling,
    ReadyToSubmit,
    Submitted,
    Failed,
}

impl WindowCaptureSchedule {
    pub(crate) const fn active(self) -> bool {
        !matches!(self, Self::Inactive)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowCaptureChain {
    #[default]
    Disabled,
    Active,
}

impl WindowCaptureChain {
    pub(crate) const fn active(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureMode {
    #[default]
    Disabled,
    Enabled,
}

impl CaptureMode {
    pub(crate) const fn enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureDataReadiness {
    #[default]
    Cold,
    TelemetryReady,
    UiDataReady,
}

impl CaptureDataReadiness {
    pub(crate) const fn telemetry_ready(self) -> bool {
        !matches!(self, Self::Cold)
    }

    pub(crate) const fn ui_data_ready(self) -> bool {
        matches!(self, Self::UiDataReady)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureScenarioProgress {
    #[default]
    Pending,
    Ready,
}

impl CaptureScenarioProgress {
    pub(crate) const fn ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryReplayOpenState {
    #[default]
    Closed,
    Opened,
}

#[derive(Debug, Default)]
pub(crate) struct CaptureEvidence {
    pub(super) mode: CaptureMode,
    pub(super) scenario: Option<CaptureScenario>,
    pub(super) data_readiness: CaptureDataReadiness,
    pub(super) scenario_progress: CaptureScenarioProgress,
    pub(super) snapshot_count: u8,
    pub(super) scenario_process_identity: Option<ProcessLiveKey>,
    pub(super) history_replay_open_state: HistoryReplayOpenState,
    pub(super) system_npu_state: SystemNpuCaptureState,
    /// Capture-only comparison evidence. Persistent history runtime is
    /// reader-only; deterministic screenshots must not reintroduce its retired
    /// boot writer/controller state.
    pub(super) startup_boot_baseline: Option<taskmanager_core::core::startup::BootTimeline>,
    /// Strict-capture-only typed observation. Production reports are borrowed
    /// directly from the shell projection and never copied into this slot.
    pub(super) system_health_observation: Option<SmartSelfTestObservation>,
    /// Capture-only handoff for deterministic alert events. The fixture is
    /// installed into the shared shell authority before the panel is shown.
    pub(super) event_history_fixture: Option<Vec<AlertEvent>>,
    /// Capture-only state machine that waits for two rendered frames before
    /// submitting the current-window provider request, then becomes terminal.
    pub(super) window_capture_schedule: WindowCaptureSchedule,
    /// Explicit opt-in for the private current-window provider receipt. This
    /// is kept outside the visual scenario enum because nested Niri cannot
    /// exercise Spectacle's outer-KWin active-window selector faithfully.
    pub(super) window_capture_chain: WindowCaptureChain,
}
