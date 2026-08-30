//! Process-control requests and process-properties interaction seams.

use taskmanager_application::InteractionEvent;

use super::*;

impl ShellApp {
    /// Freeze the selected process identity and request the five insight
    /// domains for it. `None` when the selection is not a trustworthy
    /// process (e.g. a group header), mirroring `request_session_control`.
    #[must_use]
    pub fn request_process_insights(&mut self) -> Option<PlatformEffect> {
        let identity = self.selected_process_identity()?;
        Some(PlatformEffect::ProcessInsights(identity))
    }

    /// Freeze the selected process identity and submit one semantic signal.
    /// The renderer chooses the menu label; this shared seam keeps signal
    /// dispatch identity-safe and gives every frontend the same completion
    /// feedback path.
    #[must_use]
    pub fn request_process_signal(&mut self, signal: ProcessSignal) -> Option<PlatformEffect> {
        let identity = self.selected_process_identity()?;
        Some(PlatformEffect::ProcessSignal {
            target: identity,
            signal,
        })
    }

    /// Request the system-wide per-process network capture escalation (the
    /// Insights network facet's `RequiresEscalation` path). No target and no
    /// confirmation gate — the one-shot authorization mirrors GPUI's
    /// `request_process_network_escalation`.
    #[must_use]
    pub const fn request_process_network_escalation() -> PlatformEffect {
        PlatformEffect::ProcessNetworkEscalation
    }

    /// Apply the persisted `graph_data_points` preference to the shared
    /// rolling history store (G-02): every system-wide series, per-core
    /// window, and per-device window this shell owns is re-pointed to the
    /// clamped capacity, keeping the newest samples. Frontends call this once
    /// at startup (and on preference change) so the shared store and their
    /// local rings agree; the value is clamped to the product's 10..=600
    /// window range.
    pub fn set_history_capacity(&mut self, capacity: usize) {
        self.history.set_capacity(capacity);
        self.alert_suggestions.set_capacity(capacity);
    }

    /// Build a batch process-control intent (Kill / Suspend / Resume /
    /// SetPriority) over the selected target set and return it as a renderer-
    /// neutral [`PlatformEffect::ExecuteBatch`]. Every identity in
    /// [`Self::selected_rows`] is resolved exactly (pid AND start token) and
    /// frozen into the intent via [`ProcessBatchIntent::freeze`] so a later
    /// list refresh — or a pid reused by a different process — cannot
    /// retarget it (mirrors [`Self::request_startup_control`]). When the set
    /// is empty the keyboard anchor is used as a single-target fallback.
    #[must_use]
    pub fn request_process_batch(&mut self, action: ProcessBatchAction) -> Option<PlatformEffect> {
        let destructive = matches!(action, ProcessBatchAction::Kill | ProcessBatchAction::End);
        // Prefer the multi-select set; fall back to the keyboard anchor for
        // single-select callers (the TUI arrow path keeps the set at one row).
        if let Some(ProcessRowId::Application(root)) = self.selected_row {
            let Some(intent) = self.freeze_tree_for_identity(root, action) else {
                self.report_process_identity_unavailable();
                return None;
            };
            if destructive {
                self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
                return None;
            }
            return Some(PlatformEffect::ExecuteBatch(intent));
        }
        // A legacy caller may still have assigned the numeric cursor directly.
        // Resolve that cursor into the stable row authority before building an
        // intent; after a failed reconciliation the invalidation flag prevents
        // the cursor from silently targeting a neighboring process.
        if self.selected_rows.is_empty()
            && self.selected_row.is_none()
            && !self.process_selection_invalidated
            && let Some(identity) = self.row_identity_at(self.selected)
        {
            self.selected_row = Some(ProcessRowId::Process(identity));
            self.selected_rows.insert(identity);
        }
        let intent = {
            let processes = self.data.processes.as_deref()?;
            let identities = if self.selected_rows.is_empty() {
                match self.selected_row {
                    Some(ProcessRowId::Process(identity)) => vec![identity],
                    Some(ProcessRowId::Application(_) | ProcessRowId::Category(_)) | None => {
                        Vec::new()
                    }
                }
            } else {
                self.selected_rows.iter().copied().collect()
            };
            (!identities.is_empty())
                .then(|| ProcessBatchIntent::freeze(processes, identities, action))
        };
        let Some(intent) = intent else {
            self.report_process_identity_unavailable();
            return None;
        };
        if destructive {
            // Gate a destructive Kill behind a confirmation (mirrors pending_end
            // for End-task). Non-destructive actions (Suspend / Resume /
            // SetPriority) submit directly.
            self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
            return None;
        }
        Some(PlatformEffect::ExecuteBatch(intent))
    }

    /// Confirm the pending destructive batch (Kill): emit the ExecuteBatch
    /// effect and clear the gate. Returns `None` if no batch is pending.
    pub fn confirm_process_batch(&mut self) -> Option<PlatformEffect> {
        self.confirm_confirmation(ConfirmationKind::ProcessBatch)
    }

    /// Freeze a process tree only after its root's full live row identity has
    /// been re-resolved in the accepted snapshot. The core tree contract still
    /// walks parent PIDs, but the PID reaches it only as a lookup hint derived
    /// from this validated row identity.
    fn freeze_tree_for_identity(
        &self,
        root: ProcessLiveKey,
        action: ProcessBatchAction,
    ) -> Option<ProcessBatchIntent> {
        let processes = self.data.processes.as_deref()?;
        if !processes
            .iter()
            .any(|process| ProcessLiveKey::from_process(process) == Some(root))
        {
            return None;
        }
        let intent = ProcessBatchIntent::freeze_tree(processes, root, action);
        intent
            .targets
            .iter()
            .any(|target| target.live_key() == Some(root))
            .then_some(intent)
    }

    fn report_process_identity_unavailable(&mut self) {
        self.report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::SHORT,
            "Process row identity is unavailable",
        );
    }

    /// Snapshot a process tree into the shared batch confirmation slot. The
    /// core intent freezes descendants leaf-first; the renderer only confirms
    /// or dismisses the pending intent and never recomputes its targets.
    pub fn request_process_tree_end(&mut self, root: ProcessLiveKey) {
        let Some(intent) = self.freeze_tree_for_identity(root, ProcessBatchAction::End) else {
            self.report_process_identity_unavailable();
            return;
        };
        self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
    }

    /// Open Process Properties for an exact renderer-selected identity.
    /// Frontends with grouped/tree projections may not be able to express the
    /// visual row through the shell's flat cursor, so they freeze the row and
    /// enter the same application-owned interaction machine through this
    /// seam. Legacy identities fail closed.
    pub fn open_process_properties_for(&mut self, target: FrozenProcessIdentity) -> bool {
        if target.authoritative_start_token().is_none() {
            return false;
        }
        let reduction = self
            .application
            .interaction
            .reduce(InteractionEvent::OpenProcessProperties(target));
        self.apply_surface_transition(reduction.transition);
        debug_assert!(reduction.effect.is_none());
        true
    }

    #[must_use]
    pub fn apply_action(&mut self, action: AppAction) -> Option<PlatformEffect> {
        self.sync_application_selection();
        let reduction = reduce(&mut self.application, action);
        self.apply_reduction(reduction)
    }

    #[must_use]
    pub fn confirm_end_task(&mut self) -> Option<PlatformEffect> {
        self.apply_action(AppAction::ConfirmEndTask)
    }

    pub fn dismiss_overlay(&mut self) {
        let _ = self.apply_action(AppAction::DismissOverlay);
        self.dismiss_informational_overlay();
    }
}
