//! Read-only accessors and the config/sampling methods for [`super::IcedApp`],
//! extracted from the state module so it stays under the source-size budget.
//! The view/test-facing getters, the launch-time config load, and the
//! perf-history sampling path live here; the heavier update/tick paths stayed
//! in `app.rs`. Moved verbatim — the [`IcedApp`] surface is unchanged.

use super::viewport_state::ViewportRegion;
use super::*;
use crate::perf_history::ProcessPerfHistory;
use taskmanager_core::core::process::ProcessLiveKey;

impl IcedApp {
    /// The view reads this to decide the search cursor rendering.
    #[must_use]
    pub fn is_demo(&self) -> bool {
        self.runtime.is_demo()
    }

    /// Active renderer-local language.
    #[must_use]
    pub fn language(&self) -> Language {
        self.configuration.language()
    }

    /// Renderer theme resolved from the same canonical configuration snapshot
    /// as [`Self::preferences`] and [`Self::language`].
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        self.configuration.theme()
    }

    /// Immutable presentation values derived from the applied config snapshot.
    #[must_use]
    pub fn preferences(&self) -> &PresentationPreferences {
        self.configuration.preferences()
    }

    /// Installed family names offered by the Settings font pickers. The
    /// snapshot is bounded and already excludes the two bundled product faces.
    #[must_use]
    pub fn font_availability(&self) -> &taskmanager_theme::FontAvailability {
        &self.configuration.preferences().font_availability
    }

    /// The per-process Performance-tab window for the process whose details
    /// modal is open (`None` until the overlay opens on a process carrying
    /// provider history, or the first telemetry refresh samples it).
    #[must_use]
    pub fn process_perf_history(&self) -> Option<&ProcessPerfHistory> {
        self.performance.process_history.as_ref()
    }

    /// Shared contiguous process-property series for the Performance tab.
    /// Cache hits clone only four `Rc` handles; the bounded ring-to-slice copy
    /// happens once per pid/revision instead of once per modal repaint.
    #[must_use]
    pub(crate) fn process_perf_series(
        &self,
    ) -> Option<crate::perf_history::ProcessPerfHistorySnapshot> {
        let history = self.performance.process_history.as_ref()?;
        Some(self.projection_caches.process_performance(history))
    }

    /// The process-details modal's active section tab.
    #[must_use]
    pub fn details_section(&self) -> DetailsSection {
        self.process_presentation.details_section
    }

    /// The modal-entrance progress (1.0 = fully visible; also 1.0 while no
    /// modal is open so non-modal frames render unchanged).
    #[must_use]
    pub fn modal_appear_progress(&self) -> f32 {
        self.input
            .modal_appear
            .as_ref()
            .map_or(1.0, |appear| appear.progress)
    }

    /// The Services-page name filter.
    #[must_use]
    pub fn services_query(&self) -> &str {
        &self.process_presentation.services_query
    }

    /// Tick-injected wall time used by service-log window filtering.
    #[must_use]
    pub(crate) const fn service_log_now_micros(&self) -> u64 {
        self.window_time.service_log_now_micros()
    }

    /// The Performance-page resource currently selected for detail rendering
    /// (the select-a-device model). Read by the view each frame.
    #[must_use]
    pub fn perf_device(&self) -> PerfDevice {
        self.performance.selected_device
    }

    /// The GPU row the shared chart-metric selection is bound to: the
    /// selected GPU device's row from the snapshot. `None` when another
    /// resource (or no GPU) is viewed — the shell fold then leaves the
    /// selection untouched (ADR-034 stage 2).
    #[must_use]
    pub fn viewed_gpu(&self) -> Option<&taskmanager_core::core::metrics::GpuMetrics> {
        let index = match self.performance.selected_device {
            PerfDevice::Gpu(index) => index,
            _ => return None,
        };
        self.shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.gpu.get(index))
    }

    /// The Applications-page process-state bucket currently projected by the
    /// shared shell. The renderer-local control and every row/action path read
    /// this same state.
    #[must_use]
    pub fn process_status_filter(&self) -> ProcessStatusFilter {
        self.shell.process_status_filter
    }

    /// Renderer-local Applications scroll offset plus a bounded first-layout
    /// viewport fallback. The real table height arrives through Iced's
    /// `Viewport` callback after layout; using the window height meanwhile
    /// keeps the first body build viewport-aware instead of eager.
    pub(crate) fn applications_virtual_scroll(&self) -> (f32, f32) {
        virtual_scroll(
            self.viewport.scroll(ViewportRegion::Applications),
            self.viewport.size().height,
        )
    }

    /// Renderer-local App-history scroll offset.
    pub(crate) fn app_history_scroll_y(&self) -> f32 {
        self.viewport.scroll(ViewportRegion::AppHistory).offset_y()
    }

    /// App-history's observed viewport height, with the same bounded first
    /// layout fallback as Applications.
    pub(crate) fn app_history_virtual_viewport_height(&self) -> f32 {
        self.viewport
            .scroll(ViewportRegion::AppHistory)
            .viewport_height(self.viewport.size().height)
    }

    /// Renderer-local Services scroll offset and first-layout viewport.
    pub(crate) fn services_virtual_scroll(&self) -> (f32, f32) {
        virtual_scroll(
            self.viewport.scroll(ViewportRegion::Services),
            self.viewport.size().height,
        )
    }

    /// Renderer-local Startup scroll offset and first-layout viewport.
    pub(crate) fn startup_virtual_scroll(&self) -> (f32, f32) {
        virtual_scroll(
            self.viewport.scroll(ViewportRegion::Startup),
            self.viewport.size().height,
        )
    }

    /// Renderer-local Users scroll offset and first-layout viewport.
    pub(crate) fn users_virtual_scroll(&self) -> (f32, f32) {
        virtual_scroll(
            self.viewport.scroll(ViewportRegion::Users),
            self.viewport.size().height,
        )
    }

    /// Wide Performance rail viewport: cards scroll on the vertical axis.
    pub(crate) fn performance_rail_vertical_scroll(&self) -> (f32, f32) {
        (
            self.viewport
                .scroll(ViewportRegion::PerformanceRail)
                .offset_y(),
            self.viewport
                .scroll(ViewportRegion::PerformanceRail)
                .viewport_height(self.viewport.size().height),
        )
    }

    /// Compact Performance selector viewport: device pills scroll on the
    /// horizontal axis, preserving the independent strip contract.
    pub(crate) fn performance_rail_horizontal_scroll(&self) -> (f32, f32) {
        (
            self.viewport
                .scroll(ViewportRegion::PerformanceRail)
                .offset_x(),
            self.viewport
                .scroll(ViewportRegion::PerformanceRail)
                .viewport_width(self.viewport.size().width),
        )
    }

    /// Stable widget operation identity for the Applications scrollable.
    pub(crate) fn applications_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::Applications).id()
    }

    /// Stable widget operation identity for the App-history scrollable.
    pub(crate) fn app_history_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::AppHistory).id()
    }

    pub(crate) fn services_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::Services).id()
    }

    pub(crate) fn startup_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::Startup).id()
    }

    pub(crate) fn users_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::Users).id()
    }

    pub(crate) fn performance_rail_scroll_id(&self) -> iced::widget::Id {
        self.viewport.scroll(ViewportRegion::PerformanceRail).id()
    }

    /// Whether the named group's member rows are expanded in a grouped view.
    /// Read by the view each frame; a group whose name is absent renders only
    /// its header.
    #[must_use]
    pub fn is_group_expanded(&self, name: &str) -> bool {
        self.process_presentation.expanded_groups.contains(name)
    }

    /// Persist the selected top-level page as the remember-last token (the
    /// counterpart of the startup projection). Only the pages iced
    /// renders are recorded; other tokens keep their previous value.
    pub(super) fn persist_last_page(&mut self, page: AppPage) {
        let token = match page {
            AppPage::Performance => "performance",
            AppPage::Applications => "apps",
            _ => return,
        };
        let mut config = self.config_draft();
        config.last_page = token.to_string();
        self.commit_config_draft(config);
    }

    /// Build the toolkit-neutral semantic snapshot for the current shell.
    ///
    /// This is intentionally a detached contract projection (owner decision
    /// G-15, decision 10): the Iced slice has no native accessibility bridge,
    /// makes no AT-SPI or screen-reader availability claim, and keeps no
    /// live-loop call site — see [`crate::a11y`].
    #[must_use]
    pub fn semantic_snapshot(&self) -> Option<taskmanager_ui_contract::SemanticSnapshot> {
        crate::a11y::semantic_snapshot(&self.shell)
    }

    /// Build the semantic snapshot including frontend-local routes: the
    /// alerts page's managed rule rows publish as a named switch group while
    /// that route is open. Same detached-projection policy as
    /// [`Self::semantic_snapshot`] (no live-loop call site).
    #[must_use]
    pub fn semantic_snapshot_with_local(
        &self,
    ) -> Option<taskmanager_ui_contract::SemanticSnapshot> {
        crate::a11y::semantic_snapshot_with_local(self)
    }

    /// Record one per-process Performance-tab point from the latest resolved
    /// snapshot, for the process whose properties overlay is open. Deduped by
    /// snapshot watermark: a tick that does not advance `timestamp_ms` pushes
    /// nothing. Public to the crate's test module so the ring logic can be
    /// exercised without a live platform client. (The former system-wide
    /// headline push was retired with the renderer-local ring, G-02 — the
    /// Performance chart reads the shared shell series.)
    pub(crate) fn sample_process_history(&mut self) {
        let timestamp_ms = match self.shell.projection().snapshot.as_ref() {
            Some(snapshot) => snapshot.timestamp_ms,
            None => return,
        };
        if self.performance.last_sampled_snapshot_ms == Some(timestamp_ms) {
            return;
        }
        self.sample_process_perf_history();
        self.performance.last_sampled_snapshot_ms = Some(timestamp_ms);
    }

    /// Record one per-process Performance-tab point for the process whose
    /// properties overlay is open (the frozen target pid). The window is
    /// created with the persisted graph-data-points capacity and re-pointed
    /// automatically when the target pid changes.
    fn sample_process_perf_history(&mut self) {
        let Some(target) = properties_target_identity(&self.shell) else {
            return;
        };
        let Some(process) = self
            .shell
            .projection()
            .processes
            .as_deref()
            .and_then(|processes| {
                processes
                    .iter()
                    .find(|process| ProcessLiveKey::from_process(process) == Some(target))
            })
        else {
            return;
        };
        let capacity = self.graph_data_points();
        let entry = self
            .performance
            .process_history
            .get_or_insert_with(|| ProcessPerfHistory::new(capacity));
        if entry.identity() != Some(target) || entry.capacity() != capacity {
            entry.resize(capacity, target);
        }
        entry.push(
            target,
            process.current_cpu_percentage(),
            process.current_memory_bytes(),
            process.current_disk_read_bytes_per_sec(),
            process.current_disk_write_bytes_per_sec(),
        );
    }

    /// Seed the per-process Performance-tab window from the
    /// provider-pre-populated history the moment the details overlay opens
    /// (G-14): the Linux provider fills ~60 s of
    /// `ProcessItem::cpu_history`/`mem_history`/`disk_read_history`/
    /// `disk_write_history`, which the live-only window previously discarded
    /// (the ring sampled from an empty window until enough ticks passed). A
    /// provider that leaves the windows empty (mac/win) still re-points the
    /// ring at the new target with an honest empty window — the live-only
    /// fallback, never the previous process's samples.
    pub(super) fn seed_process_perf_history_from_provider(&mut self) {
        let Some(target) = properties_target_identity(&self.shell) else {
            return;
        };
        let Some(process) = self
            .shell
            .projection()
            .processes
            .as_deref()
            .and_then(|processes| {
                processes
                    .iter()
                    .find(|process| ProcessLiveKey::from_process(process) == Some(target))
            })
        else {
            return;
        };
        let capacity = self.graph_data_points();
        let entry = self
            .performance
            .process_history
            .get_or_insert_with(|| ProcessPerfHistory::new(capacity));
        entry.seed_from_provider(
            target,
            capacity,
            &process.cpu_history,
            &process.mem_history,
            &process.disk_read_history,
            &process.disk_write_history,
        );
    }
}

fn virtual_scroll(state: &VirtualScrollState, window_height: f32) -> (f32, f32) {
    (state.offset_y(), state.viewport_height(window_height))
}

/// The frozen pid behind the open process-properties overlay, when open
/// (shared by the app's sampling path and the test suite).
fn properties_target_identity(shell: &ShellApp) -> Option<ProcessLiveKey> {
    shell
        .process_properties_target()
        .and_then(|target| target.live_key())
}
