//! Renderer-local pacing for the shared GPU-engine request session.
//!
//! Request identity, replacement, terminal acceptance and accepted payload are
//! owned by `DirectTrackState`. This module only binds the visible GPU to a
//! refresh cadence and invalidates scheduled work when that device is no
//! longer visible.

use taskmanager_application::{
    GpuEngineRowsRequest, GpuEngineRowsState, request_submission_failure,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_platform_contract::SubmissionErrorKind;

use crate::gpui_app::root::RootView;

#[derive(Clone, Debug, PartialEq, Eq)]
struct GpuEngineBinding {
    index: usize,
    device_id: DeviceId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum GpuEnginePacingPhase {
    #[default]
    Detached,
    Bound(GpuEngineBinding),
    Polling {
        binding: GpuEngineBinding,
        generation: u64,
    },
}

/// Per-window scheduling state. It deliberately contains no request id,
/// loading/error state or engine payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct GpuEnginePacingState {
    next_generation: u64,
    phase: GpuEnginePacingPhase,
}

impl GpuEnginePacingState {
    fn binding(&self) -> Option<&GpuEngineBinding> {
        match &self.phase {
            GpuEnginePacingPhase::Detached => None,
            GpuEnginePacingPhase::Bound(binding)
            | GpuEnginePacingPhase::Polling { binding, .. } => Some(binding),
        }
    }

    fn bind(&mut self, binding: GpuEngineBinding) -> bool {
        if self.binding() == Some(&binding) {
            return false;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        self.phase = GpuEnginePacingPhase::Bound(binding);
        true
    }

    fn start(&mut self, binding: GpuEngineBinding) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        self.phase = GpuEnginePacingPhase::Polling {
            binding,
            generation,
        };
        generation
    }

    fn stop(&mut self, detach: bool) -> bool {
        if (detach && matches!(&self.phase, GpuEnginePacingPhase::Detached))
            || (!detach
                && matches!(
                    &self.phase,
                    GpuEnginePacingPhase::Detached | GpuEnginePacingPhase::Bound(_)
                ))
        {
            return false;
        }
        let binding = self.binding().cloned();
        self.next_generation = self.next_generation.wrapping_add(1);
        self.phase = if detach {
            GpuEnginePacingPhase::Detached
        } else {
            binding.map_or(GpuEnginePacingPhase::Detached, GpuEnginePacingPhase::Bound)
        };
        true
    }

    fn finish(&mut self, generation: u64) {
        let GpuEnginePacingPhase::Polling {
            binding,
            generation: current,
        } = &self.phase
        else {
            return;
        };
        if *current == generation {
            self.phase = GpuEnginePacingPhase::Bound(binding.clone());
        }
    }

    fn is_polling(&self, generation: u64) -> bool {
        matches!(
            &self.phase,
            GpuEnginePacingPhase::Polling {
                generation: current,
                ..
            } if *current == generation
        )
    }

    fn device_id(&self) -> Option<&DeviceId> {
        self.binding().map(|binding| &binding.device_id)
    }
}

impl RootView {
    pub(crate) fn gpu_engine_rows_device_id(&self, index: usize) -> DeviceId {
        self.system_snapshot()
            .gpu
            .get(index)
            .map(|gpu| gpu.device_id.trim().to_owned())
            .filter(|id| !id.is_empty())
            .map_or_else(
                || DeviceId::new(format!("gpu-index:{index}")),
                DeviceId::new,
            )
    }

    pub(crate) fn reconcile_gpu_engine_binding(&mut self, index: usize, device_id: DeviceId) {
        if self
            .gpu_engine_pacing
            .bind(GpuEngineBinding { index, device_id })
        {
            self.shell.close_gpu_engine_rows_request();
        }
    }

    pub(crate) fn start_gpu_engine_polling(&mut self, index: usize) -> u64 {
        let binding = GpuEngineBinding {
            index,
            device_id: self.gpu_engine_rows_device_id(index),
        };
        self.shell.close_gpu_engine_rows_request();
        self.gpu_engine_pacing.start(binding)
    }

    pub(crate) fn stop_gpu_engine_polling(&mut self, detach: bool) {
        if self.gpu_engine_pacing.stop(detach)
            || !matches!(
                self.shell.gpu_engine_rows_state(),
                GpuEngineRowsState::Closed
            )
        {
            self.shell.close_gpu_engine_rows_request();
        }
    }

    pub(crate) fn gpu_engine_polling_is_current(&self, generation: u64) -> bool {
        self.gpu_engine_pacing.is_polling(generation)
    }

    pub(crate) fn finish_gpu_engine_polling(&mut self, generation: u64) {
        self.gpu_engine_pacing.finish(generation);
    }

    /// Submit one request into the application-owned session. Beginning the
    /// attempt before touching the platform makes replacement and synchronous
    /// rejection obey the same identity rules as asynchronous terminals.
    pub(crate) fn submit_gpu_engine_rows_refresh(&mut self) -> bool {
        let Some(device_id) = self.gpu_engine_pacing.device_id().cloned() else {
            return false;
        };
        let request = GpuEngineRowsRequest {
            device_id: device_id.clone(),
        };
        let attempt = self.shell.begin_gpu_engine_rows_request(device_id);
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_gpu_engine_rows(request, super::platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => self
                .shell
                .accept_gpu_engine_rows_request(attempt, request_id),
            Err(kind) => {
                self.shell
                    .reject_gpu_engine_rows_request(attempt, request_submission_failure(kind));
                false
            }
        }
    }

    pub(crate) fn gpu_engine_session_allows_polling(&self) -> bool {
        matches!(
            self.shell.gpu_engine_rows_state(),
            GpuEngineRowsState::Loading { .. } | GpuEngineRowsState::Ready(_)
        )
    }
}
