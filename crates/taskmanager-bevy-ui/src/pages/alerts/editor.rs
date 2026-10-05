//! Mounted rule controls edit the shared authority through explicit access sets.
use super::{metric_label, metric_unit, severity_label};
use crate::app::{FrontendTrack, RouteChanged};
use crate::palette::{UiPalette, space_4, space_8};
use crate::text_selection::ClipboardPort;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};
use bevy::app::{App, PreUpdate};
use bevy::clipboard::{Clipboard, ClipboardRead};
use bevy::ecs::{
    component::Component,
    hierarchy::Children,
    observer::On,
    resource::Resource,
    system::{Commands, NonSendMut, Query, ResMut, SystemParam},
};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{Checked, FlexDirection, FlexWrap, Node, UiRect, percent, px};
use bevy::ui_widgets::{Activate, Button, Checkbox};
use std::time::Duration;
use taskmanager_application::i18n::t;
use taskmanager_application::{AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit};
use taskmanager_core::core::alerts::{
    AlertMetric, AlertRule, AlertRuleConflictPolicy, AlertRuleTransferEntry, AlertSeverity,
    export_alert_rules_json, import_alert_rules_json,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(crate) enum RuleControl {
    #[default]
    Add,
    Metric(String),
    Threshold(String, i8),
    Duration(String, i8),
    Hysteresis(String, i8),
    Severity(String),
    Target(String),
    Remove(String),
    Export,
    ImportMerge,
    ImportReplace,
}
#[derive(Resource, Default)]
pub(crate) struct PendingImport(pub(crate) Option<(ClipboardRead, AlertRuleImportMode)>);
pub(crate) fn register(app: &mut App) {
    app.init_resource::<PendingImport>()
        .add_systems(PreUpdate, poll_import);
}
#[derive(Component, Clone, Default)]
pub(crate) struct RuleButton(pub(crate) RuleControl);

#[derive(SystemParam)]
struct RuleAccess<'w, 's> {
    controls: Query<'w, 's, &'static RuleButton>,
    track: NonSendMut<'w, FrontendTrack>,
    clipboard: Option<ResMut<'w, Clipboard>>,
    output: Option<ResMut<'w, ClipboardPort>>,
    pending: ResMut<'w, PendingImport>,
    commands: Commands<'w, 's>,
}
fn report(shell: &mut ShellApp, error: impl std::fmt::Display) {
    shell.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Warning,
        FeedbackLifecycle::SHORT,
        format!("Alert rule operation failed: {error}"),
    );
}
fn activate(event: On<Activate>, mut access: RuleAccess) {
    let Ok(action) = access.controls.get(event.event().entity) else {
        return;
    };
    let action = &action.0;
    match action {
        RuleControl::Export => {
            let entries: Vec<_> = access
                .track
                .shell
                .projection()
                .alert_center
                .managed_rules()
                .iter()
                .map(AlertRuleTransferEntry::from)
                .collect();
            match export_alert_rules_json(&entries) {
                Ok(json) => {
                    if let Some(output) = &mut access.output {
                        output.request_text(json, t("alerts.manage"));
                    } else {
                        report(&mut access.track.shell, "clipboard unavailable");
                    }
                }
                Err(error) => report(&mut access.track.shell, error),
            }
            return;
        }
        RuleControl::ImportMerge | RuleControl::ImportReplace => {
            if access.pending.0.is_some() {
                return;
            }
            let mode = if *action == RuleControl::ImportReplace {
                AlertRuleImportMode::Replace
            } else {
                AlertRuleImportMode::Merge(AlertRuleConflictPolicy::ReplaceExisting)
            };
            if let Some(clipboard) = &mut access.clipboard {
                access.pending.0 = Some((clipboard.fetch_text(), mode));
            } else {
                report(&mut access.track.shell, "clipboard unavailable");
            }
            return;
        }
        _ => {}
    }
    let Some(edit) = edit(&access.track.shell, action) else {
        return;
    };
    if let Err(error) = access.track.shell.edit_alert_rules(edit) {
        report(&mut access.track.shell, error);
    }
    access.commands.trigger(RouteChanged);
}
fn edit(shell: &ShellApp, action: &RuleControl) -> Option<ManagedAlertRuleEdit> {
    let rules = shell.projection().alert_center.managed_rules();
    if *action == RuleControl::Add {
        let id = (0..=rules.len())
            .map(|index| format!("custom-cpu-{index}"))
            .find(|id| rules.iter().all(|managed| managed.rule.id != *id))?;
        return Some(ManagedAlertRuleEdit::Add(ManagedAlertRule::new(
            AlertRule::new(
                id,
                AlertMetric::CpuUsagePercent,
                AlertSeverity::Warning,
                85.0,
                Duration::from_secs(5),
                5.0,
            ),
            true,
        )));
    }
    let id = match action {
        RuleControl::Metric(id)
        | RuleControl::Threshold(id, _)
        | RuleControl::Duration(id, _)
        | RuleControl::Hysteresis(id, _)
        | RuleControl::Severity(id)
        | RuleControl::Target(id)
        | RuleControl::Remove(id) => id,
        _ => return None,
    };
    if matches!(action, RuleControl::Remove(_)) {
        return Some(ManagedAlertRuleEdit::Remove {
            rule_id: id.clone(),
        });
    }
    let mut managed = rules.iter().find(|managed| &managed.rule.id == id)?.clone();
    match action {
        RuleControl::Metric(_) => {
            let index = AlertMetric::ALL
                .iter()
                .position(|metric| *metric == managed.rule.metric)?;
            managed.rule.metric = AlertMetric::ALL
                .get((index + 1) % AlertMetric::ALL.len())
                .copied()?;
            if managed.rule.metric == AlertMetric::SmartCriticalWarning {
                managed.rule.threshold = 1.0;
                managed.rule.hysteresis = 1.0;
            }
            if matches!(
                managed.rule.metric,
                AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
            ) {
                managed.rule.target = None;
            }
        }
        RuleControl::Threshold(_, delta) => {
            managed.rule.threshold = (managed.rule.threshold + f32::from(*delta)).max(0.0)
        }
        RuleControl::Duration(_, delta) => {
            managed.rule.for_duration = if *delta < 0 {
                managed
                    .rule
                    .for_duration
                    .saturating_sub(Duration::from_secs(1))
            } else {
                managed
                    .rule
                    .for_duration
                    .saturating_add(Duration::from_secs(1))
            }
        }
        RuleControl::Hysteresis(_, delta) => {
            managed.rule.hysteresis = (managed.rule.hysteresis + f32::from(*delta)).max(0.0)
        }
        RuleControl::Severity(_) => {
            managed.rule.severity = match managed.rule.severity {
                AlertSeverity::Info => AlertSeverity::Warning,
                AlertSeverity::Warning => AlertSeverity::Critical,
                AlertSeverity::Critical => AlertSeverity::Info,
            }
        }
        RuleControl::Target(_) => {
            if matches!(
                managed.rule.metric,
                AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
            ) {
                return None;
            }
            let targets: Vec<_> = shell
                .projection()
                .snapshot
                .as_ref()
                .into_iter()
                .flat_map(|snapshot| snapshot.disks.iter())
                .map(|disk| disk.device_id.clone())
                .collect();
            managed.rule.target = match &managed.rule.target {
                None => targets.first().cloned(),
                Some(current) => targets
                    .iter()
                    .position(|id| id == current)
                    .and_then(|index| targets.get(index + 1))
                    .cloned(),
            };
        }
        _ => return None,
    }
    Some(ManagedAlertRuleEdit::Update {
        target_id: id.clone(),
        managed,
    })
}
fn poll_import(
    mut pending: ResMut<PendingImport>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Some((read, mode)) = &mut pending.0 else {
        return;
    };
    let Some(result) = read.poll_result() else {
        return;
    };
    let mode = *mode;
    pending.0 = None;
    match result {
        Ok(json) => match import_alert_rules_json(&json) {
            Ok(entries) => {
                let rules = entries.into_iter().map(ManagedAlertRule::from).collect();
                if let Err(error) = track
                    .shell
                    .edit_alert_rules(ManagedAlertRuleEdit::Import { rules, mode })
                {
                    report(&mut track.shell, error);
                }
            }
            Err(error) => report(&mut track.shell, error),
        },
        Err(error) => report(&mut track.shell, error),
    }
    commands.trigger(RouteChanged);
}
fn button(label: String, action: RuleControl, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { min_height: px(palette.control_height_px), max_width: percent(100), min_width: px(0.0), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 }
        Button RuleButton(action) ControlVisual(ControlTone::Surface, false) on(activate)
        Children [ Text(label) TextLayout { linebreak: LineBreak::WordOrCharacter } TextRole(Role::Caption) Pickable::IGNORE ]
    }
}
pub(super) fn toolbar(palette: &UiPalette) -> impl Scene + use<> {
    let controls: Vec<Box<dyn Scene>> = vec![
        Box::new(button(
            t("alerts.add_rule").into(),
            RuleControl::Add,
            palette,
        )),
        Box::new(button("Export".into(), RuleControl::Export, palette)),
        Box::new(button(
            "Import (merge)".into(),
            RuleControl::ImportMerge,
            palette,
        )),
        Box::new(button(
            "Import (replace)".into(),
            RuleControl::ImportReplace,
            palette,
        )),
    ];
    bsn! { Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_4()), row_gap: px(space_4()), flex_shrink: 0.0 } Children [ { controls } ] }
}
fn numeric(
    label: &str,
    value: String,
    down: RuleControl,
    up: RuleControl,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let label = format!("{label}: {value}");
    let minus = button("−".into(), down, palette);
    let plus = button("+".into(), up, palette);
    bsn! { Node { flex_direction: FlexDirection::Row, column_gap: px(space_4()), flex_shrink: 0.0 } Children [ Text(label) TextRole(Role::Caption) -- @{ minus } -- @{ plus } ] }
}
pub(super) fn row(managed: &ManagedAlertRule, palette: &UiPalette) -> impl Scene + use<> {
    let id = managed.rule.id.clone();
    let title = format!(
        "{} · {} · {}",
        id,
        metric_label(managed.rule.metric),
        super::managed_rule_line(managed)
    );
    let threshold = numeric(
        t("alerts.threshold"),
        format!(
            "{:.1}{}",
            managed.rule.threshold,
            metric_unit(managed.rule.metric)
        ),
        RuleControl::Threshold(id.clone(), -1),
        RuleControl::Threshold(id.clone(), 1),
        palette,
    );
    let duration = numeric(
        t("alerts.duration"),
        format!("{}s", managed.rule.for_duration.as_secs()),
        RuleControl::Duration(id.clone(), -1),
        RuleControl::Duration(id.clone(), 1),
        palette,
    );
    let hysteresis = numeric(
        t("alerts.hysteresis"),
        format!(
            "{:.1}{}",
            managed.rule.hysteresis,
            metric_unit(managed.rule.metric)
        ),
        RuleControl::Hysteresis(id.clone(), -1),
        RuleControl::Hysteresis(id.clone(), 1),
        palette,
    );
    let metric = button(
        metric_label(managed.rule.metric).into(),
        RuleControl::Metric(id.clone()),
        palette,
    );
    let severity = button(
        severity_label(managed.rule.severity).into(),
        RuleControl::Severity(id.clone()),
        palette,
    );
    let target_label = format!(
        "{}: {}",
        t("alerts.target"),
        managed.rule.target.as_deref().unwrap_or(
            if matches!(
                managed.rule.metric,
                AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
            ) {
                t("alerts.system_target")
            } else {
                t("alerts.all_disks")
            }
        )
    );
    let target = button(target_label, RuleControl::Target(id.clone()), palette);
    let remove = button("Remove".into(), RuleControl::Remove(id.clone()), palette);
    let enabled = if managed.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let toggle: Box<dyn Scene> = if managed.enabled {
        Box::new(
            bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 } Checkbox Checked super::AlertRuleToggleTarget({id.clone()}) ControlVisual(ControlTone::Surface, true) Children [ Text(enabled) TextRole(Role::Caption) Pickable::IGNORE ] },
        )
    } else {
        Box::new(
            bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 } Checkbox super::AlertRuleToggleTarget({id.clone()}) ControlVisual(ControlTone::Surface, false) Children [ Text(enabled) TextRole(Role::Caption) Pickable::IGNORE ] },
        )
    };
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: px(space_4()), padding: UiRect::all(px(space_8())), flex_shrink: 0.0 }
        Children [
            Text(title) TextLayout { linebreak: LineBreak::WordOrCharacter } TextRole(Role::Body) --
            Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_8()), row_gap: px(space_4()) }
                Children [ @{ threshold } -- @{ duration } -- @{ hysteresis } ] --
            Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_4()), row_gap: px(space_4()) }
                Children [ @{ toggle } -- @{ metric } -- @{ severity } -- @{ target } -- @{ remove } ]
        ]
    }
}
