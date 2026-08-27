//! Application-facing builder for the canonical [`SemanticSnapshot`].
//!
//! Frontends that render TaskForest's process table, CPU graph, and status
//! announcements share one stable semantic shape. This builder turns
//! application-level inputs (process rows, a telemetry summary, an optional
//! live-region status line) into a validated snapshot that any
//! [`AccessibilityBridge`] may publish. The builder never touches a native
//! accessibility stack; it only guarantees the published tree is well-formed,
//! so a real adapter can feed the output through verbatim.
//!
//! [`AccessibilityBridge`]: super::AccessibilityBridge

use crate::{
    SemanticAction, SemanticLiveRegion, SemanticNode, SemanticNodeId, SemanticNumericValue,
    SemanticRole, SemanticSnapshot, SemanticSnapshotError, SemanticState,
};

/// One process rendered as a table row with name, CPU, and memory cells.
///
/// `id` must be unique within a single snapshot; the builder derives stable
/// semantic identifiers for the row and its three cells from it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRowInput {
    pub id: String,
    pub name: String,
    /// Current CPU percentage, when the provider supplied a trustworthy value.
    /// `None` is rendered as an unavailable semantic value, never as zero.
    pub cpu_percent: Option<f64>,
    /// Current memory percentage, when both the process value and denominator
    /// are trustworthy. `None` is rendered as an unavailable semantic value.
    pub memory_percent: Option<f64>,
    pub selected: bool,
}

/// One modal surface exposed by a frontend's semantic tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModalInput {
    /// Stable identity within the semantic snapshot, without the `modal:` prefix.
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// One managed alert rule row on a frontend's alerts-management page.
///
/// The rule is published as a focusable [`SemanticRole::Switch`] whose
/// `checked` state carries the enabled choice; `detail` is the
/// frontend-localized line (severity · threshold · current value, plus the
/// triggering flag when the rule currently fires). Filling this gap was
/// necessary because the builder's only row vocabulary
/// ([`ProcessRowInput`]) is process-shaped and cannot express a toggle row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertRuleInput {
    /// Stable identity within the semantic snapshot (the rule's id).
    pub id: String,
    /// Localized rule name (the metric label the row renders).
    pub name: String,
    /// The rule's enabled choice — the row toggle the user controls.
    pub enabled: bool,
    /// Localized row detail; `None` omits the description node text.
    pub detail: Option<String>,
}

/// Telemetry summary for the CPU (or other `0..=maximum` utilization) graph.
///
/// `current` is the value the graph lands on this revision; `peak` is the
/// high-water mark announced for screen readers but not expressed as the
/// numeric node's `current` (which must stay within `[minimum, maximum]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphSummary {
    pub current: f64,
    pub peak: f64,
    pub maximum: f64,
}

impl GraphSummary {
    /// Build the numeric range a screen reader announces for the graph value.
    #[must_use]
    pub fn to_numeric_value(self) -> SemanticNumericValue {
        SemanticNumericValue {
            current: self.current,
            minimum: 0.0,
            maximum: self.maximum,
        }
    }

    /// Frontend-neutral spoken summary, e.g. `"Latest 18%, peak 72%"`.
    #[must_use]
    pub fn to_value_text(self) -> String {
        format!("Latest {:.0}%, peak {:.0}%", self.current, self.peak)
    }
}

const ROOT_ID: &str = "app";
const MAIN_ID: &str = "main";
const TABLE_ID: &str = "process-table";
const COL_NAME_ID: &str = "col-name";
const COL_CPU_ID: &str = "col-cpu";
const COL_MEMORY_ID: &str = "col-memory";
const GRAPH_ID: &str = "cpu-graph";
const STATUS_ID: &str = "status";
const ALERTS_ID: &str = "alerts-rules";

/// Builds the canonical TaskForest semantic snapshot from application inputs.
///
/// The builder is non-destructive: invalid inputs (a graph summary whose
/// `current`/`peak` exceed `maximum`, etc.) surface as a typed
/// [`SemanticSnapshotError`] from [`SemanticSnapshotBuilder::build`] rather
/// than panicking, so a frontend can never publish a malformed tree through
/// this path.
#[derive(Clone, Debug)]
pub struct SemanticSnapshotBuilder {
    revision: u64,
    application_name: String,
    rows: Vec<ProcessRowInput>,
    graph: Option<GraphSummary>,
    status: Option<String>,
    modal: Option<ModalInput>,
    alert_rules: Option<(String, Vec<AlertRuleInput>)>,
}

impl SemanticSnapshotBuilder {
    /// Begin a snapshot at the given application revision.
    ///
    /// Revisions must be monotonic per frontend; adapters reject actions
    /// pinned to a stale revision via [`AccessibilityActionRequest`] validation.
    ///
    /// [`AccessibilityActionRequest`]: super::AccessibilityActionRequest
    #[must_use]
    pub fn new(revision: u64) -> Self {
        Self {
            revision,
            application_name: String::from("TaskForest"),
            rows: Vec::new(),
            graph: None,
            status: None,
            modal: None,
            alert_rules: None,
        }
    }

    /// Override the root application name. An empty/whitespace name is simply
    /// omitted from the root node rather than failing the build.
    #[must_use]
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = name.into();
        self
    }

    #[must_use]
    pub fn process_row(mut self, row: ProcessRowInput) -> Self {
        self.rows.push(row);
        self
    }

    #[must_use]
    pub fn process_rows(mut self, rows: impl IntoIterator<Item = ProcessRowInput>) -> Self {
        self.rows.extend(rows);
        self
    }

    /// Attach the CPU utilization graph summary.
    #[must_use]
    pub fn cpu_graph(mut self, summary: GraphSummary) -> Self {
        self.graph = Some(summary);
        self
    }

    /// Attach a polite live-region status announcement (e.g. row-count change).
    #[must_use]
    pub fn status_announcement(mut self, text: impl Into<String>) -> Self {
        self.status = Some(text.into());
        self
    }

    /// Attach the active modal surface. The builder exposes it as a modal
    /// dialog with a dismiss action; the toolkit adapter remains responsible
    /// for actual focus containment and event routing.
    #[must_use]
    pub fn modal(mut self, modal: ModalInput) -> Self {
        self.modal = Some(modal);
        self
    }

    /// Attach the alerts-management page: a named group under `main` holding
    /// one focusable switch per managed rule. Frontends call this only while
    /// their frontend-local alerts route is open, so the group's presence in a
    /// snapshot is the honest "this page is showing" fact.
    #[must_use]
    pub fn alert_rules(
        mut self,
        page_name: impl Into<String>,
        rules: impl IntoIterator<Item = AlertRuleInput>,
    ) -> Self {
        self.alert_rules = Some((page_name.into(), rules.into_iter().collect()));
        self
    }

    /// Assemble and validate the snapshot. Returning `Ok` guarantees the tree
    /// is connected, acyclic, and that every node obeys the role/state/action
    /// invariants a native adapter depends on.
    pub fn build(self) -> Result<SemanticSnapshot, SemanticSnapshotError> {
        let Self {
            revision,
            application_name,
            rows,
            graph,
            status,
            modal,
            alert_rules,
        } = self;

        let root = SemanticNodeId::borrowed(ROOT_ID);
        let main = SemanticNodeId::borrowed(MAIN_ID);
        let table = SemanticNodeId::borrowed(TABLE_ID);
        let graph_id = SemanticNodeId::borrowed(GRAPH_ID);
        let status_id = SemanticNodeId::borrowed(STATUS_ID);

        let mut nodes: Vec<SemanticNode> = Vec::new();

        // --- Process table: column headers + one row per process. ---
        let col_name = SemanticNodeId::borrowed(COL_NAME_ID);
        let col_cpu = SemanticNodeId::borrowed(COL_CPU_ID);
        let col_memory = SemanticNodeId::borrowed(COL_MEMORY_ID);
        nodes.push(SemanticNode::new(col_name.clone(), SemanticRole::ColumnHeader).named("Name"));
        nodes.push(SemanticNode::new(col_cpu.clone(), SemanticRole::ColumnHeader).named("CPU"));
        nodes.push(
            SemanticNode::new(col_memory.clone(), SemanticRole::ColumnHeader).named("Memory"),
        );
        let mut table_children: Vec<SemanticNodeId> = vec![col_name, col_cpu, col_memory];

        for row in &rows {
            let row_id = SemanticNodeId::owned(format!("row:{}", row.id));
            let cell_name = SemanticNodeId::owned(format!("row:{}:cell:name", row.id));
            let cell_cpu = SemanticNodeId::owned(format!("row:{}:cell:cpu", row.id));
            let cell_memory = SemanticNodeId::owned(format!("row:{}:cell:memory", row.id));
            table_children.push(row_id.clone());

            nodes.push(
                SemanticNode::new(row_id.clone(), SemanticRole::Row)
                    .named(row.name.clone())
                    .with_state(SemanticState {
                        focusable: true,
                        selected: Some(row.selected),
                        ..SemanticState::default()
                    })
                    .with_action(SemanticAction::Focus)
                    .with_action(SemanticAction::Select)
                    .with_children([cell_name.clone(), cell_cpu.clone(), cell_memory.clone()]),
            );
            nodes.push(
                SemanticNode::new(cell_name, SemanticRole::Cell).with_value_text(row.name.clone()),
            );
            nodes.push(
                SemanticNode::new(cell_cpu, SemanticRole::Cell)
                    .with_value_text(format_optional_percent(row.cpu_percent)),
            );
            nodes.push(
                SemanticNode::new(cell_memory, SemanticRole::Cell)
                    .with_value_text(format_optional_percent(row.memory_percent)),
            );
        }

        let mut main_children: Vec<SemanticNodeId> = vec![table.clone()];

        // --- Optional alerts-management page: a named group of rule switches. ---
        if let Some((page_name, rules)) = alert_rules {
            let group = SemanticNodeId::borrowed(ALERTS_ID);
            let mut group_children = Vec::with_capacity(rules.len());
            for rule in &rules {
                let rule_id = SemanticNodeId::owned(format!("alert-rule:{}", rule.id));
                let mut node = SemanticNode::new(rule_id.clone(), SemanticRole::Switch)
                    .named(rule.name.clone())
                    .with_state(SemanticState {
                        focusable: true,
                        checked: Some(rule.enabled),
                        ..SemanticState::default()
                    })
                    .with_action(SemanticAction::Focus)
                    .with_action(SemanticAction::Toggle);
                if let Some(detail) = &rule.detail {
                    node = node.described(detail.clone());
                }
                nodes.push(node);
                group_children.push(rule_id);
            }
            nodes.push(
                SemanticNode::new(group.clone(), SemanticRole::Group)
                    .named(page_name)
                    .with_children(group_children),
            );
            main_children.push(group);
        }

        // --- Optional CPU graph: focusable numeric + spoken peak summary. ---
        if let Some(summary) = graph {
            main_children.push(graph_id.clone());
            nodes.push(
                SemanticNode::new(graph_id.clone(), SemanticRole::Graph)
                    .named("CPU history")
                    .with_value_text(summary.to_value_text())
                    .with_numeric_value(summary.to_numeric_value())
                    .with_state(SemanticState {
                        focusable: true,
                        ..SemanticState::default()
                    })
                    .with_action(SemanticAction::Focus)
                    .with_action(SemanticAction::ReadPreviousValue)
                    .with_action(SemanticAction::ReadNextValue),
            );
        }

        nodes.push(
            SemanticNode::new(table.clone(), SemanticRole::Table)
                .named("Processes")
                .described("Sortable process list")
                .with_children(table_children),
        );

        // --- Main landmark wraps the table + graph. ---
        nodes
            .push(SemanticNode::new(main.clone(), SemanticRole::Main).with_children(main_children));

        // --- Optional polite live region for status announcements. ---
        let mut root_children: Vec<SemanticNodeId> = vec![main];
        if let Some(status_text) = status {
            nodes.push(
                SemanticNode::new(status_id.clone(), SemanticRole::StaticText)
                    .named(status_text)
                    .with_live_region(SemanticLiveRegion::Polite),
            );
            root_children.push(status_id);
        }

        // --- Optional modal surface: semantic state is toolkit-neutral. ---
        if let Some(modal) = modal {
            let modal_id = SemanticNodeId::owned(format!("modal:{}", modal.id));
            let mut node = SemanticNode::new(modal_id.clone(), SemanticRole::Dialog)
                .named(modal.name)
                .with_state(SemanticState {
                    focusable: true,
                    modal: true,
                    ..SemanticState::default()
                })
                .with_action(SemanticAction::Dismiss);
            if let Some(description) = modal.description {
                node = node.described(description);
            }
            nodes.push(node);
            root_children.push(modal_id);
        }

        // --- Root application node. ---
        let mut root_node = SemanticNode::new(root.clone(), SemanticRole::Application);
        if !application_name.trim().is_empty() {
            root_node = root_node.named(application_name);
        }
        nodes.push(root_node.with_children(root_children));

        SemanticSnapshot::new(revision, root, nodes)
    }
}

fn format_optional_percent(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "Unavailable".to_owned(), |value| format!("{value:.1}%"))
}

#[cfg(test)]
#[path = "../../tests/headless/ui_accessibility_snapshot.rs"]
mod tests;
