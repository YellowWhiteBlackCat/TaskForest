//! Ratatui frontend for TaskForest's shared application and native platform ports.
//!
//! The renderer-independent shell state machine lives in `taskmanager-shell`
//! (ADR-027); `ui` is the only Ratatui rendering layer, `theme` maps the
//! neutral skin registry onto terminal colors, and `runtime` is the only
//! Crossterm/native-runtime wiring layer.
//!
//! [`TuiApp`] owns the shell state machine plus the TUI-local surface: the
//! settings/about/health/containers overlays, the service-action menu, the
//! export feedback, and the runtime theme construction parameters. Container
//! rollups themselves remain in the shared `SystemProjectionStore` owned by the shell.

#![forbid(unsafe_code)]

mod bindings;
mod capabilities;
mod clipboard;
mod column_prefs;
mod command_palette;
mod demo;
mod functional;
mod history_runtime;
mod menus;
mod preferences;
mod process_view;
mod runtime;
mod selection;
mod selectors;
mod snapshot_export;
mod startup_control;
mod surface;
mod terminal;
mod theme;
mod ui;

pub use preferences::AppliedPrefs;

pub use bindings::binding_declaration;

pub use capabilities::capability_declaration;

pub use functional::functional_declaration;

pub use command_palette::{CommandPalette, CommandPaletteRow, PaletteLocalAction};

pub use demo::demo_app;
pub use menus::BatchMenuTarget;
pub use runtime::{run_demo, run_live, snapshot_text};
pub use selectors::{FocusPanel, PerfDevice};
pub(crate) use surface::{TuiInputScope, TuiSurface, TuiSurfaceKind, TuiSurfaceState};
pub use terminal::{TuiColorMode, TuiGlyphMode, TuiTerminalProfile};
pub use theme::{ThemeParams, TuiTheme};
pub use ui::process_menu::ProcessMenuTarget;
pub use ui::process_properties::{ProcessDetailsSection, ProcessPropertiesTarget};
pub use ui::render;
pub use ui::service_menu::ServiceMenuTarget;
pub use ui::session_menu::SessionMenuTarget;
pub use ui::settings::SettingsForm;
pub use ui::startup_menu::StartupMenuTarget;

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use taskmanager_application::{
    AppAction, AppPage, ConfigClient, ConfigRevision, DirectoryUsageRequest, PlatformEffect,
    PlatformEventBatch, RefreshRequest, i18n::t,
};
use taskmanager_core::core::config::Config;
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanSpec, DirectoryScanStatus,
};
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{GpuMetrics, SystemSnapshot};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem};
use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, InputDispatch, ShellKeyEvent, SortCol,
};

/// Inactivity window (micros) after which the Applications-page prefix jump
/// resets: two consecutive bare characters within the window extend the
/// prefix (htop-style), a pause longer than this starts a fresh jump.
pub(crate) const PREFIX_JUMP_WINDOW_MICROS: u64 = 2_000_000;

fn default_category_expansions() -> std::collections::HashSet<String> {
    taskmanager_core::core::process::ProcessCategory::ALL
        .iter()
        .copied()
        .map(taskmanager_application::process_category_projection::category_expansion_key)
        .collect()
}

/// The terminal frontend's complete state: the shared shell state machine
/// plus renderer-specific surface state (ADR-027 keeps the latter out of
/// `ShellApp`).
pub struct TuiApp {
    /// The renderer-independent shell state machine.
    shell: taskmanager_shell::ShellApp,
    /// Immutable local-time rules supplied by the native composition root.
    pub(crate) local_time_rules: taskmanager_core::core::time::LocalTimeRulesObservation,
    /// The sole TUI-local menu/modal. Shared confirmations and Process
    /// Properties remain authoritative in `application.interaction`.
    local_surface: TuiSurfaceState,
    /// Renderer cache for the shared Process Properties surface. The cache
    /// carries the frozen row, active tab and scroll offset; visibility is
    /// decided only by `application.interaction`.
    process_properties_view: Option<ProcessPropertiesTarget>,
    /// Keyboard-navigable settings form (values are applied on Enter).
    pub settings_form: SettingsForm,
    /// Runtime theme construction parameters; the runtime rebuilds the
    /// terminal palette from these on every frame.
    pub theme_params: ThemeParams,
    /// Non-blocking cursor into the app-host-owned configuration coordinator.
    config_client: Option<ConfigClient>,
    applied_config_revision: Option<ConfigRevision>,
    config_draft: Config,
    settings_draft: preferences::SettingsDraftLifecycle,
    /// Optional export directory; `None` exports into the current working
    /// directory (the `write_snapshot` convention).
    pub export_dir: Option<PathBuf>,
    /// Typed lifecycle plus the app-host's non-blocking export client.
    snapshot_export: snapshot_export::TuiSnapshotExportRuntime,
    /// Read-only durable-history lifecycle and replay capability.
    history_runtime: history_runtime::TuiHistoryRuntime,
    /// Frontend-local Performance resource selector (select-a-device detail
    /// model). Default [`PerfDevice::Cpu`]; only mutated by the Performance-page
    /// digit-key handler in `runtime::handle_key`.
    pub perf_device: PerfDevice,
    /// Frontend-local vertical-scroll intent for the inline selected-process
    /// detail/insights panel on the Applications page. The panel content (frozen
    /// identity rows + the bounded ProcessInsights cards) can exceed the fixed
    /// 18-row detail area on short terminals; Ctrl+Up / Ctrl+Down walk the
    /// content without moving the table cursor. The renderer clamps this to
    /// `[0, max(0, content_lines - visible_height)]`, so the stored value is the
    /// user's intent (it may grow past the max when content shrinks). Reset to 0
    /// on every table selection move and Properties-modal open so a
    /// stale offset never carries into different content.
    pub detail_scroll: usize,
    /// Vertical grid-row offset for the CPU per-core viewport. The renderer
    /// clamps it against the current topology and visible grid height; it is
    /// reset whenever the selected Performance resource changes.
    pub cpu_core_scroll: usize,
    /// Vertical line offset for the standard GPU engine viewport. The primary
    /// utilization chart and fact strip never scroll; compact layout removes
    /// this optional region entirely.
    pub gpu_engine_scroll: usize,
    /// Vertical fact offset for the System page's typed section viewport.
    /// Stored as navigation intent and clamped by the current projection and
    /// terminal height during paint.
    pub system_scroll: usize,
    /// The locale-neutral category/app/type expansion keys whose headers are
    /// currently expanded on the Applications page. Toggled by activating a
    /// header (Enter / Right). Re-seeded for the canonical category tree when
    /// the product selector is activated.
    pub expanded_groups: std::collections::HashSet<String>,
    /// The PIDs whose tree nodes are currently COLLAPSED on the Applications
    /// page in the canonical category tree. Mirrors
    /// the `flatten_tree_visible` contract: the set holds collapsed pids, so
    /// an empty set means every node expanded. Toggled by Enter / Right
    /// (expand) and Left (collapse) on a node with children.
    pub collapsed_tree: std::collections::HashSet<u32>,
    /// Bounded presentation cache for the Applications visual-row count.
    /// Its key contains every UI input that affects tree shape; it stores no
    /// borrowed process facts.
    pub(crate) visual_row_count_cache: std::cell::RefCell<Option<selection::VisualRowCountCache>>,
    /// The last provider start-token observed for each process PID. This
    /// TUI-local index prevents a reused PID from inheriting old tree
    /// presentation state; exact control authority remains
    /// `FrozenProcessIdentity` in the application layer.
    tree_identity_by_pid: std::collections::HashMap<u32, Option<u64>>,
    /// The table columns the user HID through the column menu (`C` on the
    /// Applications page). PID and Name are identity columns and can never be
    /// hidden; every other column is toggleable. The renderer (header, cells,
    /// widths) skips the hidden columns so a narrow terminal can drop the
    /// columns it cannot show, and the `s` sort cycle walks only the visible
    /// columns. TUI-local (ADR-027): the shell owns the sort value; the
    /// *selection* of visible columns is presentation state.
    pub hidden_columns: std::collections::HashSet<SortCol>,
    /// Which Applications-page panel owns the keyboard (Tab cycles the
    /// focus; the details panel consumes Up/Down as panel scroll).
    pub focus_panel: FocusPanel,
    /// The plain help overlay's vertical scroll offset (rows from the top of
    /// the binding list). The overlay shows two side-by-side columns; a short
    /// terminal or a growing binding list clips the tail, so ↑/↓ / PageUp /
    /// PageDown walk the content while the overlay owns the keys. Reset to 0
    /// whenever the overlay (re)opens.
    pub help_scroll: usize,
    /// The last process identity an insights request was issued for, used to
    /// dedupe re-requests after refreshes: the platform bumps the revision on
    /// every submission, so re-requesting an unchanged selection would reset
    /// an in-flight projection instead of letting it complete. Every key path
    /// that moves the Applications cursor re-syncs; the runtime tick re-syncs
    /// after each refresh. Cleared on page change and when the cursor lands on
    /// a row without a trustworthy identity.
    last_insights_target: Option<FrozenProcessIdentity>,
    /// Monotonic wall-clock (micros) of the last runtime tick, computed once
    /// per loop iteration (never in the render path) and consumed by the
    /// service-log time filter. Defaults to 0 for deterministic headless
    /// renders; the runtime updates it every tick.
    pub(crate) service_log_now_micros: u64,
    /// The accumulated htop-style prefix-jump string on the Applications page
    /// (consecutive bare characters, reset after
    /// [`PREFIX_JUMP_WINDOW_MICROS`] of inactivity). The cursor moves to the
    /// first process row whose name starts with the prefix; an empty string
    /// means no jump is in progress. TUI-local (ADR-027): the shell owns the
    /// search field, but this is a frontend navigation gesture, not a query.
    pub(crate) prefix_jump: String,
    /// Wall-clock micros of the last prefix-jump key, read from the same
    /// runtime-updated clock as `service_log_now_micros` so the reset window
    /// is deterministic under TestBackend. Zero means "no jump key yet".
    pub(crate) prefix_jump_at_micros: u64,
    /// The APPLIED preference mirrors the renderer reads (device visibility,
    /// unit matrix, gray-zero policy). Refreshed by [`Self::load_config`] and
    /// by a successful settings save — an unsaved form edit never leaks into
    /// the frame.
    pub(crate) prefs: AppliedPrefs,
    /// Per-page selection anchors captured when a navigation leaves a table
    /// page (`selection::PageRowAnchor`): the selected row's stable identity,
    /// so an A → B → A round trip restores the same row instead of trusting
    /// the one shared cursor index across pages whose rows moved meanwhile.
    /// Keyed by the page the anchor belongs to; consumed on restore. Never
    /// holds a positional guess — identity-less rows are not anchored.
    page_row_anchors: std::collections::HashMap<AppPage, selection::PageRowAnchor>,
}

impl TuiApp {
    /// Construct an uncomposed terminal state with no host capabilities.
    /// `runtime::run_live` is the production composition edge and injects the
    /// app-host configuration client and local-time observation explicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::from_shell(taskmanager_shell::ShellApp::new())
    }

    /// Production construction with the native composition edge's shared
    /// non-blocking configuration client.
    #[must_use]
    pub fn new_with_config_client(config_client: ConfigClient) -> Self {
        let mut app = Self::from_shell(taskmanager_shell::ShellApp::new());
        app.config_client = Some(config_client);
        app.load_config();
        app
    }

    /// Wrap a shell state machine with a fresh TUI surface (no config I/O).
    #[must_use]
    pub fn from_shell(shell: taskmanager_shell::ShellApp) -> Self {
        let mut app = Self::shell_default(shell);
        app.prefs = AppliedPrefs::default();
        app
    }

    fn shell_default(shell: taskmanager_shell::ShellApp) -> Self {
        let tree_identity_by_pid = shell
            .projection()
            .processes_slice()
            .iter()
            .map(|process| (process.pid, process.current_start_token()))
            .collect();
        Self {
            shell,
            local_time_rules: taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(
                0,
            ),
            local_surface: TuiSurfaceState::default(),
            process_properties_view: None,
            settings_form: SettingsForm::default(),
            theme_params: ThemeParams::default(),
            config_client: None,
            applied_config_revision: None,
            config_draft: Config::default(),
            settings_draft: preferences::SettingsDraftLifecycle::default(),
            export_dir: None,
            snapshot_export: snapshot_export::TuiSnapshotExportRuntime::default(),
            history_runtime: history_runtime::TuiHistoryRuntime::default(),
            perf_device: PerfDevice::Cpu,
            detail_scroll: 0,
            cpu_core_scroll: 0,
            gpu_engine_scroll: 0,
            system_scroll: 0,
            expanded_groups: default_category_expansions(),
            collapsed_tree: std::collections::HashSet::new(),
            visual_row_count_cache: std::cell::RefCell::new(None),
            tree_identity_by_pid,
            hidden_columns: std::collections::HashSet::new(),
            focus_panel: FocusPanel::Table,
            help_scroll: 0,
            last_insights_target: None,
            service_log_now_micros: 0,
            prefix_jump: String::new(),
            prefix_jump_at_micros: 0,
            prefs: AppliedPrefs::default(),
            page_row_anchors: std::collections::HashMap::new(),
        }
    }

    /// Apply a platform batch through the shell state machine. The shell is
    /// the sole owner of `SystemProjectionStore::containers` (and of every other rolling
    /// read model, including the shared `LiveGraphHistory` and typed feedback
    /// lifecycle); the TUI neither pre-scans raw events nor keeps a second
    /// outcome message. When the batch changed the process domain (the same
    /// timing the shell uses to prune its stale selections), the TUI-local
    /// per-pid tree state is pruned against the new live set so exited pids
    /// cannot leak into a later pid reuse.
    pub fn apply_platform_batch(&mut self, batch: PlatformEventBatch) {
        let process_revision_before = self.shell.projection().process_revision;
        let selected_application_anchor = self.selected_application_row_anchor();
        let selected_inventory_anchor = self.selected_inventory_row_anchor();
        let page_before = self.page();
        let services_revision_before = self.shell.projection().services_revision;
        let startup_revision_before = self.shell.projection().startup_revision;
        let sessions_revision_before = self.shell.projection().sessions_revision;
        self.shell.apply_platform_batch(batch);
        if self.shell.projection().process_revision != process_revision_before {
            self.prune_stale_tree_state();
            self.reconcile_application_row_anchor(selected_application_anchor);
        }
        let inventory_revision_changed = match page_before {
            AppPage::Services => {
                self.shell.projection().services_revision != services_revision_before
            }
            AppPage::Startup => self.shell.projection().startup_revision != startup_revision_before,
            AppPage::Users => self.shell.projection().sessions_revision != sessions_revision_before,
            _ => false,
        };
        if inventory_revision_changed {
            self.reconcile_inventory_row_anchor(selected_inventory_anchor);
        }
        // Same-wave fold (ADR-034 stage 2): the shared GPU chart-metric
        // selection reconciles against the batch's fresh GPU facts before
        // the next paint, so a generation change or a family going dark
        // falls back to the default in the very frame that carried the
        // fact — never one frame later.
        let gate = taskmanager_shell::gpu_chart_metric_gate(self.viewed_gpu());
        if self.shell.reconcile_gpu_chart_metric(&gate) {
            let selected = self.shell.gpu_chart_metric_selected();
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("tui.status.gpu_series").replacen("{}", t(selected.label_key()), 1),
            );
        }
        // Same-wave fold for the TUI-local Performance resource selection
        // (the frontend companion of the shared GPU chart-metric reconcile
        // above): a device family the batch just made disappear (hot-unplug,
        // provider going dark) cannot stay selected into the next paint — the
        // anchor falls back fail-closed to the first still-backed resource.
        self.reconcile_perf_device_anchor();
    }

    pub(crate) fn install_history_frontend_connector(
        &mut self,
        connector: Result<
            taskmanager_app_host::HistoryFrontendConnector,
            taskmanager_app_host::HistoryFrontendConnectorStartError,
        >,
    ) {
        self.history_runtime.install_connector(connector);
        self.sync_history_persistence_sink();
    }

    pub(crate) fn application_history_projection(
        &self,
    ) -> taskmanager_application::ApplicationHistoryProjection {
        self.history_runtime.projection()
    }

    pub(crate) fn application_history_unavailable_reason(
        &self,
    ) -> Option<taskmanager_application::ApplicationHistoryUnavailableReason> {
        self.history_runtime.unavailable_reason()
    }

    pub(crate) fn select_application_history_window(
        &mut self,
        window: taskmanager_core::core::history::HistoryWindow,
    ) -> bool {
        self.history_runtime.select_window(window)
    }

    pub(crate) fn drain_history_replay_completions(&mut self) -> bool {
        let changed = self.history_runtime.drain();
        if changed {
            self.sync_history_persistence_sink();
        }
        changed
    }

    pub(crate) fn request_history_frontend(&mut self, enabled: bool) {
        self.history_runtime.request(enabled);
        self.sync_history_persistence_sink();
    }

    fn sync_history_persistence_sink(&mut self) {
        self.shell
            .set_history_persistence_sink(self.history_runtime.record_sink());
    }

    /// Return the retry scope for the visible inventory page when its source
    /// policy says a refresh can help. The TUI renders this as `r` in the
    /// source banner; the request still crosses the shared platform seam.
    pub(crate) fn source_retry_request(&self) -> Option<RefreshRequest> {
        let (sources, request) = match self.page() {
            AppPage::Services => (
                self.projection().services_source.as_deref(),
                RefreshRequest::Services,
            ),
            AppPage::Startup => (
                self.projection().startup_source.as_deref(),
                RefreshRequest::Startup,
            ),
            AppPage::Users => (
                self.projection().sessions_source.as_deref(),
                RefreshRequest::Sessions,
            ),
            _ => return None,
        };
        taskmanager_application::source_notice(sources?)?
            .is_retryable()
            .then_some(request)
    }

    /// Proxy with page-change hygiene: leaving the Services page while the
    /// action menu is open (e.g. through a page shortcut racing the modal)
    /// must not leave a stale frozen target behind. The same hygiene closes
    /// the open service-log stream (its entries are only meaningful while the
    /// Services page is visible) and forgets the insights dedupe target so a
    /// return to Applications re-requests for the current row.
    pub fn apply_action(&mut self, action: AppAction) -> Option<PlatformEffect> {
        let page_before = self.page();
        let leaving_anchor = self.capture_page_row_anchor();
        let effect = self.shell.apply_action(action);
        if self.page() != page_before {
            self.remember_page_row_anchor(page_before, leaving_anchor);
            if !self.restore_page_row_anchor() {
                self.reconcile_applications_cursor();
            }
            self.close_local_overlays();
            self.close_service_log();
            self.last_insights_target = None;
            self.focus_panel = FocusPanel::Table;
            self.system_scroll = 0;
            self.persist_last_page();
        }
        if self.shell.interaction_surface().is_some()
            || self.shell.help_open()
            || self.shell.suggestions_open()
            || self.shell.search_active()
        {
            self.dismiss_local_surface();
            self.shell.close_service_log();
            self.focus_panel = FocusPanel::Table;
        }
        self.assert_surface_invariants();
        effect
    }

    /// Proxy for the shell's key handler with the same page-change hygiene.
    pub fn handle_local_key(&mut self, event: ShellKeyEvent) -> InputDispatch {
        let page_before = self.page();
        let query_before = self.query.clone();
        let leaving_anchor = self.capture_page_row_anchor();
        let effect = self.shell.handle_local_key(event);
        if self.page() != page_before {
            self.remember_page_row_anchor(page_before, leaving_anchor);
            if !self.restore_page_row_anchor() {
                self.reconcile_applications_cursor();
            }
            self.close_local_overlays();
            self.close_service_log();
            self.last_insights_target = None;
            self.focus_panel = FocusPanel::Table;
            self.system_scroll = 0;
        }
        if self.page() == AppPage::Applications && self.query != query_before {
            self.application.selected_process = None;
            self.reconcile_applications_cursor();
        }
        if self.shell.interaction_surface().is_some()
            || self.shell.help_open()
            || self.shell.suggestions_open()
            || self.shell.search_active()
        {
            self.dismiss_local_surface();
            self.shell.close_service_log();
            self.focus_panel = FocusPanel::Table;
        }
        self.assert_surface_invariants();
        effect
    }

    /// Proxy for the shell's character router that preserves the TUI's one
    /// input-owner invariant. If a shared surface opens, the inline details
    /// panel and any partial service-log owner release the keyboard.
    pub fn handle_local_char(
        &mut self,
        character: char,
        modifiers: taskmanager_application::Modifiers,
    ) -> InputDispatch {
        let query_before = self.query.clone();
        let effect = self.shell.handle_local_char(character, modifiers);
        if self.page() == AppPage::Applications && self.query != query_before {
            self.application.selected_process = None;
            self.reconcile_applications_cursor();
        }
        if self.shell.interaction_surface().is_some()
            || self.shell.help_open()
            || self.shell.suggestions_open()
            || self.shell.search_active()
        {
            self.dismiss_local_surface();
            self.shell.close_service_log();
            self.focus_panel = FocusPanel::Table;
        }
        self.assert_surface_invariants();
        effect
    }

    /// Toggle the settings overlay, closing every other modal first.
    pub fn toggle_settings(&mut self) {
        if self.settings_open() {
            self.cancel_settings();
        } else {
            self.open_local_surface(TuiSurface::Settings);
        }
    }

    /// Toggle the about overlay, closing every other modal first.
    pub fn toggle_about(&mut self) {
        if self.about_open() {
            self.dismiss_local_surface_kind(TuiSurfaceKind::About);
        } else {
            self.open_local_surface(TuiSurface::About);
        }
    }

    /// Toggle the system-health overlay, closing every other modal first.
    pub fn toggle_health(&mut self) {
        if self.health_open() {
            self.dismiss_local_surface_kind(TuiSurfaceKind::Health);
        } else {
            self.open_local_surface(TuiSurface::Health);
        }
    }

    /// Toggle the containers overlay, closing every other modal first.
    pub fn toggle_containers(&mut self) {
        if self.containers_open() {
            self.dismiss_local_surface_kind(TuiSurfaceKind::Containers);
        } else {
            self.open_local_surface(TuiSurface::Containers);
        }
    }

    /// Close every TUI-local overlay without touching the shell's modals.
    pub fn close_local_overlays(&mut self) {
        self.dismiss_local_surface();
        if self.shell.interaction_surface()
            == Some(taskmanager_application::SurfaceKind::ProcessProperties)
        {
            self.shell.dismiss_overlay();
        }
    }

    /// htop-style prefix jump: accumulate a case-insensitive name prefix from
    /// consecutive bare characters (a pause longer than
    /// [`PREFIX_JUMP_WINDOW_MICROS`] resets it) and move the Applications
    /// cursor to the first process row whose name starts with the prefix. The
    /// wall clock is the runtime-updated `service_log_now_micros` — never
    /// read in the render path. Returns the insights re-request effect for the
    /// landing row, mirroring the arrow paths; a row with no single process
    /// (a group header) still moves the cursor without an insights request.
    #[must_use]
    pub(crate) fn handle_prefix_jump(
        &mut self,
        character: char,
        now_micros: u64,
    ) -> Option<PlatformEffect> {
        if now_micros.saturating_sub(self.prefix_jump_at_micros) > PREFIX_JUMP_WINDOW_MICROS {
            self.prefix_jump.clear();
        }
        self.prefix_jump.push(character);
        self.prefix_jump_at_micros = now_micros;

        let Some(index) = self.prefix_jump_index(&self.prefix_jump) else {
            // No visible row starts with the accumulated prefix: keep the
            // cursor put — the next key within the window extends the prefix.
            let text = t("tui.status.jump").replacen("{}", &self.prefix_jump, 1);
            self.report_notice(
                FeedbackSource::Navigation,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                text,
            );
            return None;
        };
        self.detail_scroll_reset();
        let rows = self.process_rows_snapshot();
        let process = crate::process_view::process_at(&rows, index).cloned();
        let row_key = crate::process_view::row_key_at(&rows, index);
        let text = t("tui.status.jump").replacen("{}", &self.prefix_jump, 1);
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            text,
        );
        self.apply_selection_resolution_with_row(index, process, row_key)
    }

    /// The visual-row index of the first row whose name starts with `prefix`
    /// (case-insensitive). A hierarchy header matches through its label and a
    /// process node through its process name. `None` when no visible row
    /// starts with the prefix.
    #[must_use]
    fn prefix_jump_index(&self, prefix: &str) -> Option<usize> {
        let starts_with = |name: &str| {
            let name = name.as_bytes();
            let prefix = prefix.as_bytes();
            name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix)
        };
        self.process_rows_snapshot()
            .iter()
            .position(|row| match row {
                crate::process_view::ProcessRow::Group { label, .. } => starts_with(label),
                crate::process_view::ProcessRow::TreeNode { process, .. } => {
                    starts_with(&process.name)
                }
            })
    }

    /// Enter on the active search field: move the Applications cursor to the
    /// NEXT row matching the query (wrapping past the end), mirroring the
    /// graphical frontends' Enter-to-next-match. The cursor advances over the
    /// canonical visual rows; a group header counts as a
    /// match through its group name. A non-empty query with no further match
    /// leaves the cursor put. Returns the insights re-request effect for the
    /// landing row.
    #[must_use]
    pub(crate) fn jump_to_next_search_match(&mut self) -> Option<PlatformEffect> {
        let query = self.query.trim().to_owned();
        if query.is_empty() {
            return None;
        }
        let matches = |process: &ProcessItem| {
            taskmanager_core::core::text::contains_ascii_ci(&process.name, &query)
                || taskmanager_core::core::text::contains_ascii_ci(&process.cmdline, &query)
                || process.current_user().is_some_and(|user| {
                    taskmanager_core::core::text::contains_ascii_ci(&user, &query)
                })
                || (query.bytes().all(|b| b.is_ascii_digit())
                    && process.pid.to_string().contains(&query))
        };
        let rows = self.process_rows_snapshot();
        let start = self.selected.saturating_add(1);
        let index = rows
            .iter()
            .enumerate()
            .cycle()
            .skip(start)
            .take(rows.len())
            .find(|(_, row)| match row {
                crate::process_view::ProcessRow::Group { label, .. } => {
                    taskmanager_core::core::text::contains_ascii_ci(label, &query)
                }
                crate::process_view::ProcessRow::TreeNode { process, .. } => matches(process),
            })
            .map(|(index, _)| index);
        let index = index?;
        let process = crate::process_view::process_at(&rows, index).cloned();
        let row_key = crate::process_view::row_key_at(&rows, index);
        self.apply_selection_resolution_with_row(index, process, row_key)
    }

    /// Toggle the directory-usage scan lifecycle on the
    /// Performance page's Disk device. An idle or terminal slot starts a
    /// bounded scan of the first mounted partition (or `/` when none is
    /// reported); a `Scanning` slot cancels the active scan — mirroring
    /// GPUI's one-pill-per-partition start plus the conditional cancel pill,
    /// collapsed into a single keyboard toggle. The typed request crosses the
    /// shared seam: [`ShellApp::request_directory_usage`] wraps it in the
    /// `PlatformEffect::DirectoryUsage` variant the runtime routes through
    /// `queue_effect` — the exact same application lane every on-demand
    /// effect uses (G-03; no frontend-owned `PlatformClient` bypass).
    /// Progress and results fold back into the shared
    /// `SystemProjectionStore::directory_usage` slot the Disk panel renders.
    pub(crate) fn toggle_directory_scan(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Disk {
            return None;
        }
        // Cancel path: an active (Scanning) scan toggles to Cancel, mirroring
        // GPUI's conditional cancel pill (only rendered while Scanning). The
        // scan state is the shared `SystemProjectionStore` slot (latest-wins).
        if let Some(snapshot) = self.shell.projection().directory_usage.as_ref()
            && snapshot.status == DirectoryScanStatus::Scanning
        {
            let scan_id = snapshot.scan_id;
            let root = snapshot.root.clone();
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("tui.status.scan_cancelling").replacen("{}", &root, 1),
            );
            return Some(taskmanager_shell::ShellApp::request_directory_usage(
                DirectoryUsageRequest::Cancel(scan_id),
            ));
        }
        // Start path: scan the first mounted partition (or `/`), mirroring
        // GPUI's default bounds (the UI never customizes depth/entry caps).
        let root = self
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.disks.first())
            .and_then(|disk| disk.partitions.iter().find(|p| !p.mount_point.is_empty()))
            .map(|partition| partition.mount_point.clone())
            .unwrap_or_else(|| "/".to_string());
        let spec = DirectoryScanSpec {
            root: root.clone(),
            bounds: DirectoryScanBounds::default(),
        };
        self.report_notice(
            FeedbackSource::Control,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            t("tui.status.scan_started").replacen("{}", &root, 1),
        );
        Some(taskmanager_shell::ShellApp::request_directory_usage(
            DirectoryUsageRequest::StartScan(spec),
        ))
    }

    /// Toggle the per-engine GPU utilization session (`e` on the
    /// Performance·GPU page): enable submits ONE bounded engine-rows request
    /// for the first GPU's device (the OS-native prompt fires at most on this
    /// user-initiated request — the escalation discipline forbids
    /// auto-triggering), disable stops the TUI's re-request cadence. The typed
    /// answer lands in the shared request session, which is also the sole row
    /// payload authority. A closed session never displays stale rows as live.
    pub(crate) fn toggle_gpu_engine_rows(&mut self) -> Option<PlatformEffect> {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Gpu {
            return None;
        }
        let device_id = self.gpu_engine_rows_device_id()?;
        let action = taskmanager_shell::presentation::gpu_engine_rows::present_gpu_engine_rows(
            self.shell.gpu_engine_rows_state(),
            &device_id,
            self.projection().capability_status(
                &taskmanager_platform_contract::CapabilityId::TELEMETRY_GPU_ENGINES,
            ),
        )
        .action();
        match action {
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Disable => {
                self.shell.close_gpu_engine_rows_request();
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_stopped"),
                );
                None
            }
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Enable
            | taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Reauthorize
            | taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::Recheck => {
                self.report_notice(
                    FeedbackSource::Control,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.gpu_engines_requested"),
                );
                Some(taskmanager_shell::ShellApp::request_gpu_engine_rows(
                    device_id,
                ))
            }
            taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsAction::None => None,
        }
    }

    /// The device identity for the engine-rows request: the first GPU's stable
    /// native identity from the live snapshot (the PMU helper reads the
    /// integrated engine block). `None` when no GPU exists — the toggle is an
    /// honest no-op rather than a request about nothing.
    pub(crate) fn gpu_engine_rows_device_id(&self) -> Option<DeviceId> {
        let gpu = self.projection().snapshot.as_ref()?.gpu.first()?;
        let id = gpu.device_id.trim();
        (!id.is_empty()).then(|| DeviceId::new(id.to_owned()))
    }

    /// Cycle the GPU headline chart's metric family with `g` on the
    /// Performance·GPU page (ADR-034 stage 2). The selection, its
    /// availability gate, and the fixed vocabulary order live in the shared
    /// shell contract — this only routes the key and reports the resulting
    /// family in the status bar. No-op off the page, on another device, or
    /// when the viewed GPU reports no available family.
    pub(crate) fn cycle_gpu_chart_metric(&mut self) {
        if self.page() != AppPage::Performance || self.perf_device != PerfDevice::Gpu {
            return;
        }
        let gate = taskmanager_shell::gpu_chart_metric_gate(self.viewed_gpu());
        if self.shell.cycle_gpu_chart_metric(&gate) {
            let selected = self.shell.gpu_chart_metric_selected();
            self.report_notice(
                FeedbackSource::Control,
                FeedbackSeverity::Info,
                FeedbackLifecycle::SHORT,
                t("tui.status.gpu_series").replacen("{}", t(selected.label_key()), 1),
            );
        }
    }

    /// The GPU row the shared chart-metric selection is bound to: the first
    /// device of the Performance·GPU page's snapshot (the panel's headline
    /// device — the same one the engine-rows session binds to). `None`
    /// everywhere else; the shell fold then leaves the selection untouched.
    pub(crate) fn viewed_gpu(&self) -> Option<&GpuMetrics> {
        if self.page() == AppPage::Performance && self.perf_device == PerfDevice::Gpu {
            self.projection()
                .snapshot
                .as_ref()
                .and_then(|s| s.gpu.first())
        } else {
            None
        }
    }

    /// The current snapshot, when one exists.
    #[must_use]
    pub fn snapshot(&self) -> Option<&SystemSnapshot> {
        self.projection().snapshot.as_ref()
    }

    /// Copy the selected Applications row to the terminal emulator's system
    /// clipboard as `pid<TAB>name` (OSC 52). The payload is written to
    /// `sink`; production passes the runtime's stdout, tests pass a buffer.
    /// The status line reports the copied identity or an honest failure —
    /// never a panic.
    pub fn copy_selected_process<W: std::io::Write>(&mut self, sink: &mut W) {
        let Some(process) = self.selected_detail_process() else {
            self.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("tui.status.no_row_to_copy"),
            );
            return;
        };
        let payload = format!("{}\t{}", process.pid, process.name);
        match crate::clipboard::write_clipboard(sink, &payload) {
            Ok(()) => {
                self.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    t("tui.status.clipboard_copied")
                        .replacen("{}", &process.name, 1)
                        .replacen("{}", &process.pid.to_string(), 1),
                );
            }
            Err(error) => {
                self.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    t("tui.status.clipboard_failed").replacen("{}", &error.to_string(), 1),
                );
            }
        }
    }
}

impl Default for TuiApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for TuiApp {
    type Target = taskmanager_shell::ShellApp;

    fn deref(&self) -> &Self::Target {
        &self.shell
    }
}

impl DerefMut for TuiApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shell
    }
}

#[cfg(test)]
#[path = "../tests/gui/lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/gui/identity_tests.rs"]
mod identity_tests;
