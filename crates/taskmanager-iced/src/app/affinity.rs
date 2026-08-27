//! Iced-local CPU-affinity editor state and effect routing.
//!
//! The shell owns the platform port and correlation. This module only owns
//! the editor's frozen target, logical-CPU selection, and the fail-closed
//! transition between read and write. In particular, Apply never asks the
//! shell to resolve the current row again: a recycled PID cannot be silently
//! retargeted while the modal is open.

use super::{IcedApp, Message, PlatformEffect};
use taskmanager_application::i18n::t;
use taskmanager_application::{ProcessAffinityReady, ProcessAffinityState};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

impl IcedApp {
    /// Route the three affinity messages through one local state machine so
    /// the main update loop stays a thin dispatch table.
    pub(super) fn handle_process_affinity_message(
        &mut self,
        message: Message,
    ) -> Option<PlatformEffect> {
        match message {
            Message::OpenProcessAffinity => self.open_process_affinity_effect(),
            Message::ToggleProcessAffinityCpu(cpu) => {
                self.toggle_process_affinity_cpu(cpu);
                None
            }
            Message::SelectAllProcessAffinity => {
                self.select_all_process_affinity();
                None
            }
            Message::ClearAllProcessAffinity => {
                self.clear_all_process_affinity();
                None
            }
            Message::InvertProcessAffinity => {
                self.invert_process_affinity();
                None
            }
            Message::SelectProcessAffinityPCores => {
                self.select_process_affinity_p_cores();
                None
            }
            Message::SelectProcessAffinityECores => {
                self.select_process_affinity_e_cores();
                None
            }
            Message::ApplyProcessAffinity => self.apply_process_affinity_effect(),
            _ => None,
        }
    }

    /// Open the affinity editor and queue a fresh read for the selected
    /// process. A matching cached snapshot seeds the controls immediately,
    /// while the new correlated read remains authoritative when it arrives.
    pub(super) fn open_process_affinity_effect(&mut self) -> Option<PlatformEffect> {
        let Some(target) = self.shell.selected_process_identity() else {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("empty.no_process_selected"),
            );
            return None;
        };

        self.open_local_surface(super::LocalSurface::ProcessAffinity {
            target: target.clone(),
        });
        self.process_presentation.affinity_cpus = None;

        if let Some(snapshot) = affinity_last_good(self.shell.process_affinity_state(), &target) {
            self.process_presentation.affinity_cpus = Some(snapshot.cpus.iter().copied().collect());
        }

        self.shell.request_process_affinity()
    }

    /// Copy a correlated snapshot into the editor only when its frozen target
    /// is the one captured at open time. A different process with the same PID
    /// is never allowed to rewrite the visible mask.
    pub(super) fn sync_process_affinity_snapshot(&mut self) {
        let Some(target) = self.affinity_target().cloned() else {
            return;
        };
        if matches!(
            self.shell.process_affinity_state(),
            ProcessAffinityState::Failed { target: failed, .. } if failed == &target
        ) {
            self.process_presentation.affinity_cpus = None;
            return;
        }
        let Some(snapshot) = affinity_last_good(self.shell.process_affinity_state(), &target)
        else {
            return;
        };

        let logical_cpu_count = self.logical_cpu_count();
        self.process_presentation.affinity_cpus = Some(
            snapshot
                .cpus
                .iter()
                .copied()
                .filter(|cpu| {
                    usize::try_from(*cpu)
                        .ok()
                        .is_some_and(|index| index < logical_cpu_count)
                })
                .collect(),
        );
    }

    /// Toggle a logical CPU only inside the measured topology and only after
    /// a correlated mask has been observed. An empty/unread mask is not an
    /// editable default.
    pub(super) fn toggle_process_affinity_cpu(&mut self, cpu: u32) {
        if !self.affinity_open() {
            return;
        }
        let Ok(index) = usize::try_from(cpu) else {
            return;
        };
        if index >= self.logical_cpu_count() {
            return;
        }
        let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() else {
            return;
        };
        if !cpus.remove(&cpu) {
            cpus.insert(cpu);
        }
    }

    /// Select all logical CPUs.
    pub(super) fn select_all_process_affinity(&mut self) {
        if !self.affinity_open() {
            return;
        }
        let count = self.logical_cpu_count();
        let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() else {
            return;
        };
        for cpu in 0..count {
            if let Ok(cpu_id) = u32::try_from(cpu) {
                cpus.insert(cpu_id);
            }
        }
    }

    /// Deselect all logical CPUs.
    pub(super) fn clear_all_process_affinity(&mut self) {
        if !self.affinity_open() {
            return;
        }
        if let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() {
            cpus.clear();
        }
    }

    /// Invert current CPU selection.
    pub(super) fn invert_process_affinity(&mut self) {
        if !self.affinity_open() {
            return;
        }
        let count = self.logical_cpu_count();
        let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() else {
            return;
        };
        for cpu in 0..count {
            if let Ok(cpu_id) = u32::try_from(cpu)
                && !cpus.remove(&cpu_id)
            {
                cpus.insert(cpu_id);
            }
        }
    }

    /// Select only Performance cores (P-Cores) if heterogeneous topology is present.
    pub(super) fn select_process_affinity_p_cores(&mut self) {
        if !self.affinity_open() {
            return;
        }
        let count = self.logical_cpu_count();
        let cpu_types = self
            .shell
            .projection()
            .hardware
            .as_ref()
            .map(|hw| hw.cpu_types.clone());
        let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() else {
            return;
        };
        cpus.clear();
        if let Some(types) = cpu_types {
            for (idx, cpu_type) in types.iter().enumerate() {
                if *cpu_type == taskmanager_application::CpuType::Performance
                    && let Ok(cpu_id) = u32::try_from(idx)
                {
                    cpus.insert(cpu_id);
                }
            }
        }
        if cpus.is_empty() {
            // Fallback: select first half if no cpu_types
            for cpu in 0..(count / 2).max(1) {
                if let Ok(cpu_id) = u32::try_from(cpu) {
                    cpus.insert(cpu_id);
                }
            }
        }
    }

    /// Select only Efficient cores (E-Cores) if heterogeneous topology is present.
    pub(super) fn select_process_affinity_e_cores(&mut self) {
        if !self.affinity_open() {
            return;
        }
        let count = self.logical_cpu_count();
        let cpu_types = self
            .shell
            .projection()
            .hardware
            .as_ref()
            .map(|hw| hw.cpu_types.clone());
        let Some(cpus) = self.process_presentation.affinity_cpus.as_mut() else {
            return;
        };
        cpus.clear();
        if let Some(types) = cpu_types {
            for (idx, cpu_type) in types.iter().enumerate() {
                if matches!(
                    *cpu_type,
                    taskmanager_application::CpuType::Efficient
                        | taskmanager_application::CpuType::LowPower
                ) && let Ok(cpu_id) = u32::try_from(idx)
                {
                    cpus.insert(cpu_id);
                }
            }
        }
        if cpus.is_empty() {
            // Fallback: select second half if no cpu_types
            for cpu in (count / 2)..count {
                if let Ok(cpu_id) = u32::try_from(cpu) {
                    cpus.insert(cpu_id);
                }
            }
        }
    }

    /// Submit the edited mask against the exact target captured on open.
    /// Empty masks and failed/unobserved reads are rejected locally so the UI
    /// cannot ask a provider to apply an unrepresentable or unknown state.
    pub(super) fn apply_process_affinity_effect(&mut self) -> Option<PlatformEffect> {
        if !self.affinity_open()
            || matches!(
                self.shell.process_affinity_state(),
                ProcessAffinityState::Failed { .. }
            )
        {
            return None;
        }
        let Some(selected_cpus) = self.process_presentation.affinity_cpus.as_ref() else {
            self.shell.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("common.collecting_telemetry"),
            );
            return None;
        };

        let target = self.affinity_target()?.clone();
        let logical_cpu_count = self.logical_cpu_count();
        let mut cpus: Vec<u32> = selected_cpus
            .iter()
            .copied()
            .filter(|cpu| {
                usize::try_from(*cpu)
                    .ok()
                    .is_some_and(|index| index < logical_cpu_count)
            })
            .collect();
        cpus.sort_unstable();
        if cpus.is_empty() {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("proc.affinity_select_one"),
            );
            return None;
        }

        let effect = self
            .shell
            .request_process_affinity_control_for(target, cpus);
        if effect.is_some() {
            // The shell records completion/failure feedback after submission;
            // close the editor just as GPUI does, so the footer remains the
            // point of action without leaving a stale editable mask visible.
            self.close_local_modals();
        }
        effect
    }

    /// The logical-CPU count used by the editor. Prefer the typed hardware
    /// inventory; fall back to the process-visible topology without panicking,
    /// and cap the grid at the same bounded 128 CPUs as the GPUI surface.
    #[must_use]
    pub(crate) fn logical_cpu_count(&self) -> usize {
        self.shell
            .projection()
            .hardware
            .as_ref()
            .and_then(|hardware| hardware.cpu_cores)
            .filter(|count| *count > 0)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(8, |count| count.get()))
            .min(128)
    }
}

fn affinity_last_good<'a>(
    state: &'a ProcessAffinityState,
    target: &taskmanager_application::FrozenProcessIdentity,
) -> Option<&'a ProcessAffinityReady> {
    match state {
        ProcessAffinityState::Ready(ready) if &ready.target == target => Some(ready),
        ProcessAffinityState::Loading {
            target: loading,
            last_good: Some(ready),
            ..
        }
        | ProcessAffinityState::Failed {
            target: loading,
            last_good: Some(ready),
            ..
        } if loading == target => Some(ready),
        ProcessAffinityState::Closed
        | ProcessAffinityState::Loading { .. }
        | ProcessAffinityState::Ready(_)
        | ProcessAffinityState::Failed { .. } => None,
    }
}
