//! Stateless System dashboard backed by typed, RootView-owned UI state.

use std::collections::HashSet;
use taskforest_core::core::alerts::AlertRuleTransferError;
use taskmanager_application::AlertEventSeed;
use taskmanager_application::AlertRuleEditResult;
use taskmanager_application::ManagedAlertRuleEdit;
use taskmanager_application::system_timeline::{
    SystemHistoryWindow, SystemPageSection, TimelineSelection,
};

mod panels;
pub use panels::{DashboardPanelOverlayProps, render_panel_overlay};
mod readouts;
use taskmanager_shell::saved_views::{
    SavedViewPreset, SavedViewTransferFeedback, default_built_in_presets,
};
mod view;
mod widget;
pub use view::{DashboardViewProps, render_dashboard, render_system_header};
pub use widget::{DashboardWidgetProps, render_widget};

use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::timeline::TimelineGraphCache;
use taskforest_core::core::{
    Alert, AlertEvent, AlertEventId, AlertEventKind, AlertInstanceId, AlertMetric, AlertRuleId,
    AlertSeverity, AlertTargetIdentity,
};
use taskmanager_application::DesktopNotificationRequest;
use taskmanager_application::i18n::alert_severity_label;
use taskmanager_shell::SortDir;
use taskmanager_shell::app::alert_rows::AlertRowFamily;
use taskmanager_shell::{ProcessStatusFilter, SortCol};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardPanel {
    AlertRules,
    Events,
    SavedViews,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventFilter {
    #[default]
    All,
    Active,
    Cleared,
}

#[derive(Clone, Debug, Default)]
pub struct EventCenterState {
    read_through: Option<AlertEventId>,
    pub filter: EventFilter,
}

impl EventCenterState {
    pub fn unread_count(&self, events: &[AlertEvent]) -> usize {
        events
            .iter()
            .filter(|event| {
                self.read_through
                    .is_none_or(|read_through| event.id > read_through)
            })
            .count()
    }

    pub fn visible_events(&self, events: &[AlertEvent]) -> Vec<AlertEvent> {
        events
            .iter()
            .rev()
            .filter(|event| match self.filter {
                EventFilter::All => true,
                EventFilter::Active => event.kind == AlertEventKind::Activated,
                EventFilter::Cleared => event.kind == AlertEventKind::Cleared,
            })
            .cloned()
            .collect()
    }

    pub fn mark_all_read(&mut self, events: &[AlertEvent]) {
        if let Some(max_id) = events.iter().map(|event| event.id).max() {
            self.read_through = Some(max_id);
        }
    }

    pub fn clear(&mut self) {
        self.read_through = None;
    }

    /// Deterministic event history used by the capture fixture. The caller
    /// installs this into the shared shell alert authority; this function
    /// only constructs owned event seeds and keeps no local copy.
    pub fn capture_event_fixture() -> Vec<AlertEventSeed> {
        let cpu_rule_id = AlertRuleId::new("capture-cpu");
        let warning = Alert {
            instance_id: AlertInstanceId::new(cpu_rule_id.clone(), AlertTargetIdentity::System),
            rule_id: cpu_rule_id,
            target: "CPU".into(),
            metric: AlertMetric::CpuUsagePercent,
            severity: AlertSeverity::Warning,
            value: 93.0,
            threshold: 90.0,
            active_since_ms: 3_590_000,
        };
        let activated = AlertEventSeed {
            kind: AlertEventKind::Activated,
            alert: warning.clone(),
            observed_at_ms: 3_590_000,
        };
        let mut cleared = warning;
        let memory_rule_id = AlertRuleId::new("capture-memory");
        cleared.instance_id =
            AlertInstanceId::new(memory_rule_id.clone(), AlertTargetIdentity::System);
        cleared.rule_id = memory_rule_id;
        cleared.metric = AlertMetric::MemoryUsagePercent;
        cleared.target = "Memory".into();
        cleared.value = 74.0;
        vec![
            activated,
            AlertEventSeed {
                kind: AlertEventKind::Cleared,
                alert: cleared,
                observed_at_ms: 3_560_000,
            },
        ]
    }
}

#[derive(Clone, Debug)]
pub struct DashboardState {
    pub section: SystemPageSection,
    pub history_window: SystemHistoryWindow,
    pub history_selection: TimelineSelection,
    pub history_first_metric: usize,
    pub timeline: TimelineGraphCache,
    pub events: EventCenterState,
    pub saved_views: Vec<SavedViewPreset>,
    pub saved_view_transfer_feedback: Option<SavedViewTransferFeedback>,
    pub(super) next_saved_view_id: u64,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            section: SystemPageSection::Dashboard,
            history_window: SystemHistoryWindow::FifteenMinutes,
            history_selection: TimelineSelection::default(),
            history_first_metric: 0,
            timeline: TimelineGraphCache::default(),
            events: EventCenterState::default(),
            saved_views: default_built_in_presets(),
            saved_view_transfer_feedback: None,
            next_saved_view_id: 4,
        }
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
        edit: ManagedAlertRuleEdit,
    ) -> Result<AlertRuleEditResult, AlertRuleTransferError> {
        let result = self.shell.edit_alert_rules(edit)?;
        if !result.outcome.changed() {
            return Ok(result);
        }
        let snapshot = self.system_snapshot().clone();
        let evaluation = self.shell.evaluate_alerts(
            &snapshot,
            crate::gpui_app::root::platform_submission_time_ms(),
        );
        let projection = self.shell.projection();
        let revision = projection
            .alert_row_projection_generation(AlertRowFamily::Active)
            .data_revision()
            .get();
        self.materialize_active_alerts(revision, projection.alert_center.active_alerts().to_vec());
        self.submit_alert_notifications(evaluation.notifications);
        Ok(result)
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
        self.set_process_status_filter(preset.filter);
        self.set_process_sort(
            preset.sort_col,
            if preset.sort_asc {
                SortDir::Asc
            } else {
                SortDir::Desc
            },
        );
        self.processes_state.hidden_cols = preset.hidden_cols.clone();
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
