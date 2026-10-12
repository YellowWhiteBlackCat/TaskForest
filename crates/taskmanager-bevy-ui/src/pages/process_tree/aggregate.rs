//! Bounded aggregate row cells and their full tooltip/accessibility summary.

use bevy::a11y::AccessibilityNode;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{AlignItems, FlexDirection, Node, Overflow, UiRect, Val, percent, px, rem};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_shell::ProcessRowId;
use taskmanager_shell::presentation::{bytes, control_error_detail};

use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::palette::{UiPalette, space_8};
use crate::tooltip::TooltipText;
use crate::window::{Role, TextRole};

use super::{ProcessTreeRowMarker, ProcessTreeRowView, ProcessTreeToggle, on_tree_row_activated};

/// Bounded name and metric cells keep the tree row on one visual line; the
/// row's tooltip and AccessKit node carry the complete aggregate summary.
pub(super) fn row_scene(row: &ProcessTreeRowView<'_>, palette: &UiPalette) -> impl Scene + use<> {
    let name = if row.member_count > 1 || row.item.is_none() {
        format!("{} · {}", row.label, row.member_count)
    } else {
        row.label.clone()
    };
    let mut summary = name.clone();
    let mut cells: Vec<Box<dyn Scene>> = vec![Box::new(process_tree_text_cell(
        row.semantic_key.clone(),
        ProcessTreeCellKind::Name,
        name,
        4.0,
    ))];
    if let Some(cpu) = &row.cpu {
        let format_value = |value: &f32| format!("{value:.1}%");
        let compact_value = |value: &f32| format!("{value:.1}%").replace(".0%", "%");
        let full = aggregate_metric_text(cpu, format_value);
        summary.push_str(&format!(" · {} {full}", t("common.cpu")));
        cells.push(Box::new(process_tree_text_cell(
            row.semantic_key.clone(),
            ProcessTreeCellKind::Cpu,
            format!(
                "{}: {}",
                t("common.cpu"),
                aggregate_metric_compact_text(cpu, format_value, compact_value),
            ),
            3.0,
        )));
    }
    if let Some(memory) = &row.memory {
        let format_value = |value: &u64| bytes(*value);
        let compact_value = |value: &u64| bytes(*value).replace(".0", "").replace(' ', "");
        let full = aggregate_metric_text(memory, format_value);
        summary.push_str(&format!(" · {} {full}", t("common.memory")));
        cells.push(Box::new(process_tree_text_cell(
            row.semantic_key.clone(),
            ProcessTreeCellKind::Memory,
            format!(
                "{}: {}",
                t("common.memory"),
                aggregate_metric_compact_text(memory, format_value, compact_value),
            ),
            3.0,
        )));
    }
    let left_padding = space_8() * (row.depth as f32 + 1.0);
    let semantic = SemanticAddress(stable_semantic_address("process-tree", &row.semantic_key));
    let accessibility_node = crate::semantic::process_row_node(&summary, semantic.0.as_str());
    let toggle = row.key.map_or_else(
        || {
            row.expansion_key
                .clone()
                .map(ProcessTreeToggle::UnknownGroup)
        },
        |key| match key {
            ProcessRowId::Category(category) => Some(ProcessTreeToggle::Category(category)),
            ProcessRowId::Application(identity) => Some(ProcessTreeToggle::Application(identity)),
            ProcessRowId::Process(identity) if row.has_children => {
                Some(ProcessTreeToggle::Process(identity))
            }
            ProcessRowId::Process(_) => None,
        },
    );
    bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::left(Val::Px(left_padding)),
            column_gap: px(space_8()),
            overflow: Overflow::clip_x(),
        }
        ProcessTreeRowMarker({ toggle })
        Button
        on(on_tree_row_activated)
        SemanticAddress({ semantic.0.clone() })
        TooltipText({ summary.clone() })
        AccessibilityNode({ accessibility_node })
        Children [ { cells } ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProcessTreeCellKind {
    #[default]
    Name,
    Cpu,
    Memory,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProcessTreeTextCell(pub(super) String, pub(super) ProcessTreeCellKind);

fn process_tree_text_cell(
    semantic_key: String,
    kind: ProcessTreeCellKind,
    text: String,
    flex_grow: f32,
) -> impl Scene + use<> {
    bsn! {
        Node {
            min_width: px(0.0),
            flex_basis: px(0.0),
            flex_grow,
            flex_shrink: 1.0,
            overflow: Overflow::clip_x(),
        }
        ProcessTreeTextCell({ semantic_key }, { kind })
        Pickable::IGNORE
        Children [
             Text(text) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    }
}

/// Preserve typed aggregate freshness and coverage in the full summary.
fn aggregate_metric_text<T>(
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
fn aggregate_metric_compact_text<T>(
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
