//! The TUI-local modal/menu surface on [`TuiApp`]: the service-action,
//! process-action, session-action, and Process-Properties interactions.
//! Extracted verbatim from `lib.rs` to keep the crate root under the source
//! line budget. Behavior is unchanged — every method stays reachable on
//! `TuiApp` (impl blocks may live in any module of the defining crate).

use crate::ui;
use crate::{
    ProcessDetailsSection, ProcessMenuTarget, ProcessPropertiesTarget, ServiceMenuTarget,
    SessionMenuTarget, TuiApp, TuiSurface, TuiSurfaceKind,
};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    AppAction, AppPage, InteractionEvent, PendingConfirmation, PlatformEffect,
    ProcessAffinityReady, ProcessAffinityRequest, ProcessAffinityState, SurfaceTransition,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::SmartAvailability;
use taskmanager_core::core::process::{ProcessBatchAction, ProcessLiveKey};
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::smart::self_test::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::target::StorageDeviceKey;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ProcessRowId};

/// The frozen batch-control menu target: the menu cursor plus the count of
/// marked processes the actions will apply to. The shell's `selected_pids`
/// set is the live source of truth (re-frozen at confirm); the count is
/// captured at open time so the menu can label its rows honestly even if a
/// refresh prunes a died pid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchMenuTarget {
    pub selection: usize,
    pub marked_count: usize,
}

impl TuiApp {
    /// Open the service-action menu for the selected Services-page row.
    /// Returns false (and an honest status line) when no row or no provider
    /// target is available. The cursor indexes the same sorted projection
    /// the renderer paints, so the frozen target is always the highlighted
    /// row (the shell's `sorted_service_at` is the single translation).
    #[must_use]
    pub fn open_service_menu(&mut self) -> bool {
        if self.page() != AppPage::Services {
            return false;
        }
        let Some(service) = self.sorted_service_at(self.selected) else {
            return false;
        };
        if service.id.as_str().is_empty() {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "This service row has no provider target",
            );
            return false;
        }
        self.open_local_surface(TuiSurface::ServiceMenu(ServiceMenuTarget {
            service: service.clone(),
            selection: 0,
        }));
        true
    }

    /// Move the service-action menu cursor (clamped).
    pub fn service_menu_move(&mut self, delta: isize) {
        if let Some(menu) = self.service_menu_mut() {
            menu.selection = menu
                .selection
                .saturating_add_signed(delta)
                .min(ui::service_menu::MENU_ACTIONS.len() - 1);
        }
    }

    /// Consume the menu selection: record the gated service target and open
    /// the shared confirmation overlay. The platform request is only emitted
    /// by `ConfirmServiceControl`.
    pub fn service_menu_select(&mut self) {
        let Some(TuiSurface::ServiceMenu(menu)) =
            self.take_local_surface(TuiSurfaceKind::ServiceMenu)
        else {
            return;
        };
        let action = ui::service_menu::MENU_ACTIONS
            .get(menu.selection)
            .copied()
            .unwrap_or(ServiceAction::Restart);
        if self.select_service_control(&menu.service, action) {
            let _ = self.apply_action(AppAction::RequestServiceControl);
        }
    }

    /// Open the process-action menu for the selected Applications-page row.
    /// Returns false when not on the Applications page or no row is available.
    #[must_use]
    pub fn open_process_menu(&mut self) -> bool {
        if self.page() != AppPage::Applications {
            return false;
        }
        // Resolve the canonical visual row. A structural or application
        // aggregate header has no single process, so the menu stays closed.
        let Some(item) = self.selected_detail_process() else {
            return false;
        };
        let Some(identity) = ProcessLiveKey::from_process(&item) else {
            return false;
        };
        self.open_local_surface(TuiSurface::ProcessMenu(Box::new(ProcessMenuTarget {
            item,
            identity,
            selection: 0,
        })));
        true
    }

    /// Move the process-action menu cursor (clamped).
    pub fn process_menu_move(&mut self, delta: isize) {
        if let Some(menu) = self.process_menu_mut() {
            menu.selection = menu
                .selection
                .saturating_add_signed(delta)
                .min(ui::process_menu::MENU_ACTIONS.len() - 1);
        }
    }

    /// Resolve the chosen process action into a [`PlatformEffect`]. Control
    /// actions (End / End process tree / Suspend / Resume / Kill / priority)
    /// route through the shell's shared batch path — Kill, End-task, and the
    /// tree-end are gated behind their confirmation overlays — while the
    /// integration actions (open location / search online) route through the
    /// platform integration ports. Returns `None` with an honest status line
    /// when the frozen row lacks the identity/name the action needs.
    #[must_use]
    pub fn process_menu_select(&mut self) -> Option<PlatformEffect> {
        let TuiSurface::ProcessMenu(menu) = self.take_local_surface(TuiSurfaceKind::ProcessMenu)?
        else {
            return None;
        };
        let action = ui::process_menu::MENU_ACTIONS
            .get(menu.selection)
            .copied()
            .unwrap_or(ui::process_menu::ProcessMenuAction::OpenLocation);
        let is_control = matches!(
            action,
            ui::process_menu::ProcessMenuAction::EndTask
                | ui::process_menu::ProcessMenuAction::EndProcessTree
                | ui::process_menu::ProcessMenuAction::Suspend
                | ui::process_menu::ProcessMenuAction::Resume
                | ui::process_menu::ProcessMenuAction::Kill
                | ui::process_menu::ProcessMenuAction::PriorityHigh
                | ui::process_menu::ProcessMenuAction::PriorityNormal
                | ui::process_menu::ProcessMenuAction::PriorityLow
        );
        if is_control
            && !self
                .shell
                .select_row_id(ProcessRowId::Process(menu.identity))
        {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "The frozen process is no longer in the list",
            );
            return None;
        }
        match action {
            ui::process_menu::ProcessMenuAction::EndTask => self
                .shell
                .apply_action(taskmanager_application::AppAction::RequestEndTask),
            ui::process_menu::ProcessMenuAction::EndProcessTree => {
                self.shell.request_process_tree_end(menu.identity);
                None
            }
            ui::process_menu::ProcessMenuAction::Kill => {
                self.shell.request_process_batch(ProcessBatchAction::Kill)
            }
            ui::process_menu::ProcessMenuAction::Suspend => self
                .shell
                .request_process_batch(ProcessBatchAction::Suspend),
            ui::process_menu::ProcessMenuAction::Resume => {
                self.shell.request_process_batch(ProcessBatchAction::Resume)
            }
            ui::process_menu::ProcessMenuAction::PriorityHigh
            | ui::process_menu::ProcessMenuAction::PriorityNormal
            | ui::process_menu::ProcessMenuAction::PriorityLow => {
                // priority_tier is total over the priority variants; the
                // Normal fallback keeps the production tree panic-free.
                let tier = ui::process_menu::priority_tier(action)
                    .unwrap_or(taskmanager_core::core::process::PriorityTier::Normal);
                self.shell
                    .request_process_batch(ProcessBatchAction::SetPriority(tier))
            }
            ui::process_menu::ProcessMenuAction::Affinity => {
                if let Some(target) =
                    taskmanager_core::core::process::FrozenProcessIdentity::from_process(&menu.item)
                {
                    self.open_process_affinity_for(target)
                } else {
                    self.open_process_affinity()
                }
            }
            ui::process_menu::ProcessMenuAction::OpenLocation
            | ui::process_menu::ProcessMenuAction::SearchOnline => {
                match ui::process_menu::resolve_action(&menu) {
                    Some(effect) => Some(effect),
                    None => {
                        self.report_notice(
                            FeedbackSource::Interaction,
                            FeedbackSeverity::Warning,
                            FeedbackLifecycle::SHORT,
                            "This process has no location or name to open",
                        );
                        None
                    }
                }
            }
        }
    }

    /// Open the batch-control menu (`B`) for the marked multi-select set on
    /// the Applications page. The menu only opens when at least one process is
    /// marked (`m`); an empty set reports an honest status line instead of a
    /// dead-end menu. The count is frozen at open time so the menu labels its
    /// actions honestly even if a later refresh prunes a died pid.
    #[must_use]
    pub fn open_batch_menu(&mut self) -> bool {
        if self.page() != AppPage::Applications {
            return false;
        }
        let marked_count = if self.shell.selected_identities().is_empty() {
            0
        } else {
            self.shell
                .marked_process_control_availability()
                .target_count()
        };
        if marked_count == 0 {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "No processes marked — use m to mark rows for batch control",
            );
            return false;
        }
        self.open_local_surface(TuiSurface::BatchMenu(BatchMenuTarget {
            selection: 0,
            marked_count,
        }));
        true
    }

    /// Move the batch-menu cursor (clamped to the action rows).
    pub fn batch_menu_move(&mut self, delta: isize) {
        if let Some(menu) = self.batch_menu_mut() {
            menu.selection = menu
                .selection
                .saturating_add_signed(delta)
                .min(ui::batch_menu::MENU_ACTIONS.len() - 1);
        }
    }

    /// Consume the batch-menu selection: route the chosen action through the
    /// shell's shared batch path — the frozen intent targets the LIVE marked
    /// set (the shell re-freezes from `selected_pids`), so a refresh between
    /// open and confirm can only narrow the scope, never widen it. Destructive
    /// actions (End / Kill) gate behind the batch confirmation; the rest emit
    /// `ExecuteBatch` directly. The Clear action empties the set instead.
    #[must_use]
    pub fn batch_menu_select(&mut self) -> Option<PlatformEffect> {
        let TuiSurface::BatchMenu(menu) = self.take_local_surface(TuiSurfaceKind::BatchMenu)?
        else {
            return None;
        };
        let action = ui::batch_menu::MENU_ACTIONS
            .get(menu.selection)
            .copied()
            .unwrap_or(ui::batch_menu::BatchMenuAction::Suspend);
        match action {
            ui::batch_menu::BatchMenuAction::End => {
                self.shell.request_process_batch(ProcessBatchAction::End)
            }
            ui::batch_menu::BatchMenuAction::Kill => {
                self.shell.request_process_batch(ProcessBatchAction::Kill)
            }
            ui::batch_menu::BatchMenuAction::Suspend => self
                .shell
                .request_process_batch(ProcessBatchAction::Suspend),
            ui::batch_menu::BatchMenuAction::Resume => {
                self.shell.request_process_batch(ProcessBatchAction::Resume)
            }
            ui::batch_menu::BatchMenuAction::PriorityHigh
            | ui::batch_menu::BatchMenuAction::PriorityNormal
            | ui::batch_menu::BatchMenuAction::PriorityLow => {
                // priority_tier is total over the priority variants; the
                // Normal fallback keeps the production tree panic-free.
                let tier = ui::batch_menu::priority_tier(action)
                    .unwrap_or(taskmanager_core::core::process::PriorityTier::Normal);
                self.shell
                    .request_process_batch(ProcessBatchAction::SetPriority(tier))
            }
            ui::batch_menu::BatchMenuAction::Clear => {
                self.shell.clear_selected_rows();
                self.report_notice(
                    FeedbackSource::Interaction,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    "Batch selection cleared",
                );
                None
            }
        }
    }

    /// Open the Process Properties modal for the selected Applications-page row,
    /// mirroring the GPUI Properties dialog. Returns false when not on the
    /// Applications page or no process row is selected (a group header in a
    /// grouped mode has no single process, so the modal honestly stays closed).
    /// The frozen row is cloned at open time so a list refresh cannot redirect
    /// the view; the modal opens on the Overview tab (GPUI default).
    #[must_use]
    pub fn open_process_properties(&mut self) -> bool {
        if self.page() != AppPage::Applications {
            return false;
        }
        let Some(item) = self.selected_detail_process() else {
            return false;
        };
        let Some(identity) =
            taskmanager_core::core::process::FrozenProcessIdentity::from_process(&item)
        else {
            return false;
        };
        self.close_local_overlays();
        self.shell.close_service_log();
        self.shell.close_search();
        self.focus_panel = crate::FocusPanel::Table;
        self.process_properties_view = Some(ProcessPropertiesTarget {
            item,
            section: ProcessDetailsSection::default(),
            scroll: 0,
        });
        self.shell.open_process_properties_for(identity)
    }

    /// Advance the Properties modal's active tab to the next section in the
    /// Overview → Performance → Command → Insights cycle. No-op when the modal
    /// is closed. Resets the tab-body scroll offset to 0 — each tab is
    /// independent content, so a stale offset from the previous tab must not
    /// survive the switch.
    pub fn process_properties_next_tab(&mut self) {
        if let Some(target) = self.process_properties_mut() {
            target.section = target.section.next();
            target.scroll = 0;
        }
    }

    /// Advance the Properties modal's active tab to the previous section.
    /// No-op when the modal is closed. Resets the tab-body scroll offset (see
    /// [`Self::process_properties_next_tab`]).
    pub fn process_properties_prev_tab(&mut self) {
        if let Some(target) = self.process_properties_mut() {
            target.section = target.section.prev();
            target.scroll = 0;
        }
    }

    /// Scroll the Properties modal's tab body by `delta` lines (positive =
    /// down). The renderer clamps the stored intent to the valid range, so this
    /// only stores the user's intent; it never reads the render area. No-op when
    /// the modal is closed.
    pub fn process_properties_scroll_by(&mut self, delta: isize) {
        if let Some(target) = self.process_properties_mut() {
            if delta >= 0 {
                target.scroll = target.scroll.saturating_add(delta as usize);
            } else {
                target.scroll = target.scroll.saturating_sub(delta.unsigned_abs());
            }
        }
    }

    /// Open the session-action menu for the selected Users-page row. Returns
    /// false (and an honest status line) when not on the Users page or no row
    /// is available. The cursor indexes the sorted projection the renderer
    /// paints (`sorted_session_at` is the single row→target translation).
    #[must_use]
    pub fn open_session_menu(&mut self) -> bool {
        if self.page() != AppPage::Users {
            return false;
        }
        let Some(session) = self.sorted_session_at(self.selected) else {
            return false;
        };
        if session.id.as_str().is_empty() {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "This session row has no provider target",
            );
            return false;
        }
        self.open_local_surface(TuiSurface::SessionMenu(SessionMenuTarget {
            session: session.clone(),
            selection: 0,
        }));
        true
    }

    /// Move the session-action menu cursor (clamped).
    pub fn session_menu_move(&mut self, delta: isize) {
        if let Some(menu) = self.session_menu_mut() {
            menu.selection = menu
                .selection
                .saturating_add_signed(delta)
                .min(ui::session_menu::MENU_ACTIONS.len() - 1);
        }
    }

    /// Consume the menu selection: arm the shared shell confirmation gate
    /// ([`taskmanager_shell::ShellApp::select_session_control`]) with the
    /// frozen row + chosen action. The platform request is only emitted by
    /// [`Self::confirm_session_control`]. The frozen row's id is checked
    /// against the current selection so a refresh cannot redirect the intent
    /// to a different row (mirrors the startup menu's guard).
    pub fn session_menu_select(&mut self) {
        let Some(TuiSurface::SessionMenu(menu)) =
            self.take_local_surface(TuiSurfaceKind::SessionMenu)
        else {
            return;
        };
        let action = ui::session_menu::MENU_ACTIONS
            .get(menu.selection)
            .copied()
            .unwrap_or(SessionControlAction::Disconnect);
        let still_selected = self
            .sorted_session_at(self.selected)
            .is_some_and(|session| session.id == menu.session.id);
        if !still_selected {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "Session target changed; action cancelled",
            );
            return;
        }
        if !self.shell.select_session_control(&menu.session, action) {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "This session row has no provider target",
            );
        }
    }

    /// Confirm the pending session action through the shell's gated path:
    /// emit the renderer-neutral `PlatformEffect::SessionControl` built from
    /// the frozen target and clear the gate. The frozen identity is what the
    /// confirmation overlay displayed, so a refresh between arm and confirm
    /// can never retarget the request.
    #[must_use]
    pub fn confirm_session_control(&mut self) -> Option<PlatformEffect> {
        self.shell.confirm_session_control()
    }

    /// Arm the shared SMART self-test confirmation gate (`t` on the
    /// Performance·Disk page; the palette row runs the same method under the
    /// same device guard). Only the application interaction gate opens — the
    /// platform request is emitted exclusively by the gate's `y`, and the
    /// runtime routes that effect through the shared `queue_effect` seam,
    /// which owns the request session (begin → submit → accept/reject).
    /// Nothing renders as progress before the provider actually accepted the
    /// request: until then the honest state is "requested", reported through
    /// the shell's typed footer feedback. Returns whether a gate armed.
    #[must_use]
    pub fn arm_smart_self_test(&mut self) -> bool {
        let Some(intent) = smart_self_test_target(self) else {
            return false;
        };
        let reduction =
            self.shell
                .application
                .interaction
                .reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::SmartSelfTest(intent),
                ));
        match reduction.transition {
            SurfaceTransition::Opened(_) | SurfaceTransition::Replaced { .. } => {
                // The same input-mode reset the shell's own arm path applies
                // when a shared surface opens.
                self.shell.reset_input_mode();
                true
            }
            SurfaceTransition::Unchanged
            | SurfaceTransition::Confirmed(_)
            | SurfaceTransition::Dismissed { .. } => false,
        }
    }

    /// Open the interactive service dependencies browsing modal for the selected service.
    #[must_use]
    pub fn open_service_dependencies(&mut self) -> bool {
        if self.page() != taskmanager_application::AppPage::Services {
            return false;
        }
        let Some(service) = self.sorted_service_at(self.selected) else {
            return false;
        };
        let target_id = service.id.clone();
        let target_name = service.name.clone();
        self.open_local_surface(crate::TuiSurface::ServiceDependencies(
            crate::ServiceDependenciesTarget {
                service_id: target_id,
                service_name: target_name,
                scroll: 0,
            },
        ));
        true
    }

    /// Scroll the interactive service dependencies modal.
    pub fn service_dependencies_scroll(&mut self, delta: isize) {
        if let Some(target) = self.service_dependencies_mut() {
            if delta < 0 {
                target.scroll = target.scroll.saturating_sub(delta.unsigned_abs());
            } else {
                target.scroll = target.scroll.saturating_add(delta.unsigned_abs());
            }
        }
    }

    /// Open the CPU affinity editor modal for the selected process on the Applications page.
    #[must_use]
    pub fn open_process_affinity(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Applications {
            return None;
        }
        let item = self.selected_detail_process()?;
        let target = taskmanager_core::core::process::FrozenProcessIdentity::from_process(&item)?;
        self.open_process_affinity_for(target)
    }

    /// Open the CPU affinity editor modal for a specific frozen process
    /// identity. A correlated snapshot the shell already holds for this exact
    /// target seeds the controls immediately; a fresh read is queued so the
    /// authoritative mask arrives while the editor is open. Before that read
    /// lands, the grid shows every CPU unchecked and neither the toggles nor
    /// Enter submit anything — an unobserved mask is never fabricated into an
    /// editable default (Iced/GPUI fail closed the same way).
    #[must_use]
    pub fn open_process_affinity_for(
        &mut self,
        target: taskmanager_core::core::process::FrozenProcessIdentity,
    ) -> Option<PlatformEffect> {
        let logical_cpu_count = self.logical_cpu_count();
        let current_mask = match self.shell.process_affinity_state() {
            taskmanager_application::ProcessAffinityState::Ready(ready)
                if ready.target == target =>
            {
                Some(ready.cpus.clone())
            }
            _ => None,
        };
        let state = crate::surface::AffinityModalState::new(
            target.clone(),
            current_mask,
            logical_cpu_count,
        );
        self.open_local_surface(TuiSurface::ProcessAffinity(state));
        Some(PlatformEffect::ProcessAffinity(ProcessAffinityRequest {
            target,
        }))
    }

    /// Copy an authoritative affinity read into the open editor, but only
    /// when the snapshot's frozen target is the identity captured at open
    /// time. A different process with the same PID is never allowed to
    /// rewrite the visible mask (the Iced editor's same-wave sync rule).
    pub fn sync_process_affinity_modal(&mut self) {
        let Some(crate::TuiSurface::ProcessAffinity(state)) = self.local_surface() else {
            return;
        };
        let target = state.target.clone();
        let observed = affinity_last_good(self.shell.process_affinity_state(), &target)
            .map(|ready| ready.cpus.clone());
        let Some(cpus) = observed else {
            return;
        };
        if let Some(crate::TuiSurface::ProcessAffinity(state)) = self.local_surface_mut() {
            state.observe_mask(&cpus);
        }
    }

    /// Apply the edited CPU affinity mask, emit the control request, and close
    /// the modal. The decision fails closed: before an authoritative read is
    /// observed, Enter reports the honest collecting notice and keeps the
    /// editor open; an observed-but-empty mask reports the selection warning
    /// and keeps the editor open. Only the displayed mask is ever submitted —
    /// Apply never asks the shell to resolve the row again, so a recycled PID
    /// cannot be silently retargeted, and the user's in-progress edits are
    /// judged as displayed (authoritative reads rewrite the mask when the
    /// batch folds in, never retroactively at decision time).
    #[must_use]
    pub fn apply_process_affinity(&mut self) -> Option<PlatformEffect> {
        let Some(crate::TuiSurface::ProcessAffinity(state)) = self.local_surface() else {
            return None;
        };
        let mask_observed = state.mask_observed;
        let cpus = state.selected_mask.clone();
        if !mask_observed {
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("common.collecting_telemetry"),
            );
            return None;
        }
        if cpus.is_empty() {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("proc.affinity_select_one"),
            );
            return None;
        }
        let TuiSurface::ProcessAffinity(state) =
            self.take_local_surface(TuiSurfaceKind::ProcessAffinity)?
        else {
            return None;
        };
        self.shell
            .request_process_affinity_control_for(state.target, state.selected_mask)
    }
}

/// The last-good affinity snapshot for `target`, mirroring the Iced editor's
/// read discipline: an exact `Ready` match, or a `Loading`/`Failed` session
/// that still carries a previous good read for the same frozen identity.
#[must_use]
fn affinity_last_good<'a>(
    state: &'a ProcessAffinityState,
    target: &taskmanager_core::core::process::FrozenProcessIdentity,
) -> Option<&'a ProcessAffinityReady> {
    match state {
        ProcessAffinityState::Ready(ready) if &ready.target == target => Some(ready),
        ProcessAffinityState::Loading {
            target: loading,
            last_good: Some(ready),
            ..
        } if loading == target => Some(ready),
        ProcessAffinityState::Failed {
            target: failed,
            last_good: Some(ready),
            ..
        } if failed == target => Some(ready),
        ProcessAffinityState::Closed
        | ProcessAffinityState::Loading { .. }
        | ProcessAffinityState::Ready(_)
        | ProcessAffinityState::Failed { .. } => None,
    }
}

/// Resolve the frozen SMART self-test intent the Performance·Disk surface can
/// offer, or `None` when the current snapshot has no disk whose provider
/// reports [`SmartAvailability::Available`] (the exact readiness GPUI's
/// health view demands before it enables its self-test actions). The TUI disk
/// view has no per-disk cursor, so the target is the first SMART-capable disk
/// in snapshot order — the same first-disk fallback GPUI uses when no device
/// is selected. The intent freezes the full identity (device id, hot-plug
/// generation, provider locator, display name), so the shared confirmation
/// names the disk it would act on and `y` can only submit what was displayed.
/// The Short kind is the terminal entry's single offered test (GPUI also
/// offers Extended; a terminal menu surface for the second kind stays out of
/// this crate's current surface vocabulary).
#[must_use]
pub(crate) fn smart_self_test_target(app: &TuiApp) -> Option<SmartSelfTestIntent> {
    let disks = &app.projection().snapshot.as_ref()?.disks;
    let disk = disks
        .iter()
        .find(|disk| disk.smart_availability == SmartAvailability::Available)?;
    Some(SmartSelfTestIntent {
        device_id: DeviceId::new(disk.device_id.clone()),
        device_generation: disk.device_generation,
        device_key: StorageDeviceKey::new(disk.name.clone()),
        display_name: if disk.model.is_empty() {
            disk.name.clone()
        } else {
            disk.model.clone()
        },
        kind: SmartSelfTestKind::Short,
    })
}
