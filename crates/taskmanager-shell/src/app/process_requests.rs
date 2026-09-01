//! Process-control requests and process-properties interaction seams.

use taskmanager_application::InteractionEvent;

use super::*;

impl ShellApp {
    /// Project the selected Applications row into the shared process-control
    /// availability state. The UI may render this state, but request
    /// construction still goes through the methods below and freezes exact
    /// identities at the point of submission.
    #[must_use]
    pub fn process_control_availability(&self) -> super::ProcessControlAvailability {
        let selected: Vec<_> = self.selected_rows.iter().copied().collect();
        super::process_control::process_control_availability(
            self.data.processes_slice(),
            self.selected_row,
            &selected,
            self.data
                .capability_status(&taskmanager_platform_contract::CapabilityId::PROCESS_CONTROL),
        )
    }

    /// Project the marked set independently of the keyboard row. The batch
    /// menu uses this view because its scope is the explicit marked set even
    /// when the visual cursor currently rests on a category or application
    /// header.
    #[must_use]
    pub fn marked_process_control_availability(&self) -> super::ProcessControlAvailability {
        let selected: Vec<_> = self.selected_rows.iter().copied().collect();
        super::process_control::process_control_availability(
            self.data.processes_slice(),
            None,
            &selected,
            self.data
                .capability_status(&taskmanager_platform_contract::CapabilityId::PROCESS_CONTROL),
        )
    }

    /// Resolve the current selection into exact live identities. Application
    /// rows expand through the shared leaf-first tree walk; process selection
    /// uses the marked set and falls back to the validated anchor. Renderers
    /// consume this projection and never rebuild a target set locally.
    #[must_use]
    pub fn process_control_targets(&self) -> Vec<ProcessLiveKey> {
        let selected: Vec<_> = if self.selected_rows.is_empty() {
            self.selected_process_identity()
                .and_then(|target| target.live_key())
                .into_iter()
                .collect()
        } else {
            self.selected_rows.iter().copied().collect()
        };
        super::process_control::process_control_targets(
            self.data.processes_slice(),
            self.selected_row,
            &selected,
        )
    }

    /// Build the one atomic process-control intent used by every renderer's
    /// confirmation or direct-submit surface. Target expansion and exact
    /// identity freezing stay at this shell/application boundary.
    #[must_use]
    pub fn process_control_intent(&self, action: ProcessBatchAction) -> Option<ProcessBatchIntent> {
        let processes = self.data.processes.as_deref()?;
        let selected: Vec<_> = if self.selected_rows.is_empty() {
            self.selected_process_identity()
                .and_then(|target| target.live_key())
                .into_iter()
                .collect()
        } else {
            self.selected_rows.iter().copied().collect()
        };
        super::process_control::process_control_intent(
            processes,
            self.selected_row,
            &selected,
            action,
        )
    }

    fn process_control_capability_allowed(&self) -> bool {
        super::process_control::process_control_capability_allowed(
            self.data
                .capability_status(&taskmanager_platform_contract::CapabilityId::PROCESS_CONTROL),
        )
    }

    fn report_process_control_unavailable(&mut self) {
        self.report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::SHORT,
            "Process control capability is unavailable",
        );
    }

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
        if !self.process_control_capability_allowed() {
            self.report_process_control_unavailable();
            return None;
        }
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
    ///
    /// Gating goes through [`Self::process_batch_requires_confirmation`]: the
    /// returned `None` with a pending batch means "confirmed, not applied".
    #[must_use]
    pub fn request_process_batch(&mut self, action: ProcessBatchAction) -> Option<PlatformEffect> {
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
        if !self.process_control_capability_allowed() {
            self.report_process_control_unavailable();
            return None;
        }
        let Some(intent) = self.process_control_intent(action) else {
            self.report_process_identity_unavailable();
            return None;
        };
        // One authoritative gate (see [`Self::process_batch_requires_confirmation`]):
        // a destructive verb is always confirmed, and so is anything that
        // reaches past the row the user is looking at. The gate reads the
        // selection's reach (how many rows the user marked), not the frozen
        // survivor count, so identities that went stale between marking and
        // acting cannot quietly shrink a batch into an unconfirmed submit.
        let application_tree = matches!(self.selected_row, Some(ProcessRowId::Application(_)));
        let target_count = if application_tree || self.selected_rows.is_empty() {
            intent.targets.len()
        } else {
            self.selected_rows.len()
        };
        if Self::process_batch_requires_confirmation(action, target_count, application_tree) {
            self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
            return None;
        }
        Some(PlatformEffect::ExecuteBatch(intent))
    }

    /// Whether one batch verb is destructive enough to demand a confirmation
    /// whatever it targets.
    #[must_use]
    pub const fn process_batch_is_destructive(action: ProcessBatchAction) -> bool {
        matches!(action, ProcessBatchAction::Kill | ProcessBatchAction::End)
    }

    /// The SINGLE authority for whether one frozen batch intent must pass the
    /// shared confirmation gate before it may reach the platform. Destructive
    /// verbs (Kill / End) are gated at any target count. Anything that reaches
    /// beyond the single row the user is looking at — a multi-select set, or an
    /// application root whose tree is expanded at the boundary — is gated even
    /// for a reversible verb (Suspend / Resume / SetPriority), because a
    /// mis-aimed batch Suspend costs as much trust as a mis-aimed Kill. One
    /// explicit non-destructive target applies immediately.
    ///
    /// Renderers query this (through
    /// [`Self::selection_requires_batch_confirmation`]) instead of keeping a
    /// per-frontend rule, so a multi-select Suspend cannot be gated on one
    /// surface and silent on another.
    #[must_use]
    pub const fn process_batch_requires_confirmation(
        action: ProcessBatchAction,
        target_count: usize,
        application_tree: bool,
    ) -> bool {
        Self::process_batch_is_destructive(action) || application_tree || target_count > 1
    }

    /// The same authority evaluated against the selection as it stands now, so
    /// a renderer can label its action (arm a gate or submit) before freezing
    /// the intent. An application row is a tree freeze; an empty set falls back
    /// to the single keyboard anchor exactly like
    /// [`Self::request_process_batch`] does.
    #[must_use]
    pub fn selection_requires_batch_confirmation(&self, action: ProcessBatchAction) -> bool {
        let application_tree = matches!(self.selected_row, Some(ProcessRowId::Application(_)));
        let target_count = if application_tree || self.selected_rows.is_empty() {
            1
        } else {
            self.selected_rows.len()
        };
        Self::process_batch_requires_confirmation(action, target_count, application_tree)
    }

    /// Confirm the pending batch and emit the frozen `ExecuteBatch` effect,
    /// clearing the gate. Returns `None` if no batch is pending; the armed
    /// gate is whatever [`Self::process_batch_requires_confirmation`] asked
    /// for.
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
        if !self.process_control_capability_allowed() {
            self.report_process_control_unavailable();
            return;
        }
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
