//! Pure mapping from the toolkit-neutral [`SemanticSnapshot`] tree to an
//! [`accesskit::TreeUpdate`].
//!
//! This module is intentionally free of any platform/adapter code: it only
//! depends on the core `accesskit` types, so the mapping can be compiled and
//! unit-tested on every target (the `accesskit_unix` adapter that consumes the
//! output is Linux-only and lives in `crate::bridge`).
//!
//! Design rules obeyed by this mapping:
//!
//! * Node identity is content-addressed. [`SemanticNodeId`] strings are already
//!   stable across revisions (e.g. `"app"`, `"row:1024"`, `"row:1024:cell:cpu"`),
//!   so a deterministic hash of the string yields a stable accesskit
//!   [`NodeId`]. This lets `accesskit` diff consecutive updates even when the
//!   process list churns between revisions.
//! * Every [`SemanticRole`] maps to the closest [`accesskit::Role`]. There is no
//!   direct Graph role in accesskit; a graph is published as a read-only
//!   [`Role::Meter`] carrying the numeric range plus a spoken `value`, which is
//!   what a screen reader announces.
//! * A full node list is emitted on every update. `accesskit` diffs internally,
//!   so this is the simple, correct path and matches the accesskit_winit model.

use accesskit::{Action, Live, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use taskmanager_ui_contract::{
    SemanticAction, SemanticLiveRegion, SemanticNode, SemanticNodeId, SemanticRole,
    SemanticSnapshot,
};

/// FNV-1a offset basis (64-bit).
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a prime (64-bit).
const FNV_PRIME: u64 = 0x100000001b3;

/// Deterministic, stable [`SemanticNodeId`] → [`NodeId`] mapping.
///
/// FNV-1a is chosen because it is a fixed, well-known algorithm with no seed,
/// so the same semantic id always resolves to the same accesskit id within and
/// across revisions of a process. The snapshot guarantees unique non-empty ids,
/// so collisions are not a practical concern for this tree size.
#[must_use]
pub fn stable_node_id(id: &SemanticNodeId) -> NodeId {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in id.as_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    NodeId(hash)
}

/// Map a semantic role onto the closest accesskit role.
#[must_use]
pub fn map_role(role: SemanticRole) -> Role {
    match role {
        SemanticRole::Application => Role::Application,
        SemanticRole::Window => Role::Window,
        SemanticRole::Main => Role::Main,
        SemanticRole::Navigation => Role::Navigation,
        SemanticRole::Group => Role::Group,
        SemanticRole::Heading => Role::Heading,
        // accesskit has no dedicated static-text role; TextRun is the leaf
        // text-bearing role a screen reader reads verbatim.
        SemanticRole::StaticText => Role::TextRun,
        SemanticRole::Button => Role::Button,
        SemanticRole::Switch => Role::Switch,
        SemanticRole::CheckBox => Role::CheckBox,
        SemanticRole::Radio => Role::RadioButton,
        SemanticRole::TabList => Role::TabList,
        SemanticRole::Tab => Role::Tab,
        SemanticRole::Table => Role::Table,
        SemanticRole::Row => Role::Row,
        SemanticRole::ColumnHeader => Role::ColumnHeader,
        SemanticRole::Cell => Role::Cell,
        SemanticRole::Dialog => Role::Dialog,
        SemanticRole::AlertDialog => Role::AlertDialog,
        SemanticRole::TextField => Role::TextInput,
        SemanticRole::SearchBox => Role::SearchInput,
        SemanticRole::Link => Role::Link,
        SemanticRole::Image => Role::Image,
        SemanticRole::Meter => Role::Meter,
        SemanticRole::ProgressBar => Role::ProgressIndicator,
        SemanticRole::Slider => Role::Slider,
        // No Graph role in accesskit. Publish as a read-only Meter so the AT
        // announces the numeric range + spoken value without exposing
        // Increment/Decrement that a non-interactive graph cannot honor.
        SemanticRole::Graph => Role::Meter,
        SemanticRole::List => Role::List,
        SemanticRole::Option => Role::ListBoxOption,
        SemanticRole::Tree => Role::Tree,
        SemanticRole::TreeItem => Role::TreeItem,
        SemanticRole::Menu => Role::Menu,
        SemanticRole::MenuItem => Role::MenuItem,
    }
}

/// Map a semantic action onto an accesskit action, if a faithful equivalent
/// exists. `ReadPreviousValue`/`ReadNextValue` (graph-only) and `Dismiss` have
/// no accesskit equivalent and are dropped — the spoken value already carries
/// the announcement text, and dismissal is handled by the dialog's own controls.
#[must_use]
pub fn map_action(action: SemanticAction) -> Option<Action> {
    match action {
        SemanticAction::Focus => Some(Action::Focus),
        SemanticAction::Press | SemanticAction::Toggle | SemanticAction::Select => {
            Some(Action::Click)
        }
        SemanticAction::Expand => Some(Action::Expand),
        SemanticAction::Collapse => Some(Action::Collapse),
        SemanticAction::Increment => Some(Action::Increment),
        SemanticAction::Decrement => Some(Action::Decrement),
        SemanticAction::SetValue => Some(Action::SetValue),
        SemanticAction::Dismiss
        | SemanticAction::ReadPreviousValue
        | SemanticAction::ReadNextValue => None,
    }
}

/// Map a semantic live-region politeness to an accesskit [`Live`] marker.
#[must_use]
pub fn map_live_region(live: SemanticLiveRegion) -> Live {
    match live {
        SemanticLiveRegion::Off => Live::Off,
        SemanticLiveRegion::Polite => Live::Polite,
        SemanticLiveRegion::Assertive => Live::Assertive,
    }
}

/// Build one accesskit [`Node`] from a semantic node, mapping label/value,
/// numeric range, state flags, live region, actions, and children.
#[must_use]
pub fn build_node(node: &SemanticNode) -> Node {
    let mut mapped = Node::new(map_role(node.role()));

    // Accessible name / value / description.
    if let Some(name) = node.name() {
        mapped.set_label(name);
    }
    if let Some(value) = node.value_text() {
        mapped.set_value(value);
    }
    if let Some(description) = node.description() {
        mapped.set_description(description);
    }

    // Numeric range for meters / progress / sliders / graph.
    if let Some(numeric) = node.numeric_value() {
        mapped.set_numeric_value(numeric.current);
        mapped.set_min_numeric_value(numeric.minimum);
        mapped.set_max_numeric_value(numeric.maximum);
    }

    // State flags.
    let state = node.state();
    if state.disabled {
        mapped.set_disabled();
    }
    if state.modal {
        mapped.set_modal();
    }
    if state.busy {
        mapped.set_busy();
    }
    if state.hidden {
        mapped.set_hidden();
    }
    if state.read_only_for_at() {
        mapped.set_read_only();
    }
    if let Some(selected) = state.selected {
        mapped.set_selected(selected);
    }
    if let Some(expanded) = state.expanded {
        mapped.set_expanded(expanded);
    }
    if let Some(checked) = state.checked {
        mapped.set_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }

    // Live region (only meaningful when explicitly set).
    if !matches!(node.live_region(), SemanticLiveRegion::Off) {
        mapped.set_live(map_live_region(node.live_region()));
    }

    // Actions exposed to the AT. Focusability is implied by the Focus action
    // (accesskit 0.24 has no separate focusable property).
    for action in node.actions() {
        if let Some(accesskit_action) = map_action(action) {
            mapped.add_action(accesskit_action);
        }
    }

    // Children, mapped through the same stable id function.
    let children: Vec<NodeId> = node.children().map(stable_node_id).collect();
    if !children.is_empty() {
        mapped.set_children(children);
    }

    mapped
}

/// Resolve the node id that should carry keyboard focus for this snapshot.
///
/// Priority: an explicitly focused node → the selected process row → the root.
/// The result is always a member of the published tree, satisfying accesskit's
/// invariant that `TreeUpdate::focus` references an existing node.
#[must_use]
pub fn focused_node_id(snapshot: &SemanticSnapshot) -> NodeId {
    // Explicitly focused node wins.
    for node in snapshot.nodes() {
        if node.state().focused {
            return stable_node_id(node.id());
        }
    }
    // Otherwise the selected row (if any) is the logical focus point.
    for node in snapshot.nodes() {
        if node.role() == SemanticRole::Row && node.state().selected == Some(true) {
            return stable_node_id(node.id());
        }
    }
    stable_node_id(snapshot.root())
}

/// Translate a complete [`SemanticSnapshot`] into an accesskit [`TreeUpdate`].
///
/// The returned update always carries the full node set and a [`Tree`] header,
/// so it is valid both as the initial publication and as a subsequent refresh.
#[must_use]
pub fn snapshot_to_tree_update(snapshot: &SemanticSnapshot) -> TreeUpdate {
    let nodes: Vec<(NodeId, Node)> = snapshot
        .nodes()
        .map(|node| (stable_node_id(node.id()), build_node(node)))
        .collect();

    let root_id = stable_node_id(snapshot.root());
    let mut tree = Tree::new(root_id);
    tree.toolkit_name = Some(String::from("gpui"));
    tree.toolkit_version = Some(String::from("0.2.2"));

    TreeUpdate {
        nodes,
        tree: Some(tree),
        tree_id: TreeId::ROOT,
        focus: focused_node_id(snapshot),
    }
}

/// Best-effort reverse lookup: the [`SemanticNodeId`] string whose stable hash
/// produced `id`, if it is present in `snapshot`. Used to translate inbound
/// accesskit action targets back into semantic identity.
#[must_use]
pub fn semantic_id_for(snapshot: &SemanticSnapshot, id: NodeId) -> Option<SemanticNodeId> {
    snapshot
        .nodes()
        .find(|node| stable_node_id(node.id()) == id)
        .map(|node| node.id().clone())
}

/// Translate an inbound accesskit [`Action`] back into the closest semantic
/// action, if any. The reverse of [`map_action`] for the actions a screen
/// reader actually emits.
#[must_use]
pub fn unmap_action(action: Action) -> Option<SemanticAction> {
    match action {
        Action::Focus => Some(SemanticAction::Focus),
        Action::Click => Some(SemanticAction::Press),
        Action::Expand => Some(SemanticAction::Expand),
        Action::Collapse => Some(SemanticAction::Collapse),
        Action::Increment => Some(SemanticAction::Increment),
        Action::Decrement => Some(SemanticAction::Decrement),
        Action::SetValue => Some(SemanticAction::SetValue),
        _ => None,
    }
}

/// Extension allowing the mapping to treat a `SemanticState` as read-only for
/// an AT when the node is non-interactively informational. Kept here so the
/// state-mapping logic stays with the rest of the translation.
trait ReadOnlyForAt {
    fn read_only_for_at(&self) -> bool;
}

impl ReadOnlyForAt for taskmanager_ui_contract::SemanticState {
    fn read_only_for_at(&self) -> bool {
        // A disabled node is already covered by `set_disabled`; this is a
        // placeholder for future read-only text/field roles. Today no canonical
        // TaskForest node sets a distinct read-only bit, so we never lie.
        false
    }
}
