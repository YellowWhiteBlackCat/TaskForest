//! Frontend-local Alerts-page state and messages (the Iced alerts UI face).
//!
//! The shared alert domain (rule set, evaluation, delivery gate) lives in the
//! shell's [`taskmanager_application::alerts::AlertCenter`] — this module owns
//! only the Iced-side view state: the frontend-local typed route (the alerts
//! page is an Iced-local route outside the shared `AppPage` set, mirroring how
//! GPUI keeps its Containers page renderer-specific). The complete managed
//! rule list and its enabled choices remain in the shell's application-owned
//! `AlertCenter`; this frontend only submits semantic edits. The view never
//! evaluates alerts; it reads the shell's typed rule and `alert_active`
//! projections.

use taskmanager_application::{ManagedAlertRule, ManagedAlertRuleEdit, PlatformEffect};

use super::IcedApp;

/// Frontend-local Alerts-page state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlertsPageState {
    route: FrontendRoute,
}

/// The Iced root has exactly one route owner: the selected shared page, or the
/// frontend-only Alerts page. Modal/context/search ownership is orthogonal and
/// retains input precedence without rewriting this route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FrontendRoute {
    #[default]
    SharedPage,
    AlertsPage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendRouteEvent {
    OpenAlerts,
    CloseAlerts,
    SelectSharedPage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendRouteTransition {
    Unchanged,
    AlertsOpened,
    AlertsClosed,
}

impl AlertsPageState {
    #[must_use]
    pub(crate) const fn route(&self) -> FrontendRoute {
        self.route
    }

    pub(crate) fn transition(&mut self, event: FrontendRouteEvent) -> FrontendRouteTransition {
        match (self.route, event) {
            (FrontendRoute::SharedPage, FrontendRouteEvent::OpenAlerts) => {
                self.route = FrontendRoute::AlertsPage;
                FrontendRouteTransition::AlertsOpened
            }
            (FrontendRoute::AlertsPage, FrontendRouteEvent::CloseAlerts)
            | (FrontendRoute::AlertsPage, FrontendRouteEvent::SelectSharedPage) => {
                self.route = FrontendRoute::SharedPage;
                FrontendRouteTransition::AlertsClosed
            }
            (FrontendRoute::SharedPage, FrontendRouteEvent::CloseAlerts)
            | (FrontendRoute::SharedPage, FrontendRouteEvent::SelectSharedPage)
            | (FrontendRoute::AlertsPage, FrontendRouteEvent::OpenAlerts) => {
                FrontendRouteTransition::Unchanged
            }
        }
    }
}

/// The Alerts-page message vocabulary, carried by
/// [`super::Message::Alerts`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertsMessage {
    /// Open the alerts page.
    OpenPage,
    /// Close the alerts page back to the shared shell route.
    ClosePage,
    /// Toggle one managed rule by stable identity.
    ToggleRule { rule_id: String },
}

impl IcedApp {
    /// Dispatch one alerts-page message. No platform effects: enabling and
    /// disabling rules is shared pure state, not a control submission.
    pub(crate) fn update_alerts(&mut self, message: AlertsMessage) -> Option<PlatformEffect> {
        match message {
            AlertsMessage::OpenPage => {
                self.open_alerts_page();
                None
            }
            AlertsMessage::ClosePage => {
                self.close_alerts_page();
                None
            }
            AlertsMessage::ToggleRule { rule_id } => {
                self.toggle_alert_rule(rule_id);
                None
            }
        }
    }

    /// Open the alerts page without copying the canonical managed rules.
    pub(crate) fn open_alerts_page(&mut self) {
        if self.modal_open() || self.shell.search_active() {
            return;
        }
        self.close_context_menus();
        self.shell.close_service_log();
        let _ = self.alerts_page.transition(FrontendRouteEvent::OpenAlerts);
    }

    /// Close the alerts page (route returns to the shared shell page).
    pub(crate) fn close_alerts_page(&mut self) {
        let _ = self.alerts_page.transition(FrontendRouteEvent::CloseAlerts);
    }

    pub(crate) fn select_shared_page_route(&mut self) {
        let _ = self
            .alerts_page
            .transition(FrontendRouteEvent::SelectSharedPage);
    }

    /// Whether the alerts page is the active frontend route.
    pub(crate) fn alerts_page_open(&self) -> bool {
        self.alerts_page.route() == FrontendRoute::AlertsPage
    }

    /// Immutable canonical managed-rule projection rendered by the page.
    pub(crate) fn alerts_rules(&self) -> &[ManagedAlertRule] {
        self.shell.projection().alert_center.managed_rules()
    }

    /// Toggle one canonical managed rule through the semantic reducer. An
    /// missing stable target (a stale row widget after an import/removal) is
    /// an honest no-op and cannot mutate the rule that moved into its place.
    fn toggle_alert_rule(&mut self, rule_id: String) -> Option<PlatformEffect> {
        let _ = self
            .shell
            .edit_alert_rules(ManagedAlertRuleEdit::Toggle { rule_id });
        None
    }
}

// ── Data Projection Models & Pure Functions ──────────────────────────────────
// Pure data-layer fact folds live here (ARCH.md §8.1), separating observation
// reads from the Iced widget tree in `ui/alerts.rs`.

use taskmanager_application::SystemSnapshot;
use taskmanager_application::alerts::{Alert, AlertMetric, AlertRule, AlertSeverity};
use taskmanager_application::i18n::t;

/// One rendered rule row (pure seam the headless tests assert on).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlertRuleRowModel {
    pub rule_id: String,
    pub metric: AlertMetric,
    pub metric_label: String,
    pub severity: AlertSeverity,
    pub severity_label: String,
    pub threshold_text: String,
    /// The typed current value ("37.4%") for system-wide metrics, or the
    /// rule's scope label (target disk / all disks) for disk-family metrics;
    /// the localized `None` when the metric is unobserved.
    pub current_text: String,
    pub enabled: bool,
}

/// One active-alert banner line (pure seam).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveAlertLine {
    pub severity: AlertSeverity,
    pub text: String,
}

/// The localized empty-state copy (pure seam).
pub(crate) fn empty_state_text() -> &'static str {
    t("alerts.empty")
}

/// Project the managed rules into display rows.
pub(crate) fn rule_rows(app: &crate::IcedApp) -> Vec<AlertRuleRowModel> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    app.alerts_rules()
        .iter()
        .map(|managed| rule_row_model(managed, snapshot))
        .collect()
}

fn rule_row_model(
    managed: &ManagedAlertRule,
    snapshot: Option<&SystemSnapshot>,
) -> AlertRuleRowModel {
    let rule = &managed.rule;
    let unit = metric_unit(rule.metric);
    AlertRuleRowModel {
        rule_id: rule.id.clone(),
        metric: rule.metric,
        metric_label: metric_label(rule.metric).to_owned(),
        severity: rule.severity,
        severity_label: severity_label(rule.severity).to_owned(),
        threshold_text: format!("{:.1}{unit}", rule.threshold),
        current_text: current_value_text(rule, snapshot),
        enabled: managed.enabled,
    }
}

/// The honest current value or status for one rule. System-wide metrics read
/// the typed scalar accessors (an absent observation renders the localized
/// `None`, never zero); disk-family metrics show the rule's scope because a
/// per-disk value would be ambiguous across several disks.
fn current_value_text(rule: &AlertRule, snapshot: Option<&SystemSnapshot>) -> String {
    if disk_scoped(rule.metric) {
        return rule
            .target
            .as_deref()
            .map_or_else(|| t("alerts.all_disks").to_owned(), str::to_owned);
    }
    let observed = snapshot.and_then(|snapshot| match rule.metric {
        AlertMetric::CpuUsagePercent => snapshot.cpu.current_global_usage_pct(),
        AlertMetric::MemoryUsagePercent => snapshot.memory.used_percentage_observed(),
        AlertMetric::DiskTemperatureC
        | AlertMetric::SmartPercentUsed
        | AlertMetric::SmartCriticalWarning => None,
    });
    observed.map_or_else(
        || t("common.none").to_owned(),
        |value| format!("{value:.1}{}", metric_unit(rule.metric)),
    )
}

/// Project the shell's active-alert mirror into banner lines.
pub(crate) fn active_alert_lines(app: &crate::IcedApp) -> Vec<ActiveAlertLine> {
    app.shell
        .projection()
        .alert_active
        .iter()
        .map(|alert| ActiveAlertLine {
            severity: alert.severity,
            text: active_line_text(alert),
        })
        .collect()
}

fn active_line_text(alert: &Alert) -> String {
    let unit = metric_unit(alert.metric);
    let base = format!(
        "{} {:.1}{unit} ≥ {:.1}{unit} · {}",
        metric_label(alert.metric),
        alert.value,
        alert.threshold,
        severity_label(alert.severity),
    );
    if disk_scoped(alert.metric) && !alert.target.trim().is_empty() {
        format!("{} · {}", alert.target, base)
    } else {
        base
    }
}

const fn disk_scoped(metric: AlertMetric) -> bool {
    matches!(
        metric,
        AlertMetric::DiskTemperatureC
            | AlertMetric::SmartPercentUsed
            | AlertMetric::SmartCriticalWarning
    )
}

/// Metric label through the shared `alerts.metric_*` catalog keys (the same
/// vocabulary the TUI alerts surface and threshold-suggestions overlay use).
fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => t("alerts.metric_cpu"),
        AlertMetric::MemoryUsagePercent => t("alerts.metric_memory"),
        AlertMetric::DiskTemperatureC => t("alerts.metric_disk_temperature"),
        AlertMetric::SmartPercentUsed => t("alerts.metric_smart_used"),
        AlertMetric::SmartCriticalWarning => t("alerts.metric_smart_critical"),
    }
}

/// Unit suffix, mirroring the TUI alerts overlay's unit semantics.
const fn metric_unit(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => "%",
        AlertMetric::DiskTemperatureC => "°C",
        AlertMetric::SmartCriticalWarning => "",
    }
}

/// Severity label through the shared `alert.*` catalog keys (GPUI parity).
fn severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => t("alert.info"),
        AlertSeverity::Warning => t("alert.warning"),
        AlertSeverity::Critical => t("alert.critical"),
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/alerts_route_tests.rs"]
mod route_tests;
#[cfg(test)]
#[path = "../../tests/gui/app/alerts_tests.rs"]
mod tests;
