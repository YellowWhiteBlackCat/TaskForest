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
//! itself (`page_observer`): the fold observer (`alerts_fold_observer`)
//! reacts to `crate::drain::ShellProjectionFolded` — submit queued
//! notifications, then ask the app shell's route machinery to remount the
//! mounted page so the freshly folded projection renders. Idle frames fold
//! nothing, fire nothing, and redraw nothing. The toggle observer
//! (`rule_toggle_observer`) resolves `bevy_ui_widgets` checkbox
//! activations back to their rules and applies the canonical edit. Observer
//! lifetime equals page lifetime: the route observers despawn the content
//! subtree (and with it every page observer) on each remount.
//!
//! [`ShellApp::edit_alert_rules`]: taskmanager_shell::ShellApp::edit_alert_rules

use bevy::ecs::component::Component;
use bevy::ecs::event::EventPattern;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::system::{Commands, IntoObserverSystem, NonSendMut, Query, Res};
use bevy::scene::{EntityScene, ResolveContext, ResolveSceneError, ResolvedScene, Scene, bsn};
use bevy::ui::Overflow;
use bevy::ui::prelude::{BackgroundColor, FlexDirection, Node, UiRect, Val, percent, px};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{ScrollArea, ValueChange};
pub(crate) mod editor;
use taskmanager_application::i18n::t;
use taskmanager_application::{ManagedAlertRule, ManagedAlertRuleEdit, PlatformEffect};
use taskmanager_core::core::alerts::{
    Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity,
};

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use crate::app::{FrontendTrack, PageContext, RouteChanged, SharedRuntimeHandle};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use taskmanager_shell::queue_effect;

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
pub(crate) fn page_observer<E, M, S>(system: S) -> PageObserver
where
    E: EventPattern,
    S: IntoObserverSystem<E, M> + Clone + Send + Sync + 'static,
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
        queue_effect(
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

pub(crate) fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => "CPU Usage",
        AlertMetric::MemoryUsagePercent => "Memory Usage",
        AlertMetric::DiskTemperatureC => "Disk Temperature",
        AlertMetric::SmartPercentUsed => "SMART Percent Used",
        AlertMetric::SmartCriticalWarning => "SMART Critical Warning",
    }
}

pub(crate) fn event_history_line(event: &AlertEvent) -> String {
    let unit = metric_unit(event.alert.metric);
    let kind_str = match event.kind {
        AlertEventKind::Activated => "Activated",
        AlertEventKind::Cleared => "Cleared",
    };
    format!(
        "[{kind_str}] {} · {} — {:.1}{} (thresh: {:.1}{})",
        severity_label(event.alert.severity),
        event.alert.target,
        event.alert.value,
        unit,
        event.alert.threshold,
        unit
    )
}

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
    let rule_toolbar = editor::toolbar(context.palette);
    let event_entry = crate::event_center::button(
        t("events.title").into(),
        crate::event_center::EventAction::Open,
        context.palette,
    );
    let toolbar = bsn! {Node {width:percent(100),flex_direction:FlexDirection::Column,row_gap:px(space_8()),flex_shrink:0.0} Children [@{rule_toolbar} -- @{event_entry}]};
    let events = projection.alert_center.event_history();
    let event_rows: Vec<Box<dyn Scene>> = if events.is_empty() {
        vec![Box::new(empty_events_scene()) as Box<dyn Scene>]
    } else {
        events
            .iter()
            .rev()
            .take(10)
            .map(|event| Box::new(event_row_scene(event_history_line(event))) as Box<dyn Scene>)
            .collect()
    };
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
             Text({ crate::app::Page::Alerts.title() }) TextRole(Role::Heading) --
             Text(summary) TextRole(Role::Caption) --
            @{ toolbar } --
            Node { width: percent(100), height: percent(100), min_height: px(0.0), flex_grow: 1.0, flex_shrink: 1.0, flex_direction: FlexDirection::Column, overflow: Overflow::clip() }
            ScrollArea
            Children [
            Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(space_8()) }
            Children [
             Text("Active alerts") TextRole(Role::Caption) --
            { active_rows }--
             Text("Rules") TextRole(Role::Caption) --
            { rule_rows }--
             Text("Event history") TextRole(Role::Caption) --
            { event_rows }
            ]
            ] --
            { EntityScene(page_observer(alerts_fold_observer)) }--
            { EntityScene(page_observer(rule_toggle_observer)) }
        ]
    }
}

fn empty_events_scene() -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
             Text("No recent alert events") TextRole(Role::Body)
        ]
    }
}

fn event_row_scene(line: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
             Text(line) TextRole(Role::Mono)
        ]
    }
}

fn empty_active_scene() -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
             Text("No active alerts") TextRole(Role::Body)
        ]
    }
}

fn active_row_scene(line: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: Val::Auto }
        Children [
             Text(line) TextRole(Role::Body)
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
        .map(|managed| Box::new(editor::row(managed, palette)) as Box<dyn Scene>)
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/pages/alerts.rs"]
mod tests;
