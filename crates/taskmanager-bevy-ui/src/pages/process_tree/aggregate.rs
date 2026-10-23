//! Bounded aggregate row cells and their full tooltip/accessibility summary.

use bevy::a11y::AccessibilityNode;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{AlignItems, AlignSelf, FlexDirection, Node, Overflow, percent, px, rem};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_shell::presentation::control_error_detail;

use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::palette::{UiPalette, space_8};
use crate::tooltip::TooltipText;
use crate::window::{Role, TextRole};

use super::badge::{TreeAggregateValue, aggregate_badge_scene, zombie_badge_scene};
use super::chevron::chevron_scene;
use super::guide::guide_scene;
use super::hover::hover_frame_scene;
use super::mount::TreeRowEntity;
use super::plan::TreeRowPlan;
use super::{ProcessTreeRowMarker, ProcessTreeRowView, on_tree_row_activated};

/// Bounded name and metric cells keep the tree row on one visual line; the
/// row's tooltip and AccessKit node carry the complete aggregate summary.
pub(super) fn row_scene(row: &ProcessTreeRowView<'_>, palette: &UiPalette) -> impl Scene + use<> {
    plan_scene(&TreeRowPlan::from_row(row), palette)
}

/// The production row scene of one owned plan.
///
/// The row draws, from left to right: the hierarchy guide and the
/// disclosure chevron, the bounded cells, the badges, and the hover frame
/// that carries the group's outline.
pub(super) fn plan_scene(plan: &TreeRowPlan, palette: &UiPalette) -> impl Scene + use<> {
    let semantic_key = plan.semantic_key.clone();
    let mut children: Vec<Box<dyn Scene>> = vec![indent_scene(plan, palette)];
    for (kind, text) in &plan.cells {
        let aggregate_reading = plan.is_group && *kind != ProcessTreeCellKind::Name;
        children.push(Box::new(process_tree_text_cell(
            semantic_key.clone(),
            *kind,
            text.clone(),
            aggregate_reading,
        )));
    }
    if let Some(badge) = &plan.aggregate_badge {
        children.push(aggregate_badge_scene(badge.clone(), palette));
    }
    if plan.zombie_badge {
        children.push(zombie_badge_scene(palette));
    }
    children.push(hover_frame_scene());
    let semantic = SemanticAddress(stable_semantic_address("process-tree", &semantic_key));
    let accessibility_node = crate::semantic::process_row_node(&plan.summary, semantic.0.as_str());
    bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(space_8()),
            overflow: Overflow::clip_x(),
        }
        ProcessTreeRowMarker({ plan.toggle.clone() })
        TreeRowEntity {
            semantic_key: { semantic_key.clone() },
            depth: { plan.depth },
            shape: { plan.shape },
        }
        Button
        on(on_tree_row_activated)
        SemanticAddress({ semantic.0.clone() })
        TooltipText({ plan.summary.clone() })
        AccessibilityNode({ accessibility_node })
        Children [ { children } ]
    }
}

/// The indent block of a row: the hierarchy guide and the disclosure chevron.
///
/// The guide columns carry the nesting level; the chevron slot keeps the
/// labels of the siblings aligned whether or not a row can expand.
fn indent_scene(plan: &TreeRowPlan, palette: &UiPalette) -> Box<dyn Scene> {
    let mut parts: Vec<Box<dyn Scene>> = vec![Box::new(bsn! {
        Node {
            width: px(space_8()),
            height: percent(100.0),
            flex_shrink: 0.0,
            align_self: AlignSelf::Stretch,
        }
        Pickable::IGNORE
    })];
    parts.extend(guide_scene(&plan.guide, palette));
    parts.push(chevron_scene(
        &plan.semantic_key,
        plan.expanded,
        plan.has_children,
        palette,
    ));
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            align_self: AlignSelf::Stretch,
            column_gap: px(0.0),
            flex_shrink: 0.0,
        }
        Pickable::IGNORE
        Children [ { parts } ]
    })
}

/// The kinds of cell a process-tree row carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProcessTreeCellKind {
    #[default]
    Name,
    Cpu,
    Memory,
}

impl ProcessTreeCellKind {
    /// The cell's share of the row's free width.
    fn grow(self) -> f32 {
        match self {
            Self::Name => 4.0,
            Self::Cpu | Self::Memory => 3.0,
        }
    }
}

/// One bounded cell of a process-tree row.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProcessTreeTextCell(pub(super) String, pub(super) ProcessTreeCellKind);

/// One bounded cell of a process-tree row.
///
/// The aggregate readings of a group row carry the aggregate marker, so the
/// weight system can state a total stronger than one process's own reading.
fn process_tree_text_cell(
    semantic_key: String,
    kind: ProcessTreeCellKind,
    text: String,
    aggregate_reading: bool,
) -> impl Scene + use<> {
    let reading: Box<dyn Scene> = if aggregate_reading {
        Box::new(bsn! {
            Text({ text })
            TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap }
            TreeAggregateValue
            Pickable::IGNORE
        })
    } else {
        Box::new(bsn! {
            Text({ text })
            TextRole(Role::Body)
            TextLayout { linebreak: LineBreak::NoWrap }
            Pickable::IGNORE
        })
    };
    bsn! {
        Node {
            min_width: px(0.0),
            flex_basis: px(0.0),
            flex_grow: { kind.grow() },
            flex_shrink: 1.0,
            overflow: Overflow::clip_x(),
        }
        ProcessTreeTextCell({ semantic_key }, { kind })
        Pickable::IGNORE
        Children [ @{ reading } ]
    }
}

/// Preserve typed aggregate freshness and coverage in the full summary.
pub(super) fn aggregate_metric_text<T>(
    metric: &AggregateMetric<T>,
    format_value: impl Fn(&T) -> String,
) -> String {
    let unknown = || t("proc_insights.unknown").to_owned();
    match metric.availability() {
        ScalarAvailability::Available => metric
            .current_value()
            .map(&format_value)
            .unwrap_or_else(unknown),
        ScalarAvailability::Partial(failure) => {
            let value = metric
                .current_value()
                .map(&format_value)
                .unwrap_or_else(unknown);
            format!(
                "{value} · {}/{} · {} · {}",
                metric.current_member_count(),
                metric.member_count(),
                t("disk.usage_partial"),
                control_error_detail(failure),
            )
        }
        ScalarAvailability::Stale(failure) => {
            let value = metric
                .last_known_value()
                .map(&format_value)
                .unwrap_or_else(unknown);
            format!(
                "{value} · {}/{} · {} · {}",
                metric.known_member_count(),
                metric.member_count(),
                t("device.stale"),
                control_error_detail(failure),
            )
        }
        ScalarAvailability::Unknown => unknown(),
        ScalarAvailability::Unavailable(failure) => format!(
            "{} · {}",
            t("proc_insights.unavailable"),
            control_error_detail(failure),
        ),
    }
}

/// Keep visible metric cells compact while retaining reasons in the full
/// tooltip and accessible name.
pub(super) fn aggregate_metric_compact_text<T>(
    metric: &AggregateMetric<T>,
    format_value: impl Fn(&T) -> String,
    compact_value: impl Fn(&T) -> String,
) -> String {
    let unknown = || t("proc_insights.unknown").to_owned();
    match metric.availability() {
        ScalarAvailability::Available => metric
            .current_value()
            .map(&format_value)
            .unwrap_or_else(unknown),
        ScalarAvailability::Partial(_) => {
            let value = metric
                .current_value()
                .map(&compact_value)
                .unwrap_or_else(unknown);
            format!(
                "{value}P{}/{}",
                metric.current_member_count(),
                metric.member_count()
            )
        }
        ScalarAvailability::Stale(_) => {
            let value = metric
                .last_known_value()
                .map(&compact_value)
                .unwrap_or_else(unknown);
            format!(
                "{value}S{}/{}",
                metric.known_member_count(),
                metric.member_count()
            )
        }
        ScalarAvailability::Unknown => unknown(),
        ScalarAvailability::Unavailable(_) => t("proc_insights.unavailable").to_owned(),
    }
}
