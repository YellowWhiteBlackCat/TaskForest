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
use bevy::ui::{
    AlignItems, BackgroundColor, BorderRadius, Checked, FlexDirection, FlexWrap, Node,
    PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui_widgets::{
    Activate, Button, Checkbox, Slider, SliderOrientation, SliderPrecision, SliderRange,
    SliderStep, SliderThumb, SliderValue, TrackClick, ValueChange,
};
use std::time::Duration;
use taskforest_core::core::alerts::{
    AlertMetric, AlertRule, AlertRuleConflictPolicy, AlertRuleId, AlertRuleTransferEntry,
    AlertSeverity, export_alert_rules_json, import_alert_rules_json,
};
use taskmanager_application::i18n::t;
use taskmanager_application::{AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(crate) enum RuleControl {
    #[default]
    Add,
    Metric(AlertRuleId),
    Threshold(AlertRuleId, i8),
    Duration(AlertRuleId, i8),
    Hysteresis(AlertRuleId, i8),
    Severity(AlertRuleId),
    Target(AlertRuleId),
    Remove(AlertRuleId),
    Export,
    ImportMerge,
    ImportReplace,
}
#[derive(Resource, Default)]
pub(crate) struct PendingImport(pub(crate) Option<(ClipboardRead, AlertRuleImportMode)>);
pub(crate) fn register(app: &mut App) {
    app.init_resource::<PendingImport>()
        .add_systems(PreUpdate, poll_import);
    // The alerts page module owns the rest of this page's systems (the
    // banner spring, the export toast, the pause overlay); the window
    // composition reaches them through this entry point.
    super::register(app);
}
#[derive(Component, Clone, Default)]
pub(crate) struct RuleButton(pub(crate) RuleControl);

/// The rule a threshold slider owns. The slider's typed value change is
/// resolved back to the rule through this marker.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct ThresholdSlider(pub(crate) Option<AlertRuleId>);

/// The row's threshold value text. The slider's observer rewrites it in
/// place so a drag reads its own value without a page remount.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct ThresholdValue(pub(crate) Option<AlertRuleId>);

/// Which part of the threshold slider a node is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThresholdKind {
    /// The filled portion of the track — its width is the live fraction.
    Fill,
    /// The thumb — its offset is the live fraction.
    Thumb,
}

/// One movable part of the threshold slider: the rule it belongs to and
/// which part it is. Both parts write the `Node` component, so they share
/// one exclusive query instead of two conflicting ones.
#[derive(Component, Clone, Debug)]
pub(crate) struct ThresholdPart(pub(crate) Option<AlertRuleId>, pub(crate) ThresholdKind);

impl Default for ThresholdPart {
    /// The bsn! template seed only: a spawned part always carries its rule.
    fn default() -> Self {
        Self(None, ThresholdKind::Fill)
    }
}

/// The continuous range one metric's threshold may take: the minimum, the
/// maximum and the step. The binary SMART warning bit has no continuous
/// range — it keeps the step buttons alone.
fn threshold_range(metric: AlertMetric) -> Option<(f32, f32, f32)> {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => Some((0.0, 100.0, 1.0)),
        AlertMetric::DiskTemperatureC => Some((0.0, 200.0, 1.0)),
        AlertMetric::SmartCriticalWarning => None,
    }
}

/// The metric's threshold ceiling. A rule may never ask for more than the
/// metric can express.
fn threshold_ceiling(metric: AlertMetric) -> f32 {
    threshold_range(metric).map_or(1.0, |(_, max, _)| max)
}

/// The canonical edit that moves one rule's threshold to `value`.
fn threshold_edit(
    shell: &ShellApp,
    rule_id: &AlertRuleId,
    value: f32,
) -> Option<ManagedAlertRuleEdit> {
    let rules = shell.projection().alert_center.managed_rules();
    let mut managed = rules
        .iter()
        .find(|managed| &managed.rule.id == rule_id)?
        .clone();
    let ceiling = threshold_ceiling(managed.rule.metric);
    managed.rule.threshold = value.clamp(0.0, ceiling);
    managed.rule.hysteresis = managed.rule.hysteresis.min(managed.rule.threshold);
    Some(ManagedAlertRuleEdit::Update {
        target_id: rule_id.clone(),
        managed,
    })
}

#[derive(SystemParam)]
struct RuleAccess<'w, 's> {
    controls: Query<'w, 's, &'static RuleButton>,
    track: NonSendMut<'w, FrontendTrack>,
    clipboard: Option<ResMut<'w, Clipboard>>,
    output: Option<ResMut<'w, ClipboardPort>>,
    toast: Option<ResMut<'w, super::toast::ExportToast>>,
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
                        let label = t("alerts.manage");
                        output.request_text(json, label);
                        // The confirmation toast is raised by the shell's
                        // clipboard success notice, never by the request.
                        if let Some(toast) = access.toast.as_mut() {
                            toast.requested(label);
                        }
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
            .find(|id| rules.iter().all(|managed| managed.rule.id.as_str() != id))?;
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
            managed.rule.threshold = (managed.rule.threshold + f32::from(*delta))
                .clamp(0.0, threshold_ceiling(managed.rule.metric));
            managed.rule.hysteresis = managed.rule.hysteresis.min(managed.rule.threshold);
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
/// The threshold slider's applier: a typed value change from the widget
/// becomes a canonical rule edit, and the row's live value, filled track and
/// thumb follow the value the shell actually accepted. The remount happens
/// only when the interaction ends, so a drag is never interrupted by the
/// page rebuilding under the pointer.
pub(super) fn threshold_slider_observer(
    change: On<ValueChange<f32>>,
    sliders: Query<&ThresholdSlider>,
    mut values: Query<(&ThresholdValue, &mut Text)>,
    mut parts: Query<(&ThresholdPart, &mut Node)>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(target) = sliders.get(change.event().source) else {
        return;
    };
    let Some(rule_id) = target.0.clone() else {
        return;
    };
    let Some(edit) = threshold_edit(&track.shell, &rule_id, change.event().value) else {
        return;
    };
    if let Err(error) = track.shell.edit_alert_rules(edit) {
        report(&mut track.shell, error);
    }
    let Some((threshold, metric)) = track
        .shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.id == rule_id)
        .map(|managed| (managed.rule.threshold, managed.rule.metric))
    else {
        return;
    };
    let text = format!("{:.1}{}", threshold, metric_unit(metric));
    for (target, mut value) in &mut values {
        if target.0.as_ref() == Some(&rule_id) {
            value.0 = text.clone();
        }
    }
    let Some((min, max, _)) = threshold_range(metric) else {
        return;
    };
    let span = max - min;
    let ratio = if span > 0.0 {
        ((threshold - min) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for (target, mut node) in &mut parts {
        if target.0.as_ref() != Some(&rule_id) {
            continue;
        }
        match target.1 {
            ThresholdKind::Fill => node.width = Val::Percent(ratio * 100.0),
            ThresholdKind::Thumb => node.left = Val::Percent(ratio * 100.0),
        }
    }
    if change.event().is_final {
        commands.trigger(RouteChanged);
    }
}

fn button(label: String, action: RuleControl, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { min_height: rem(palette.control_height_rem), max_width: percent(100), min_width: px(0.0), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 }
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

/// The continuous threshold slider.
///
/// The widget is the real `bevy_ui_widgets` slider: the track reports the
/// pointer position, the thumb is the widget's own marker, and the value
/// arrives as a typed `ValueChange<f32>` — the same event a drag emits. The
/// scene only dresses the widget: the track, the filled portion and the
/// thumb all ride the rem axis.
fn threshold_slider(managed: &ManagedAlertRule, palette: &UiPalette) -> Box<dyn Scene> {
    let Some((min, max, step)) = threshold_range(managed.rule.metric) else {
        return Box::new(bsn! { Node { width: rem(0.0), height: rem(0.0) } });
    };
    let id = managed.rule.id.clone();
    let value = managed.rule.threshold.clamp(min, max);
    let span = max - min;
    let ratio = if span > 0.0 {
        ((value - min) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ratio * 100.0;
    let track_rem = palette.control_height_rem * 4.0;
    let thumb_rem = palette.icon_rem * 0.6;
    let range = SliderRange::new(min, max);
    Box::new(bsn! {
        Node {
            width: rem(track_rem),
            height: rem(thumb_rem),
            position_type: PositionType::Relative,
            margin: UiRect::horizontal(rem(thumb_rem / 2.0)),
            flex_shrink: 0.0,
        }
        SliderValue({ value })
        SliderRange { ..range }
        SliderStep({ step })
        SliderPrecision(1)
        Slider { track_click: TrackClick::Snap, orientation: SliderOrientation::Horizontal }
        ThresholdSlider(Option::Some({ id.clone() }))
        Children [
             Node { width: percent(100), height: percent(100), border_radius: BorderRadius::all(Val::Percent(50.0)) }
             BackgroundColor({ palette.panel_fill }) --
             Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                bottom: px(0.0),
                width: percent(filled),
                height: percent(100),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
             }
             BackgroundColor({ palette.accent })
             ThresholdPart(Option::Some({ id.clone() }), ThresholdKind::Fill) --
             Node {
                position_type: PositionType::Absolute,
                left: percent(filled),
                top: percent(50.0),
                width: rem(thumb_rem),
                height: rem(thumb_rem),
                margin: UiRect::left(rem(-thumb_rem / 2.0)),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
             }
             BackgroundColor({ palette.accent })
             SliderThumb
             ThresholdPart(Option::Some({ id.clone() }), ThresholdKind::Thumb)
        ]
    })
}

/// The threshold control: the label, the live value, the continuous slider
/// and the step buttons. The slider is the affordance the step buttons
/// cannot give — a held drag walks the threshold continuously.
fn threshold_control(managed: &ManagedAlertRule, palette: &UiPalette) -> Box<dyn Scene> {
    let id = managed.rule.id.clone();
    let value = format!(
        "{:.1}{}",
        managed.rule.threshold,
        metric_unit(managed.rule.metric)
    );
    let minus = button("−".into(), RuleControl::Threshold(id.clone(), -1), palette);
    let plus = button("+".into(), RuleControl::Threshold(id.clone(), 1), palette);
    let slider = threshold_slider(managed, palette);
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(space_4()),
            flex_shrink: 0.0,
        }
        Children [
             Text({ t("alerts.threshold").to_owned() }) TextRole(Role::Caption) --
             Text(value) TextRole(Role::Mono) ThresholdValue(Option::Some({ id.clone() })) --
             @{ slider } --
             @{ minus } -- @{ plus }
        ]
    })
}
pub(super) fn row(managed: &ManagedAlertRule, palette: &UiPalette) -> impl Scene + use<> {
    let id = managed.rule.id.clone();
    let title = format!(
        "{} · {} · {}",
        id,
        metric_label(managed.rule.metric),
        super::managed_rule_line(managed)
    );
    let threshold = threshold_control(managed, palette);
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
            bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 } Checkbox Checked super::AlertRuleToggleTarget(Option::Some({id.clone()})) ControlVisual(ControlTone::Surface, true) Children [ Text(enabled) TextRole(Role::Caption) Pickable::IGNORE ] },
        )
    } else {
        Box::new(
            bsn! { Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 } Checkbox super::AlertRuleToggleTarget(Option::Some({id.clone()})) ControlVisual(ControlTone::Surface, false) Children [ Text(enabled) TextRole(Role::Caption) Pickable::IGNORE ] },
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
