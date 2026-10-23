//! The cgroups/prlimit resource quota facts and their limit alerts.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextColor, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, percent, px,
};
use bevy::ui::widget::Text;

use crate::icons::icon_scene;
use crate::palette::{UiPalette, space_4, space_8, space_24, theme_color};
use crate::window::{Role, TextRole, WindowPalette};
use taskmanager_application::{
    ProcessInsightFacetState, ProjectedProcessInsights, ProjectedProcessResources, i18n::t,
    project_process_resources,
};
use taskmanager_core::core::LimitValue;
use taskmanager_shell::presentation::bytes;
use taskmanager_ui_contract::IconId;

/// The share of a finite limit at which a quota row turns into an alert.
pub(crate) const QUOTA_ALERT_THRESHOLD: f32 = 0.9;

/// One observed resource quota.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuotaFact {
    /// The label of the quota.
    pub(crate) label: String,
    /// The observed readout: the measured usage against the declared limit.
    pub(crate) value: String,
    /// The observed share of the declared limit, in 0.0..=1.0.
    ///
    /// Absent whenever no finite limit is declared or the usage was not
    /// measured: an undeclared quota never becomes a percentage.
    pub(crate) share: Option<f32>,
    /// Whether the observed share reached the alert threshold.
    pub(crate) critical: bool,
}

/// The marker of a rendered quota gauge: the quota it belongs to and whether
/// that quota reached the alert threshold.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuotaGauge(pub(crate) String, pub(crate) bool);

/// The marker of a rendered quota row.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuotaMark(pub(crate) bool);

/// The marker of a quota readout that reached the alert threshold.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuotaAlert;

/// Paint every critical quota readout in the theme's critical ink.
///
/// The window's text-role observer owns the default ink of a text node, so
/// the alert ink is maintained by its own painter — the same route the
/// table's zero-value cells take.
pub(super) fn paint_quota_alerts(
    palette: Res<WindowPalette>,
    mut texts: Query<&mut TextColor, With<QuotaAlert>>,
) {
    let ink = theme_color(palette.inner.severity.critical());
    for mut text in &mut texts {
        if text.0.to_srgba() != ink.to_srgba() {
            text.0 = ink;
        }
    }
}

/// The observed resource quotas of one process.
///
/// Every fact is driven by the observed quota itself: the share exists only
/// when a finite limit was declared and the usage was measured, so an
/// undeclared quota renders as "not declared" instead of a made-up zero.
pub(super) fn quota_facts(projection: Option<&ProjectedProcessInsights>) -> Vec<QuotaFact> {
    let Some(ProcessInsightFacetState::Current(resources)) = projection.map(|p| &p.resources)
    else {
        return Vec::new();
    };
    let projected = project_process_resources(resources);
    vec![
        memory_fact(&projected),
        cpu_fact(&projected),
        pids_fact(&projected),
    ]
}

/// The honest rendering of a limit nobody declared.
///
/// A declared-but-unlimited quota reads "Unlimited"; a quota the platform
/// never declared reads "Not declared" — the two states stay apart, and
/// neither ever becomes a percentage. A reading that could not be observed
/// is a different thing and keeps the plain "Unknown" wording.
fn undeclared_limit() -> String {
    t("proc_insights.not_declared").to_owned()
}

/// The share of a measured usage against a finite limit.
fn share_of(limit: Option<LimitValue>, usage: Option<u64>) -> Option<f32> {
    let percent = limit?.usage_percent(usage)?;
    Some((percent / 100.0).clamp(0.0, 1.0))
}

/// Whether an observed share reached the alert threshold.
fn is_critical(share: Option<f32>) -> bool {
    share.is_some_and(|share| share >= QUOTA_ALERT_THRESHOLD)
}

/// The memory quota: the observed resident charge against the cgroup limit.
fn memory_fact(projected: &ProjectedProcessResources<'_>) -> QuotaFact {
    let usage = projected.memory_usage_bytes;
    let limit = projected.memory_limit;
    let limit_text = match limit {
        Some(LimitValue::Value(limit)) => bytes(limit),
        Some(LimitValue::Unlimited) => t("proc_insights.unlimited").to_owned(),
        None => undeclared_limit(),
    };
    let usage_text = usage
        .map(bytes)
        .unwrap_or_else(|| t("proc_insights.unknown").to_owned());
    let share = share_of(limit, usage);
    let mut value = format!("{usage_text} / {limit_text}");
    if let Some(share) = share {
        value = format!("{value} ({:.0}%)", share * 100.0);
    }
    QuotaFact {
        label: t("common.memory").to_owned(),
        value,
        share,
        critical: is_critical(share),
    }
}

/// The CPU quota: the declared period share of one scheduling period.
///
/// The projection carries the declared quota but not the consumed CPU time,
/// so this row never claims a share of its own.
fn cpu_fact(projected: &ProjectedProcessResources<'_>) -> QuotaFact {
    let value = match (
        projected.cpu_time_quota_micros,
        projected.cpu_time_period_micros,
    ) {
        (Some(LimitValue::Unlimited), _) => t("proc_insights.unlimited").to_owned(),
        (Some(LimitValue::Value(quota)), Some(period)) if period > 0 => {
            format!("{:.0}%", quota as f64 / period as f64 * 100.0)
        }
        (Some(LimitValue::Value(quota)), _) => format!("{quota}µs"),
        (None, _) => undeclared_limit(),
    };
    QuotaFact {
        label: t("proc_insights.cpu_quota").to_owned(),
        value,
        share: None,
        critical: false,
    }
}

/// The process-count quota: the observed process count against the limit.
fn pids_fact(projected: &ProjectedProcessResources<'_>) -> QuotaFact {
    let count = projected.process_count;
    let limit = projected.process_limit;
    let limit_text = match limit {
        Some(LimitValue::Value(limit)) => limit.to_string(),
        Some(LimitValue::Unlimited) => t("proc_insights.unlimited").to_owned(),
        None => undeclared_limit(),
    };
    let count_text = count
        .map(|count| count.to_string())
        .unwrap_or_else(|| t("proc_insights.unknown").to_owned());
    let share = share_of(limit, count);
    let mut value = format!("{count_text} / {limit_text}");
    if let Some(share) = share {
        value = format!("{value} ({:.0}%)", share * 100.0);
    }
    QuotaFact {
        label: t("proc_insights.pids").to_owned(),
        value,
        share,
        critical: is_critical(share),
    }
}

/// The micro gauge of one observed quota.
fn gauge_scene(fact: &QuotaFact, palette: &UiPalette) -> impl Scene + use<> {
    let ink = if fact.critical {
        theme_color(palette.severity.critical())
    } else {
        palette.accent
    };
    let share = fact.share.unwrap_or(0.0);
    let critical = fact.critical;
    let quota_label = fact.label.clone();
    let fill = Box::new(bsn! {
        Node { flex_grow: share, flex_basis: px(0.0), flex_shrink: 0.0, min_width: px(0.0), height: percent(100) }
        QuotaGauge(quota_label, critical)
        BackgroundColor({ ink })
    }) as Box<dyn Scene>;
    bsn! {
        Node { width: px(space_24()), height: px(space_8()), flex_direction: FlexDirection::Row,
            overflow: Overflow::clip(), flex_shrink: 0.0,
            border_radius: BorderRadius::all(px(palette.control_radius_px)) }
        BackgroundColor({ palette.content_bg })
        Children [ @{ fill } ]
    }
}

/// One observed quota row: the label, its gauge and its readout.
fn row_scene(fact: &QuotaFact, palette: &UiPalette) -> impl Scene + use<> {
    let label = fact.label.clone();
    let value = fact.value.clone();
    let alert = if fact.critical {
        Some(icon_scene(
            IconId::TriangleAlert,
            palette,
            theme_color(palette.severity.critical()),
        ))
    } else {
        None
    };
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if let Some(alert) = alert {
        children.push(alert);
    }
    children.push(Box::new(bsn! {
        Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x() }
        Children [ Text({ label }) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ]
    }) as Box<dyn Scene>);
    children.push(Box::new(gauge_scene(fact, palette)) as Box<dyn Scene>);
    let readout: Box<dyn Scene> = if fact.critical {
        Box::new(bsn! {
            Text({ value }) TextRole(Role::Body) QuotaAlert
            TextLayout { linebreak: LineBreak::NoWrap }
        })
    } else {
        Box::new(bsn! {
            Text({ value }) TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap }
        })
    };
    children.push(readout);
    let critical = fact.critical;
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center, column_gap: px(space_8()), flex_shrink: 0.0 }
        QuotaMark(critical)
        Children [{ children }]
    }
}

/// The resource quota panel of the insights facet.
pub(super) fn quota_panel_scene(facts: &[QuotaFact], palette: &UiPalette) -> impl Scene + use<> {
    let rows: Vec<Box<dyn Scene>> = facts
        .iter()
        .map(|fact| Box::new(row_scene(fact, palette)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
            row_gap: px(space_4()), flex_shrink: 0.0 }
        Children [
            Text({ t("proc_insights.resource_limits").to_owned() }) TextRole(Role::Caption) --
            Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column,
                row_gap: px(space_4()), flex_shrink: 0.0 }
            Children [{ rows }]
        ]
    }
}
