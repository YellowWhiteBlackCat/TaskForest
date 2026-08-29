//! Stateless System dashboard backed by typed, RootView-owned UI state.

use std::collections::HashSet;

mod panels;
pub use panels::{DashboardPanelOverlayProps, render_panel_overlay};
mod readouts;
pub mod saved_view_transfer;
mod view;
mod widget;
pub use view::{DashboardViewProps, render_dashboard, render_system_header};
pub use widget::{DashboardWidgetProps, render_widget};

use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::timeline::{HistoryWindow, TimelineSelection, TimelineState};
use taskmanager_application::DesktopNotificationRequest;
use taskmanager_application::i18n;
use taskmanager_application::i18n::alert_severity_label;
use taskmanager_core::core::{Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity};
use taskmanager_shell::SortDir;
use taskmanager_shell::{ProcessStatusFilter, SortCol};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SystemSection {
    #[default]
    Dashboard,
    Hardware,
    Health,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardPanel {
    AlertRules,
    Events,
    SavedViews,
}

pub type EventKind = AlertEventKind;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventFilter {
    #[default]
    All,
    Active,
    Cleared,
}

pub type NotificationEvent = AlertEvent;

#[derive(Clone, Debug, Default)]
pub struct EventCenterState {
    read_ids: HashSet<u64>,
    pub filter: EventFilter,
}

impl EventCenterState {
    pub fn unread_count(&self, events: &[NotificationEvent]) -> usize {
        events
            .iter()
            .filter(|event| !self.read_ids.contains(&event.id))
            .count()
    }

    pub fn visible_events(&self, events: &[NotificationEvent]) -> Vec<NotificationEvent> {
        events
            .iter()
            .rev()
            .filter(|event| match self.filter {
                EventFilter::All => true,
                EventFilter::Active => event.kind == EventKind::Activated,
                EventFilter::Cleared => event.kind == EventKind::Cleared,
            })
            .cloned()
            .collect()
    }

    pub fn mark_all_read(&mut self, events: &[NotificationEvent]) {
        self.read_ids.extend(events.iter().map(|event| event.id));
    }

    pub fn clear(&mut self) {
        self.read_ids.clear();
    }

    /// Deterministic event history used by the capture fixture. The caller
    /// installs this into the shared shell alert authority; this function
    /// only constructs owned domain events and keeps no local copy.
    pub fn capture_event_fixture() -> Vec<AlertEvent> {
        let warning = Alert {
            instance_id: "capture-cpu:system".into(),
            rule_id: "capture-cpu".into(),
            target: "CPU".into(),
            metric: AlertMetric::CpuUsagePercent,
            severity: AlertSeverity::Warning,
            value: 93.0,
            threshold: 90.0,
            active_since_ms: 3_590_000,
        };
        let activated = AlertEvent {
            id: 1,
            kind: EventKind::Activated,
            alert: warning.clone(),
            observed_at_ms: 3_590_000,
        };
        let mut cleared = warning;
        cleared.instance_id = "capture-memory:system".into();
        cleared.rule_id = "capture-memory".into();
        cleared.metric = AlertMetric::MemoryUsagePercent;
        cleared.target = "Memory".into();
        cleared.value = 74.0;
        vec![
            activated,
            AlertEvent {
                id: 2,
                kind: EventKind::Cleared,
                alert: cleared,
                observed_at_ms: 3_560_000,
            },
        ]
    }
}

#[derive(Clone, Debug)]
pub struct SavedViewPreset {
    pub id: u64,
    name_key: Option<&'static str>,
    custom_name: String,
    pub built_in: bool,
    pub filter: ProcessStatusFilter,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub hidden_cols: HashSet<SortCol>,
}

impl SavedViewPreset {
    fn built_in(
        id: u64,
        name_key: &'static str,
        filter: ProcessStatusFilter,
        sort_col: SortCol,
        sort_asc: bool,
    ) -> Self {
        Self {
            id,
            name_key: Some(name_key),
            custom_name: String::new(),
            built_in: true,
            filter,
            sort_col,
            sort_asc,
            hidden_cols: HashSet::new(),
        }
    }

    pub fn display_name(&self) -> String {
        self.name_key
            .map(i18n::t)
            .unwrap_or(self.custom_name.as_str())
            .to_string()
    }

    pub(crate) fn restored(
        name: String,
        filter: ProcessStatusFilter,
        sort_col: SortCol,
        sort_asc: bool,
        hidden_cols: HashSet<SortCol>,
    ) -> Self {
        Self {
            id: 0,
            name_key: None,
            custom_name: name,
            built_in: false,
            filter,
            sort_col,
            sort_asc,
            hidden_cols,
        }
    }

    pub(crate) fn is_user_saved(&self) -> bool {
        !self.built_in && self.name_key.is_none()
    }

    pub(crate) fn user_name(&self) -> Option<&str> {
        self.is_user_saved().then_some(self.custom_name.as_str())
    }

    /// Apply the preset to the live window. The hidden-column set is
    /// GPUI-local chrome state; filter + sort route through the
    /// shell-owned process-viewing reducers (the same authority the Apps
    /// page's pills and headers write).
    pub fn apply_to(&self, view: &mut RootView) {
        view.set_process_status_filter(self.filter);
        view.set_process_sort(
            self.sort_col,
            if self.sort_asc {
                SortDir::Asc
            } else {
                SortDir::Desc
            },
        );
        view.processes_state.hidden_cols = self.hidden_cols.clone();
    }
}

#[derive(Clone, Debug)]
pub struct DashboardState {
    pub section: SystemSection,
    pub history_window: HistoryWindow,
    pub history_selection: TimelineSelection,
    pub timeline: TimelineState,
    pub events: EventCenterState,
    pub saved_views: Vec<SavedViewPreset>,
    pub saved_view_transfer_feedback: Option<saved_view_transfer::SavedViewTransferFeedback>,
    next_saved_view_id: u64,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            section: SystemSection::Dashboard,
            history_window: HistoryWindow::FifteenMinutes,
            history_selection: TimelineSelection::default(),
            timeline: TimelineState::default(),
            events: EventCenterState::default(),
            saved_views: vec![
                SavedViewPreset::built_in(
                    1,
                    "saved_views.cpu_hotspots",
                    ProcessStatusFilter::All,
                    SortCol::Cpu,
                    false,
                ),
                SavedViewPreset::built_in(
                    2,
                    "saved_views.running_tree",
                    ProcessStatusFilter::Running,
                    SortCol::Cpu,
                    false,
                ),
                SavedViewPreset::built_in(
                    3,
                    "saved_views.memory_heavy",
                    ProcessStatusFilter::All,
                    SortCol::Memory,
                    false,
                ),
            ],
            saved_view_transfer_feedback: None,
            next_saved_view_id: 4,
        }
    }

    /// Snapshot the live window as a new user preset. The viewing inputs are
    /// passed by value (not as a `&RootView`) so the caller's entity update
    /// closure can read them through the shell accessors without fighting
    /// the `&mut self.dashboard` receiver borrow.
    pub fn save_current_view(
        &mut self,
        filter: ProcessStatusFilter,
        sort_col: SortCol,
        sort_asc: bool,
        hidden_cols: HashSet<SortCol>,
    ) {
        let id = self.next_saved_view_id;
        self.next_saved_view_id = self.next_saved_view_id.wrapping_add(1);
        self.saved_views.push(SavedViewPreset {
            id,
            name_key: None,
            custom_name: i18n::t("saved_views.custom_name")
                .replace("{index}", &(id - 3).to_string()),
            built_in: false,
            filter,
            sort_col,
            sort_asc,
            hidden_cols,
        });
    }

    pub fn add_capture_saved_view(&mut self) {
        if self.saved_views.iter().any(|preset| preset.id == 90_000) {
            return;
        }
        self.saved_views.push(SavedViewPreset {
            id: 90_000,
            name_key: Some("saved_views.capture_fixture"),
            custom_name: String::new(),
            built_in: false,
            filter: ProcessStatusFilter::Running,
            sort_col: SortCol::Memory,
            sort_asc: false,
            hidden_cols: HashSet::new(),
        });
    }

    /// Replace only user-created presets. Built-ins are retained exactly once;
    /// capture fixtures and any future keyed presets are not persisted.
    pub(crate) fn restore_user_saved_views(&mut self, presets: Vec<SavedViewPreset>) {
        self.saved_views.retain(|preset| preset.built_in);
        let mut next_id = self
            .saved_views
            .iter()
            .map(|preset| preset.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        for mut preset in presets {
            if !preset.is_user_saved() {
                continue;
            }
            preset.id = next_id;
            next_id = next_id.saturating_add(1);
            self.saved_views.push(preset);
        }
        self.next_saved_view_id = next_id;
    }
}

impl Default for DashboardState {
    fn default() -> Self {
        Self::new()
    }
}

impl RootView {
    /// Apply one rule edit to the shared authority and evaluate immediately
    /// when it changed. GPUI retains no durable rule or enabled-state mirror.
    pub fn edit_dashboard_alert_rules(
        &mut self,
        edit: taskmanager_application::ManagedAlertRuleEdit,
    ) -> Result<
        taskmanager_application::ManagedAlertRuleEditOutcome,
        taskmanager_core::core::alerts::AlertRuleTransferError,
    > {
        let outcome = self.shell.edit_alert_rules(edit)?;
        if !outcome.changed() {
            return Ok(outcome);
        }
        let snapshot = self.system_snapshot().clone();
        let evaluation = self.shell.evaluate_alerts(
            &snapshot,
            crate::gpui_app::root::platform_submission_time_ms(),
        );
        self.accept_alert_evaluation(evaluation);
        Ok(outcome)
    }

    /// Accept a synchronous rule-edit evaluation into the same shared store
    /// used by platform folds. This is not a second RootView alert authority:
    /// render and event history consume the revision-keyed materialization.
    pub fn accept_alert_evaluation(
        &mut self,
        evaluation: taskmanager_application::AlertEvaluation,
    ) {
        let next = evaluation.active;
        let revision = self.shell.accept_alert_evaluation(next.clone());
        self.materialize_active_alerts(revision, next);
        self.submit_alert_notifications(evaluation.notifications);
    }

    /// Submit desktop notifications for newly-fired alerts (BN-07). The
    /// decision already happened in the shared [`taskmanager_application::AlertCenter`]; the frontend
    /// only routes the requests. A `None` platform or a rejected submission
    /// is an honest no-op (delivery is best-effort by design).
    pub(crate) fn submit_alert_notifications(&mut self, requests: Vec<DesktopNotificationRequest>) {
        if requests.is_empty() {
            return;
        }
        let Some(platform) = self.platform.as_mut() else {
            return;
        };
        let now_ms = crate::gpui_app::root::platform_submission_time_ms();
        for request in requests {
            let title = format!(
                "{} — {}",
                alert_severity_label(request.severity),
                request.target
            );
            let _ = platform.submit_desktop_notification(
                DesktopNotificationRequest { title, ..request },
                now_ms,
            );
        }
    }

    fn apply_saved_view(&mut self, preset: &SavedViewPreset) {
        preset.apply_to(self);
        self.dismiss_window_surface(
            crate::gpui_app::root::WindowSurfaceKind::DashboardPanel,
            crate::gpui_app::root::WindowSurfaceDismissReason::Completed,
        );
        self.page = TopPage::Apps;
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_dashboard_tests.rs"]
mod tests;
