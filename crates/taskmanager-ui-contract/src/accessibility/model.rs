//! Toolkit-neutral semantic tree model: node identity, role, action, state,
//! and the deterministically validated [`SemanticSnapshot`].
//!
//! A snapshot has one reachable root, unique node identities, no cycles, and
//! role-consistent state and actions.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable identity of one semantic node within and across snapshots.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticNodeId(Cow<'static, str>);

impl SemanticNodeId {
    #[must_use]
    pub const fn borrowed(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    #[must_use]
    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for SemanticNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Semantic role independent of any frontend toolkit or native accessibility API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticRole {
    Application,
    Window,
    Navigation,
    Main,
    Group,
    Heading,
    StaticText,
    Button,
    Switch,
    CheckBox,
    Radio,
    TabList,
    Tab,
    Table,
    Row,
    ColumnHeader,
    Cell,
    Dialog,
    AlertDialog,
    TextField,
    SearchBox,
    Link,
    Image,
    Meter,
    ProgressBar,
    Slider,
    Graph,
    List,
    Option,
    Tree,
    TreeItem,
    Menu,
    MenuItem,
}

/// Action an assistive-technology adapter may request from the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SemanticAction {
    Focus,
    Press,
    Toggle,
    Select,
    Expand,
    Collapse,
    Increment,
    Decrement,
    SetValue,
    Dismiss,
    ReadPreviousValue,
    ReadNextValue,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SemanticLiveRegion {
    #[default]
    Off,
    Polite,
    Assertive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticSort {
    Ascending,
    Descending,
    Other,
}

/// State exposed to assistive technology. Optional fields distinguish
/// “not applicable” from a truthful `false` value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SemanticState {
    pub disabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub selected: Option<bool>,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub modal: bool,
    pub busy: bool,
    pub hidden: bool,
    pub sort: Option<SemanticSort>,
}

/// Numeric alternative for meters, progress indicators, sliders, and graphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticNumericValue {
    pub current: f64,
    pub minimum: f64,
    pub maximum: f64,
}

/// One localized, frontend-neutral node in the semantic tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    id: SemanticNodeId,
    role: SemanticRole,
    name: Option<String>,
    description: Option<String>,
    value_text: Option<String>,
    numeric_value: Option<SemanticNumericValue>,
    state: SemanticState,
    actions: BTreeSet<SemanticAction>,
    live_region: SemanticLiveRegion,
    children: Vec<SemanticNodeId>,
}

impl SemanticNode {
    #[must_use]
    pub fn new(id: SemanticNodeId, role: SemanticRole) -> Self {
        Self {
            id,
            role,
            name: None,
            description: None,
            value_text: None,
            numeric_value: None,
            state: SemanticState::default(),
            actions: BTreeSet::new(),
            live_region: SemanticLiveRegion::Off,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn with_value_text(mut self, value: impl Into<String>) -> Self {
        self.value_text = Some(value.into());
        self
    }

    #[must_use]
    pub const fn with_numeric_value(mut self, value: SemanticNumericValue) -> Self {
        self.numeric_value = Some(value);
        self
    }

    #[must_use]
    pub const fn with_state(mut self, state: SemanticState) -> Self {
        self.state = state;
        self
    }

    #[must_use]
    pub fn with_action(mut self, action: SemanticAction) -> Self {
        self.actions.insert(action);
        self
    }

    #[must_use]
    pub const fn with_live_region(mut self, live_region: SemanticLiveRegion) -> Self {
        self.live_region = live_region;
        self
    }

    #[must_use]
    pub fn with_child(mut self, child: SemanticNodeId) -> Self {
        self.children.push(child);
        self
    }

    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = SemanticNodeId>) -> Self {
        self.children.extend(children);
        self
    }

    #[must_use]
    pub fn id(&self) -> &SemanticNodeId {
        &self.id
    }

    #[must_use]
    pub const fn role(&self) -> SemanticRole {
        self.role
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub fn value_text(&self) -> Option<&str> {
        self.value_text.as_deref()
    }

    #[must_use]
    pub const fn numeric_value(&self) -> Option<SemanticNumericValue> {
        self.numeric_value
    }

    #[must_use]
    pub const fn state(&self) -> SemanticState {
        self.state
    }

    pub fn actions(&self) -> impl Iterator<Item = SemanticAction> + '_ {
        self.actions.iter().copied()
    }

    #[must_use]
    pub fn supports_action(&self, action: SemanticAction) -> bool {
        self.actions.contains(&action)
    }

    #[must_use]
    pub const fn live_region(&self) -> SemanticLiveRegion {
        self.live_region
    }

    pub fn children(&self) -> impl Iterator<Item = &SemanticNodeId> {
        self.children.iter()
    }
}

/// A validated, deterministic semantic tree at one frontend revision.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSnapshot {
    revision: u64,
    root: SemanticNodeId,
    nodes: BTreeMap<SemanticNodeId, SemanticNode>,
}

impl SemanticSnapshot {
    pub fn new(
        revision: u64,
        root: SemanticNodeId,
        nodes: impl IntoIterator<Item = SemanticNode>,
    ) -> Result<Self, SemanticSnapshotError> {
        let mut indexed = BTreeMap::new();
        for node in nodes {
            let id = node.id.clone();
            if indexed.insert(id.clone(), node).is_some() {
                return Err(SemanticSnapshotError::DuplicateNode(id));
            }
        }
        if !indexed.contains_key(&root) {
            return Err(SemanticSnapshotError::RootMissing(root));
        }
        if !matches!(
            indexed[&root].role,
            SemanticRole::Application | SemanticRole::Window
        ) {
            return Err(SemanticSnapshotError::InvalidRootRole(root));
        }
        validate_nodes(&root, &indexed)?;
        Ok(Self {
            revision,
            root,
            nodes: indexed,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn root(&self) -> &SemanticNodeId {
        &self.root
    }

    #[must_use]
    pub fn get(&self, id: &SemanticNodeId) -> Option<&SemanticNode> {
        self.nodes.get(id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &SemanticNode> {
        self.nodes.values()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticNodeIssue {
    EmptyId,
    EmptyName,
    MissingInteractiveName,
    FocusedWithoutFocusable,
    FocusActionWithoutFocusable,
    DisabledHasActions,
    CheckedOnUnsupportedRole,
    SelectedOnUnsupportedRole,
    ExpandedOnUnsupportedRole,
    ModalOnUnsupportedRole,
    SortOnUnsupportedRole,
    NumericValueOnUnsupportedRole,
    InvalidNumericRange,
    UnsupportedActionForRole,
    DuplicateChild,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSnapshotError {
    RootMissing(SemanticNodeId),
    InvalidRootRole(SemanticNodeId),
    DuplicateNode(SemanticNodeId),
    MissingChild {
        parent: SemanticNodeId,
        child: SemanticNodeId,
    },
    MultipleParents(SemanticNodeId),
    Cycle(SemanticNodeId),
    Disconnected(SemanticNodeId),
    InvalidNode {
        node: SemanticNodeId,
        issue: SemanticNodeIssue,
    },
}

impl fmt::Display for SemanticSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SemanticSnapshotError {}

fn validate_nodes(
    root: &SemanticNodeId,
    nodes: &BTreeMap<SemanticNodeId, SemanticNode>,
) -> Result<(), SemanticSnapshotError> {
    let mut parents = BTreeMap::<SemanticNodeId, SemanticNodeId>::new();
    for node in nodes.values() {
        validate_node(node)?;
        let mut local_children = BTreeSet::new();
        for child in &node.children {
            if !local_children.insert(child) {
                return invalid(node, SemanticNodeIssue::DuplicateChild);
            }
            if !nodes.contains_key(child) {
                return Err(SemanticSnapshotError::MissingChild {
                    parent: node.id.clone(),
                    child: child.clone(),
                });
            }
            if parents.insert(child.clone(), node.id.clone()).is_some() {
                return Err(SemanticSnapshotError::MultipleParents(child.clone()));
            }
        }
    }
    if parents.contains_key(root) {
        return Err(SemanticSnapshotError::Cycle(root.clone()));
    }

    let mut reached = BTreeSet::new();
    let mut pending = vec![root.clone()];
    while let Some(id) = pending.pop() {
        if !reached.insert(id.clone()) {
            return Err(SemanticSnapshotError::Cycle(id));
        }
        let node = &nodes[&id];
        pending.extend(node.children.iter().cloned());
    }
    if let Some(id) = nodes.keys().find(|id| !reached.contains(*id)) {
        return Err(SemanticSnapshotError::Disconnected(id.clone()));
    }
    Ok(())
}

fn validate_node(node: &SemanticNode) -> Result<(), SemanticSnapshotError> {
    if node.id.as_str().trim().is_empty() {
        return invalid(node, SemanticNodeIssue::EmptyId);
    }
    if node
        .name
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return invalid(node, SemanticNodeIssue::EmptyName);
    }
    if role_requires_name(node.role) && node.name.is_none() {
        return invalid(node, SemanticNodeIssue::MissingInteractiveName);
    }
    if node.state.focused && !node.state.focusable {
        return invalid(node, SemanticNodeIssue::FocusedWithoutFocusable);
    }
    if node.actions.contains(&SemanticAction::Focus) && !node.state.focusable {
        return invalid(node, SemanticNodeIssue::FocusActionWithoutFocusable);
    }
    if node.state.disabled && !node.actions.is_empty() {
        return invalid(node, SemanticNodeIssue::DisabledHasActions);
    }
    if node.state.checked.is_some()
        && !matches!(
            node.role,
            SemanticRole::Switch | SemanticRole::CheckBox | SemanticRole::Radio
        )
    {
        return invalid(node, SemanticNodeIssue::CheckedOnUnsupportedRole);
    }
    if node.state.selected.is_some()
        && !matches!(
            node.role,
            SemanticRole::Tab | SemanticRole::Row | SemanticRole::Option
        )
    {
        return invalid(node, SemanticNodeIssue::SelectedOnUnsupportedRole);
    }
    if node.state.expanded.is_some()
        && !matches!(
            node.role,
            SemanticRole::Button | SemanticRole::TreeItem | SemanticRole::MenuItem
        )
    {
        return invalid(node, SemanticNodeIssue::ExpandedOnUnsupportedRole);
    }
    if node.state.modal && !matches!(node.role, SemanticRole::Dialog | SemanticRole::AlertDialog) {
        return invalid(node, SemanticNodeIssue::ModalOnUnsupportedRole);
    }
    if node.state.sort.is_some() && node.role != SemanticRole::ColumnHeader {
        return invalid(node, SemanticNodeIssue::SortOnUnsupportedRole);
    }
    validate_numeric_value(node)?;
    for action in &node.actions {
        if !role_supports_action(node.role, *action) {
            return invalid(node, SemanticNodeIssue::UnsupportedActionForRole);
        }
    }
    Ok(())
}

fn validate_numeric_value(node: &SemanticNode) -> Result<(), SemanticSnapshotError> {
    let Some(value) = node.numeric_value else {
        return Ok(());
    };
    if !matches!(
        node.role,
        SemanticRole::Meter
            | SemanticRole::ProgressBar
            | SemanticRole::Slider
            | SemanticRole::Graph
    ) {
        return invalid(node, SemanticNodeIssue::NumericValueOnUnsupportedRole);
    }
    if !value.current.is_finite()
        || !value.minimum.is_finite()
        || !value.maximum.is_finite()
        || value.minimum > value.maximum
        || value.current < value.minimum
        || value.current > value.maximum
    {
        return invalid(node, SemanticNodeIssue::InvalidNumericRange);
    }
    Ok(())
}

fn invalid<T>(node: &SemanticNode, issue: SemanticNodeIssue) -> Result<T, SemanticSnapshotError> {
    Err(SemanticSnapshotError::InvalidNode {
        node: node.id.clone(),
        issue,
    })
}

const fn role_requires_name(role: SemanticRole) -> bool {
    matches!(
        role,
        SemanticRole::Button
            | SemanticRole::Switch
            | SemanticRole::CheckBox
            | SemanticRole::Radio
            | SemanticRole::Tab
            | SemanticRole::ColumnHeader
            | SemanticRole::Dialog
            | SemanticRole::AlertDialog
            | SemanticRole::TextField
            | SemanticRole::SearchBox
            | SemanticRole::Link
            | SemanticRole::Image
            | SemanticRole::Meter
            | SemanticRole::ProgressBar
            | SemanticRole::Slider
            | SemanticRole::Graph
            | SemanticRole::Option
            | SemanticRole::TreeItem
            | SemanticRole::MenuItem
    )
}

const fn role_supports_action(role: SemanticRole, action: SemanticAction) -> bool {
    match action {
        SemanticAction::Focus => !matches!(
            role,
            SemanticRole::Application
                | SemanticRole::Window
                | SemanticRole::Navigation
                | SemanticRole::Main
                | SemanticRole::Group
                | SemanticRole::StaticText
        ),
        SemanticAction::Press => matches!(
            role,
            SemanticRole::Button | SemanticRole::Tab | SemanticRole::Link | SemanticRole::MenuItem
        ),
        SemanticAction::Toggle => {
            matches!(role, SemanticRole::Switch | SemanticRole::CheckBox)
        }
        SemanticAction::Select => matches!(
            role,
            SemanticRole::Radio
                | SemanticRole::Tab
                | SemanticRole::Row
                | SemanticRole::Option
                | SemanticRole::TreeItem
        ),
        SemanticAction::Expand | SemanticAction::Collapse => matches!(
            role,
            SemanticRole::Button | SemanticRole::TreeItem | SemanticRole::MenuItem
        ),
        SemanticAction::Increment | SemanticAction::Decrement | SemanticAction::SetValue => {
            matches!(role, SemanticRole::Slider)
        }
        SemanticAction::Dismiss => {
            matches!(role, SemanticRole::Dialog | SemanticRole::AlertDialog)
        }
        SemanticAction::ReadPreviousValue | SemanticAction::ReadNextValue => {
            matches!(role, SemanticRole::Graph)
        }
    }
}
