//! Alerts page: the active alert list, the canonical rule toggles, and the
//! desktop-notification drain.
//!
//! Data entries (per the page-agent contract in [`crate::pages`]):
//! - `context.shell.projection().alert_active` — what currently fired, with
//!   the shared threshold/hysteresis semantics already applied by the
//!   shell's evaluation fold;
//! - `context.shell.projection().alert_center.managed_rules()` — the
//!   canonical rule set; every enable/disable edit goes through
//!   [`ShellApp::edit_alert_rules`] (the one edit authority — this page
//!   never mutates the projection directly);
//! - `ShellApp::drain_alert_notifications` — desktop notifications the
//!   shared evaluation queued are submitted through `queue_effect`, the same
//!   seam every effect uses (never a direct platform call).
//!
//! **Refresh contract.** The page content is mounted once per route
//! residence; live data reaches it through observers carried by the page
//! itself ([`page_observer`]): the fold observer ([`alerts_fold_observer`])
//! reacts to [`crate::drain::ShellProjectionFolded`] — submit queued
//! notifications, then ask the app shell's route machinery to remount the
//! mounted page so the freshly folded projection renders. Idle frames fold
//! nothing, fire nothing, and redraw nothing. The toggle observer
//! ([`rule_toggle_observer`]) resolves `bevy_ui_widgets` checkbox
//! activations back to their rules and applies the canonical edit. Observer
//! lifetime equals page lifetime: the route observers despawn the content
//! subtree (and with it every page observer) on each remount.
//!
//! [`ShellApp::edit_alert_rules`]: taskmanager_shell::ShellApp::edit_alert_rules

use bevy::ecs::bundle::Bundle;
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::system::{Commands, IntoObserverSystem, NonSendMut, Query, Res};
use bevy::scene::{EntityScene, ResolveContext, ResolveSceneError, ResolvedScene, Scene, bsn};
use bevy::ui::Checked;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Checkbox, ValueChange};
use taskmanager_application::{ManagedAlertRule, ManagedAlertRuleEdit, PlatformEffect};
use taskmanager_core::core::alerts::{Alert, AlertMetric, AlertSeverity};

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use crate::app::{FrontendTrack, PageContext, RouteChanged, SharedRuntimeHandle};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};

/// A page-scoped [`Observer`] carrier: resolves to one entity carrying the
/// observer component, so the observer's lifetime equals the page content's
/// — the route observers despawn the subtree (and with it this entity) on
/// every remount. This is the page-module mechanism for "pages register
/// their own observers": shared files register app-wide observers at plugin
/// build time; pages carry theirs inside their scenes.
///
/// Hosted here (not a shared module) because the current scope adds no module
/// declarations to the shared `pages.rs`; hoisting it is a one-line move the
/// day a later milestone opens that file anyway.
pub(crate) struct PageObserver {
    build: std::sync::Arc<dyn Fn() -> Observer + Send + Sync>,
}

impl Scene for PageObserver {
    fn resolve(
        self,
        _context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        let build = self.build;
        scene.push_template(bevy::ecs::template::template(move |_| Ok(build())));
        Ok(())
    }
}

/// Bind one observer system into the page scene. The system is stored
/// behind the builder closure the scene template evaluates at spawn time.
pub(crate) fn page_observer<E, B, M, S>(system: S) -> PageObserver
where
    E: Event,
    B: Bundle,
    S: IntoObserverSystem<E, B, M> + Clone + Send + Sync + 'static,
{
    PageObserver {
        build: std::sync::Arc::new(move || Observer::new(system.clone())),
    }
}

/// Shared page plumbing (this page and the Settings page): ask the app
/// shell's route machinery to remount the mounted page, which re-reads the
/// projection through the standard mount system. Fired only from fold or
/// intent observers, so an idle frame never redraws.
pub(crate) fn request_projection_refresh(_fold: On<ShellProjectionFolded>, mut commands: Commands) {
    commands.trigger(RouteChanged);
}

/// The alerts page's fold observer: submit every desktop notification the
/// shared evaluation queued (through the one effect seam, mirroring the TUI
/// run loop), then request the projection refresh. The route resource — not
/// the event payload — stays the remount authority.
fn alerts_fold_observer(
    _fold: On<ShellProjectionFolded>,
    mut track: NonSendMut<FrontendTrack>,
    runtime: Res<SharedRuntimeHandle>,
    mut commands: Commands,
) {
    let mut client = runtime.shared.lock_client();
    for request in track.shell.drain_alert_notifications() {
        taskmanager_shell::queue_effect(
            &mut track.shell,
            &mut client,
            PlatformEffect::DesktopNotification(request),
        );
    }
    commands.trigger(RouteChanged);
}

/// Identity of one rule-toggle row: the canonical rule id, the same id
/// [`ShellApp::edit_alert_rules`] resolves. Carried by the widget entity so
/// the activation observer can resolve a `bevy_ui_widgets` checkbox event
/// back to the rule it owns.
///
/// [`ShellApp::edit_alert_rules`]: taskmanager_shell::ShellApp::edit_alert_rules
#[derive(Component, Clone, Default)]
pub(crate) struct AlertRuleToggleTarget(pub(crate) String);

/// The rule-toggle applier: a checkbox activation resolves its row's rule
/// and flips it through the shell's canonical edit entry. Guarded so a
/// repeated activation is a no-op instead of a double flip (the widget's
/// `Checked` visual marker is the widget package's business, windowed; the
/// remount below carries the fresh projection).
fn rule_toggle_observer(
    change: On<ValueChange<bool>>,
    targets: Query<&AlertRuleToggleTarget>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let requested = change.event().value;
    let Ok(target) = targets.get(change.event().source) else {
        return; // a foreign checkbox: none of this page's business
    };
    let enabled_now = track
        .shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .any(|managed| managed.rule.id == target.0 && managed.enabled);
    if enabled_now != requested {
        let edit = ManagedAlertRuleEdit::Toggle {
            rule_id: target.0.clone(),
        };
        if let Err(error) = track.shell.edit_alert_rules(edit) {
            // The Toggle edit resolves against the canonical set; a typed
            // rejection surfaces honestly instead of being swallowed.
            track.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                format!("Alert rule edit rejected: {error:?}"),
            );
            return;
        }
    }
    commands.trigger(RouteChanged);
}

// ---- view model (pure; the headless-test surface) ----

/// Unit suffix for a metric value. The binary SMART-critical metric carries
/// no unit (its value is the warning bit).
pub(crate) fn metric_unit(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => "%",
        AlertMetric::DiskTemperatureC => "°C",
        AlertMetric::SmartCriticalWarning => "",
    }
}

fn severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => "Info",
        AlertSeverity::Warning => "Warning",
        AlertSeverity::Critical => "Critical",
    }
}

/// One active-alert row, formatted per the shared evaluation semantics: an
/// active alert means `value ≥ threshold`, and it clears only at or below
/// `threshold − hysteresis` — the clear band is joined from the canonical
/// rule set by rule id. A rule that left the set omits the band honestly
/// instead of inventing one.
pub(crate) fn active_alert_line(alert: &Alert, rules: &[ManagedAlertRule]) -> String {
    let unit = metric_unit(alert.metric);
    let clear_band = match rules
        .iter()
        .find(|managed| managed.rule.id == alert.rule_id)
    {
        Some(managed) => format!(
            " (clears ≤ {:.1}{})",
            managed.rule.threshold - managed.rule.hysteresis,
            unit
        ),
        None => " (rule not in set; clear band unknown)".to_owned(),
    };
    format!(
        "{} · {} — {:.1}{} ≥ {:.1}{}{}",
        severity_label(alert.severity),
        alert.target,
        alert.value,
        unit,
        alert.threshold,
        unit,
        clear_band
    )
}

/// One canonical rule row: severity, threshold, and the participation state
/// (a disabled rule stays visible and labelled, never reads as deleted).
pub(crate) fn managed_rule_line(managed: &ManagedAlertRule) -> String {
    format!(
        "{} · ≥ {:.1}{} — {}",
        severity_label(managed.rule.severity),
        managed.rule.threshold,
        metric_unit(managed.rule.metric),
        if managed.enabled {
            "enabled"
        } else {
            "disabled"
        }
    )
}

/// The header summary line: honest counts, zero included.
pub(crate) fn alerts_summary(shell: &ShellApp) -> String {
    let projection = shell.projection();
    let rules = projection.alert_center.managed_rules();
    let enabled = rules.iter().filter(|managed| managed.enabled).count();
    format!(
        "{} active · {}/{} rules enabled",
        projection.alert_active.len(),
        enabled,
        rules.len()
    )
}

// ---- render adapters ----

/// Content-region scene for the Alerts page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let projection = context.shell.projection();
    let rules = projection.alert_center.managed_rules();
    let summary = alerts_summary(context.shell);
    let active_rows: Vec<Box<dyn Scene>> = if projection.alert_active.is_empty() {
        vec![Box::new(empty_active_scene()) as Box<dyn Scene>]
    } else {
        projection
            .alert_active
            .iter()
            .map(|alert| {
                Box::new(active_row_scene(active_alert_line(alert, rules))) as Box<dyn Scene>
            })
            .collect()
    };
    let rule_rows = rule_rows(rules, context.palette);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ context.palette.content_bg })
        Children [
            ( Text({ crate::app::Page::Alerts.title() }) TextRole(Role::Heading) ),
            ( Text(summary) TextRole(Role::Caption) ),
            ( Text("Active alerts") TextRole(Role::Caption) ),
            { active_rows },
            ( Text("Rules") TextRole(Role::Caption) ),
            { rule_rows },
            (
                Text("Notification history — the Alerts delivery log is in incubation until the notification history seam lands")
                TextRole(Role::Caption)
            ),
            { EntityScene(page_observer(alerts_fold_observer)) },
            { EntityScene(page_observer(rule_toggle_observer)) },
        ]
    }
}

fn empty_active_scene() -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
            ( Text("No active alerts") TextRole(Role::Body) ),
        ]
    }
}

fn active_row_scene(line: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
            ( Text(line) TextRole(Role::Body) ),
        ]
    }
}

/// One rule row per canonical rule: an official `Checkbox` primitive whose
/// `Checked` marker mirrors the canonical enabled state, plus the row text.
/// The two shapes (checked/unchecked) differ by one marker component, so the
/// fan-out boxes the scenes — the documented dynamic-children seam.
fn rule_rows(rules: &[ManagedAlertRule], palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    rules
        .iter()
        .map(|managed| {
            let line = managed_rule_line(managed);
            let rule_id = managed.rule.id.clone();
            if managed.enabled {
                Box::new(checked_rule_row(line, rule_id, palette)) as Box<dyn Scene>
            } else {
                Box::new(unchecked_rule_row(line, rule_id, palette)) as Box<dyn Scene>
            }
        })
        .collect()
}

fn checked_rule_row(line: String, rule_id: String, palette: &UiPalette) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            (
                Node {
                    width: px(24.0),
                    height: px(24.0),
                    border_radius: BorderRadius::all(Val::Px(radius / 4.0)),
                }
                Checkbox
                Checked
                AlertRuleToggleTarget(rule_id)
            ),
            ( Text(line) TextRole(Role::Body) ),
        ]
    }
}

fn unchecked_rule_row(line: String, rule_id: String, palette: &UiPalette) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            (
                Node {
                    width: px(24.0),
                    height: px(24.0),
                    border_radius: BorderRadius::all(Val::Px(radius / 4.0)),
                }
                Checkbox
                AlertRuleToggleTarget(rule_id)
            ),
            ( Text(line) TextRole(Role::Body) ),
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/alerts.rs"]
mod tests;
