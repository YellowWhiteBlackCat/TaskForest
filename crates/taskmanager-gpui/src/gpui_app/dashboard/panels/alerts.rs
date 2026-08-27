//! Complete, responsive alert-rule editor controls.

use super::{metric_label, severity_label};
use gpui::{Div, Entity, ParentElement, SharedString, Styled, div, px};
use std::collections::HashSet;
use std::time::Duration;

use crate::core::metrics::DiskMetrics;
use crate::core::{AlertMetric, AlertRule, AlertSeverity};
use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens;
use crate::i18n;
use taskmanager_application::{ManagedAlertRule, ManagedAlertRuleEdit};

mod transfer;
use transfer::render_transfer_actions;

const MAX_DURATION_SECS: u64 = 3_600;

#[derive(Clone, Copy, Debug, PartialEq)]
enum RuleAdjustment {
    Threshold(f32),
    Duration(i64),
    Hysteresis(f32),
    Severity,
    Target,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AlertTargetOption {
    value: String,
    label: String,
}

fn metric_supports_target(metric: AlertMetric) -> bool {
    matches!(
        metric,
        AlertMetric::DiskTemperatureC
            | AlertMetric::SmartPercentUsed
            | AlertMetric::SmartCriticalWarning
    )
}

fn maximum_threshold(metric: AlertMetric) -> f32 {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => 100.0,
        AlertMetric::DiskTemperatureC => 200.0,
        AlertMetric::SmartCriticalWarning => 1.0,
    }
}

fn next_custom_rule_id(rules: &[ManagedAlertRule]) -> String {
    let ids: HashSet<_> = rules
        .iter()
        .map(|managed| managed.rule.id.as_str())
        .collect();
    (1_u64..)
        .map(|index| format!("custom-{index}"))
        .find(|candidate| !ids.contains(candidate.as_str()))
        .unwrap_or_else(|| "custom-fallback".to_string())
}

fn target_options(disks: &[DiskMetrics]) -> Vec<AlertTargetOption> {
    let mut seen = HashSet::new();
    disks
        .iter()
        .filter_map(|disk| {
            let value = if disk.device_id.trim().is_empty() {
                disk.name.trim()
            } else {
                disk.device_id.trim()
            };
            if value.is_empty() || !seen.insert(value.to_string()) {
                return None;
            }
            Some(AlertTargetOption {
                value: value.to_string(),
                label: if disk.name.trim().is_empty() {
                    value.to_string()
                } else {
                    disk.name.clone()
                },
            })
        })
        .collect()
}

fn cycle_target(rule: &mut AlertRule, targets: &[AlertTargetOption]) {
    if !metric_supports_target(rule.metric) || targets.is_empty() {
        rule.target = None;
        return;
    }
    rule.target = rule
        .target
        .as_ref()
        .and_then(|current| {
            targets
                .iter()
                .position(|target| target.value == *current || target.label == *current)
        })
        .and_then(|index| targets.get(index + 1))
        .or_else(|| rule.target.is_none().then(|| &targets[0]))
        .map(|target| target.value.clone());
}

fn apply_adjustment(
    rule: &mut AlertRule,
    adjustment: RuleAdjustment,
    targets: &[AlertTargetOption],
) -> bool {
    let before = rule.clone();
    match adjustment {
        RuleAdjustment::Threshold(delta) => {
            rule.threshold = (rule.threshold + delta).clamp(0.0, maximum_threshold(rule.metric));
            rule.hysteresis = rule.hysteresis.min(rule.threshold);
        }
        RuleAdjustment::Duration(delta) => {
            let seconds = if delta.is_negative() {
                rule.for_duration
                    .as_secs()
                    .saturating_sub(delta.unsigned_abs())
            } else {
                rule.for_duration
                    .as_secs()
                    .saturating_add(delta.unsigned_abs())
            };
            rule.for_duration = Duration::from_secs(seconds.min(MAX_DURATION_SECS));
        }
        RuleAdjustment::Hysteresis(delta) => {
            rule.hysteresis = (rule.hysteresis + delta).clamp(0.0, rule.threshold.max(0.0));
        }
        RuleAdjustment::Severity => {
            rule.severity = match rule.severity {
                AlertSeverity::Info => AlertSeverity::Warning,
                AlertSeverity::Warning => AlertSeverity::Critical,
                AlertSeverity::Critical => AlertSeverity::Info,
            };
        }
        RuleAdjustment::Target => cycle_target(rule, targets),
    }
    *rule != before
}

fn adjust_root(view: &mut RootView, target_id: String, adjustment: RuleAdjustment) {
    let targets = target_options(&view.system_snapshot().disks);
    let Some(mut managed) = view
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.id == target_id)
        .cloned()
    else {
        return;
    };
    if apply_adjustment(&mut managed.rule, adjustment, &targets) {
        let _ =
            view.edit_dashboard_alert_rules(ManagedAlertRuleEdit::Update { target_id, managed });
    }
}

struct AdjustmentControlProps<'a> {
    theme: &'a Theme,
    index: usize,
    rule_id: String,
    id_prefix: &'static str,
    label: &'static str,
    value: String,
    decrease: RuleAdjustment,
    increase: RuleAdjustment,
    entity: &'a Entity<RootView>,
}

fn adjustment_control(props: AdjustmentControlProps<'_>) -> Div {
    let AdjustmentControlProps {
        theme,
        index,
        rule_id,
        id_prefix,
        label,
        value,
        decrease,
        increase,
        entity,
    } = props;
    let less = entity.clone();
    let more = entity.clone();
    let less_rule_id = rule_id.clone();
    div()
        .flex_1()
        .min_w(px(142.0))
        .p(tokens::SPACE_7)
        .rounded(tokens::control_radius(theme))
        .bg(theme.card_surface())
        .child(
            div()
                .flex()
                .justify_between()
                .gap(tokens::SPACE_6)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(label)
                .child(value),
        )
        .child(
            div()
                .mt(tokens::SPACE_5)
                .flex()
                .gap(tokens::SPACE_4)
                .child(elements::pill(
                    theme,
                    (SharedString::from(format!("{id_prefix}-less")), index),
                    "−",
                    false,
                    false,
                    move |_window, cx| {
                        less.update(cx, |view, cx| {
                            adjust_root(view, less_rule_id.clone(), decrease);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    (SharedString::from(format!("{id_prefix}-more")), index),
                    "+",
                    false,
                    false,
                    move |_window, cx| {
                        more.update(cx, |view, cx| {
                            adjust_root(view, rule_id.clone(), increase);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
}

fn target_control(
    theme: &Theme,
    index: usize,
    managed: &ManagedAlertRule,
    entity: &Entity<RootView>,
) -> Div {
    let supports_target = metric_supports_target(managed.rule.metric);
    let value = if supports_target {
        managed
            .rule
            .target
            .as_ref()
            .map_or_else(|| i18n::t("alerts.all_disks").to_string(), Clone::clone)
    } else {
        i18n::t("alerts.system_target").to_string()
    };
    let mut control = div()
        .flex_1()
        .min_w(px(142.0))
        .p(tokens::SPACE_7)
        .rounded(tokens::control_radius(theme))
        .bg(theme.card_surface())
        .child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(i18n::t("alerts.target")),
        );
    if supports_target {
        let target = entity.clone();
        let rule_id = managed.rule.id.clone();
        control = control.child(elements::pill(
            theme,
            ("alert-target", index),
            &value,
            managed.rule.target.is_some(),
            false,
            move |_window, cx| {
                target.update(cx, |view, cx| {
                    adjust_root(view, rule_id.clone(), RuleAdjustment::Target);
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    } else {
        control = control.child(
            div()
                .mt(tokens::SPACE_5)
                .py(tokens::SPACE_6)
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(value),
        );
    }
    control
}

fn rule_row(
    theme: &Theme,
    index: usize,
    managed: &ManagedAlertRule,
    entity: &Entity<RootView>,
) -> Div {
    let toggle = entity.clone();
    let severity = entity.clone();
    let remove = entity.clone();
    let toggle_rule_id = managed.rule.id.clone();
    let severity_rule_id = managed.rule.id.clone();
    let remove_rule_id = managed.rule.id.clone();
    div()
        .p(tokens::SPACE_8)
        .rounded(tokens::card_radius(theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar_card_bg)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_7)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(tokens::SPACE_6)
                .child(
                    div()
                        .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                        .child(metric_label(managed.rule.metric)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(tokens::SPACE_4)
                        .child(elements::pill(
                            theme,
                            ("alert-toggle", index),
                            if managed.enabled {
                                i18n::t("common.enabled")
                            } else {
                                i18n::t("common.disabled")
                            },
                            managed.enabled,
                            false,
                            move |_window, cx| {
                                toggle.update(cx, |view, cx| {
                                    let _ = view.edit_dashboard_alert_rules(
                                        ManagedAlertRuleEdit::Toggle {
                                            rule_id: toggle_rule_id.clone(),
                                        },
                                    );
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        ))
                        .child(elements::pill(
                            theme,
                            ("alert-severity", index),
                            severity_label(managed.rule.severity),
                            false,
                            false,
                            move |_window, cx| {
                                severity.update(cx, |view, cx| {
                                    adjust_root(
                                        view,
                                        severity_rule_id.clone(),
                                        RuleAdjustment::Severity,
                                    );
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        ))
                        .child(elements::pill(
                            theme,
                            ("alert-remove", index),
                            i18n::t("common.remove"),
                            false,
                            false,
                            move |_window, cx| {
                                remove.update(cx, |view, cx| {
                                    let _ = view.edit_dashboard_alert_rules(
                                        ManagedAlertRuleEdit::Remove {
                                            rule_id: remove_rule_id.clone(),
                                        },
                                    );
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_6)
                .child(adjustment_control(AdjustmentControlProps {
                    theme,
                    index,
                    rule_id: managed.rule.id.clone(),
                    id_prefix: "alert-threshold",
                    label: i18n::t("alerts.threshold"),
                    value: format!("{:.0}", managed.rule.threshold),
                    decrease: RuleAdjustment::Threshold(-5.0),
                    increase: RuleAdjustment::Threshold(5.0),
                    entity,
                }))
                .child(adjustment_control(AdjustmentControlProps {
                    theme,
                    index,
                    rule_id: managed.rule.id.clone(),
                    id_prefix: "alert-duration",
                    label: i18n::t("alerts.duration"),
                    value: i18n::t("alerts.duration_value").replace(
                        "{seconds}",
                        &managed.rule.for_duration.as_secs().to_string(),
                    ),
                    decrease: RuleAdjustment::Duration(-5),
                    increase: RuleAdjustment::Duration(5),
                    entity,
                }))
                .child(adjustment_control(AdjustmentControlProps {
                    theme,
                    index,
                    rule_id: managed.rule.id.clone(),
                    id_prefix: "alert-hysteresis",
                    label: i18n::t("alerts.hysteresis"),
                    value: format!("{:.0}", managed.rule.hysteresis),
                    decrease: RuleAdjustment::Hysteresis(-1.0),
                    increase: RuleAdjustment::Hysteresis(1.0),
                    entity,
                }))
                .child(target_control(theme, index, managed, entity)),
        )
}

pub(super) fn render_alert_rules(
    theme: &Theme,
    rules: &[ManagedAlertRule],
    entity: Entity<RootView>,
) -> Div {
    let mut rows = div().flex().flex_col().gap(tokens::SPACE_7);
    for (index, managed) in rules.iter().enumerate() {
        rows = rows.child(rule_row(theme, index, managed, &entity));
    }
    let add = entity;
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("alerts.manager_help")),
        )
        .child(render_transfer_actions(theme, rules, add.clone()))
        .child(rows)
        .child(elements::pill(
            theme,
            "alert-add-rule",
            i18n::t("alerts.add_rule"),
            false,
            false,
            move |_window, cx| {
                add.update(cx, |view, cx| {
                    let id = next_custom_rule_id(view.projection().alert_center.managed_rules());
                    let _ = view.edit_dashboard_alert_rules(ManagedAlertRuleEdit::Add(
                        ManagedAlertRule::new(
                            AlertRule::new(
                                id,
                                AlertMetric::CpuUsagePercent,
                                AlertSeverity::Info,
                                75.0,
                                Duration::from_secs(5),
                                5.0,
                            ),
                            true,
                        ),
                    ));
                    cx.notify();
                });
            },
            |_, _, _| {},
        ))
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_dashboard_panels_alerts_tests.rs"]
mod tests;
