//! The owned row plan: exactly what one process-tree row renders.
//!
//! The shared projection hands the tree a borrowed row view, but the tree
//! mounts rows across frames, so a row that waits in the mount queue cannot
//! borrow the live process snapshot. This module owns the copy: every string,
//! every aggregate and every hierarchy fact a row needs is cloned out of the
//! projection once, and the mounted scene reads only owned data. The
//! projection stays the single authority for identity, order and aggregation;
//! this module never re-derives them.

use taskmanager_application::i18n::t;
use taskmanager_shell::ProcessRowId;
use taskmanager_shell::presentation::bytes;

use super::aggregate::{ProcessTreeCellKind, aggregate_metric_compact_text, aggregate_metric_text};
use super::{ProcessTreeRowView, ProcessTreeToggle};

/// How a hierarchy guide column draws its vertical connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideStroke {
    /// The ancestor level's block ended above this row: no connector.
    None,
    /// The connector stops at the row's middle: this row closes its block.
    Half,
    /// The connector runs the whole row height.
    Full,
}

/// One hierarchy guide column of a row (BEVY-125).
///
/// A column is one nesting level of the hierarchy: the vertical connector
/// that shows which ancestor owns this row, plus the elbow that connects the
/// row to its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GuideColumn {
    /// The vertical connector of this column.
    pub(crate) stroke: GuideStroke,
    /// The column carries this row's own elbow.
    pub(crate) elbow: bool,
}

/// One process-tree row, fully owned.
///
/// The plan is the mount queue's unit: it carries the render facts, the
/// hierarchy guide and the badges, so the scene adapter never reads the live
/// snapshot.
pub(crate) struct TreeRowPlan {
    /// The row's stable identity (the semantic key the a11y node carries).
    pub(crate) semantic_key: String,
    /// The nesting depth in the shared category-first hierarchy.
    pub(crate) depth: usize,
    /// Whether the row aggregates a group rather than naming one process.
    pub(crate) is_group: bool,
    /// Whether the row has children to reveal.
    pub(crate) has_children: bool,
    /// Whether the row's group is currently expanded.
    pub(crate) expanded: bool,
    /// The typed toggle the row performs, when it has one.
    pub(crate) toggle: Option<ProcessTreeToggle>,
    /// The bounded cell texts, in render order.
    pub(crate) cells: Vec<(ProcessTreeCellKind, String)>,
    /// The full summary the tooltip and the accessible node carry.
    pub(crate) summary: String,
    /// The hierarchy guide columns, one per nesting level.
    pub(crate) guide: Vec<GuideColumn>,
    /// The aggregate badge label, when the row aggregates several members.
    pub(crate) aggregate_badge: Option<String>,
    /// Whether the row names a zombie process (BEVY-129).
    pub(crate) zombie_badge: bool,
    /// The structural fingerprint: a row whose fingerprint matches keeps its
    /// entity and is refreshed in place; a changed fingerprint remounts it.
    pub(crate) shape: u64,
}

impl TreeRowPlan {
    /// The plan of one projected row.
    ///
    /// The plan copies the row's owned facts, including the hierarchy guide
    /// the projection resolved for it.
    pub(crate) fn from_row(row: &ProcessTreeRowView<'_>) -> Self {
        plan_of(row)
    }
}

/// Build the owned plans of a projected tree.
pub(crate) fn build_plans(rows: &[ProcessTreeRowView<'_>]) -> Vec<TreeRowPlan> {
    rows.iter().map(TreeRowPlan::from_row).collect()
}

/// The guide columns of every row of a projected tree.
///
/// The guide needs the whole ordered list: a vertical connector belongs to
/// the rows that share one ancestor, so the columns are resolved once, from
/// the ancestor chain of every row.
pub(crate) fn guide_columns(rows: &[ProcessTreeRowView<'_>]) -> Vec<Vec<GuideColumn>> {
    let chains = ancestor_chains(rows);
    rows.iter()
        .enumerate()
        .map(|(index, row)| guide_columns_of(row, &chains, index))
        .collect()
}

/// The ancestor chain of every row: the semantic key of the ancestor at each
/// level above it.
fn ancestor_chains(rows: &[ProcessTreeRowView<'_>]) -> Vec<Vec<String>> {
    let mut chains: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    let mut stack: Vec<String> = Vec::new();
    for row in rows {
        stack.truncate(row.depth);
        chains.push(stack.clone());
        stack.push(row.semantic_key.clone());
    }
    chains
}

/// The guide columns of one row.
///
/// The deepest column is the row's own elbow: it connects the row to its
/// parent and keeps running while a sibling follows. Every shallower column
/// is one ancestor level, whose connector survives while that ancestor's
/// block still has rows below this one.
fn guide_columns_of(
    row: &ProcessTreeRowView<'_>,
    chains: &[Vec<String>],
    index: usize,
) -> Vec<GuideColumn> {
    let depth = row.depth;
    let chain = &chains[index];
    let later = &chains[index + 1..];
    (0..depth)
        .map(|level| {
            if level + 1 == depth {
                let sibling_follows = later.iter().any(|next| next == chain);
                GuideColumn {
                    stroke: if sibling_follows {
                        GuideStroke::Full
                    } else {
                        GuideStroke::Half
                    },
                    elbow: true,
                }
            } else {
                let continues = later
                    .iter()
                    .any(|next| next.len() > level && next[level] == chain[level]);
                GuideColumn {
                    stroke: if continues {
                        GuideStroke::Full
                    } else {
                        GuideStroke::None
                    },
                    elbow: false,
                }
            }
        })
        .collect()
}

/// The owned plan of one projected row.
fn plan_of(row: &ProcessTreeRowView<'_>) -> TreeRowPlan {
    let (summary, cells) = row_facts(row);
    let toggle = row_toggle(row);
    let aggregate_badge =
        (row.item.is_none() && row.member_count > 1).then(|| format!("x{}", row.member_count));
    let zombie_badge = row
        .item
        .is_some_and(|process| process.status_kind().is_zombie());
    let shape = shape_fingerprint(
        &cells,
        row.has_children,
        &row.guide,
        aggregate_badge.is_some(),
        zombie_badge,
    );
    TreeRowPlan {
        semantic_key: row.semantic_key.clone(),
        depth: row.depth,
        is_group: row.item.is_none(),
        has_children: row.has_children,
        expanded: row.expanded,
        toggle,
        cells,
        summary,
        guide: row.guide.clone(),
        aggregate_badge,
        zombie_badge,
        shape,
    }
}

/// The bounded cells and the full summary of one projected row.
fn row_facts(row: &ProcessTreeRowView<'_>) -> (String, Vec<(ProcessTreeCellKind, String)>) {
    let name = if row.member_count > 1 || row.item.is_none() {
        format!("{} · {}", row.label, row.member_count)
    } else {
        row.label.clone()
    };
    let mut summary = name.clone();
    let mut cells: Vec<(ProcessTreeCellKind, String)> =
        vec![(ProcessTreeCellKind::Name, name.clone())];
    if let Some(cpu) = &row.cpu {
        let format_value = |value: &f32| format!("{value:.1}%");
        let compact_value = |value: &f32| format!("{value:.1}%").replace(".0%", "%");
        let full = aggregate_metric_text(cpu, format_value);
        summary.push_str(&format!(" · {} {full}", t("common.cpu")));
        cells.push((
            ProcessTreeCellKind::Cpu,
            format!(
                "{}: {}",
                t("common.cpu"),
                aggregate_metric_compact_text(cpu, format_value, compact_value)
            ),
        ));
    }
    if let Some(memory) = &row.memory {
        let format_value = |value: &u64| bytes(*value);
        let compact_value = |value: &u64| bytes(*value).replace(".0", "").replace(' ', "");
        let full = aggregate_metric_text(memory, format_value);
        summary.push_str(&format!(" · {} {full}", t("common.memory")));
        cells.push((
            ProcessTreeCellKind::Memory,
            format!(
                "{}: {}",
                t("common.memory"),
                aggregate_metric_compact_text(memory, format_value, compact_value)
            ),
        ));
    }
    (summary, cells)
}

/// The typed toggle the row performs, when it has one.
fn row_toggle(row: &ProcessTreeRowView<'_>) -> Option<ProcessTreeToggle> {
    row.key.map_or_else(
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
    )
}

/// The structural fingerprint of a row: the facts that decide whether the
/// mounted row can be refreshed in place or has to be remounted.
fn shape_fingerprint(
    cells: &[(ProcessTreeCellKind, String)],
    has_children: bool,
    guide: &[GuideColumn],
    has_aggregate_badge: bool,
    has_zombie_badge: bool,
) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(cells.len() as u64);
    for (kind, _) in cells {
        mix(*kind as u64);
    }
    mix(u64::from(has_children));
    mix(guide.len() as u64);
    for column in guide {
        mix(u64::from(column.stroke == GuideStroke::Full));
        mix(u64::from(column.stroke == GuideStroke::Half));
        mix(u64::from(column.elbow));
    }
    mix(u64::from(has_aggregate_badge));
    mix(u64::from(has_zombie_badge));
    hash
}
