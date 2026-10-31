//! Service dependency tree — the expandable relation hierarchy.
//!
//! **Ownership**: the shell owns the dependency facts
//! (`ShellApp::service_dependencies`) and the inventory carries each unit's
//! own relation graph; this module owns only the PRESENTATION: the tree
//! shape, the accordion expansion state and the disclosure rows.
//!
//! **Shape**: the opened unit is the root; every relation kind the shared
//! copy catalog names becomes one section, and each related unit becomes a
//! child of that section. A child the inventory also knows about carries its
//! own relations, so the panel shows a real hierarchy instead of a flat chip
//! list. The walk is depth-bounded and cycle-safe: a unit already on the
//! current path never expands again.
//!
//! **Accordion**: the root's sections start open — the direct dependencies
//! are what the panel is about — and every deeper branch is closed until the
//! reader opens it. The state is a typed resource
//! ([`ServiceDepsExpansion`]) keyed by the node key, so it survives a
//! repaint and can be asserted without touching pixels.

use std::collections::BTreeMap;

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, UiRect, Val, percent,
    rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};

use taskforest_core::core::services::{ServiceDeps, ServiceRelationKind};
use taskforest_core::core::target::ServiceId;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_ui_contract::IconId;

use crate::palette::{UiPalette, space_2, space_4, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

/// The maximum nesting depth of the dependency tree.
const MAX_DEPTH: usize = 3;

/// One node of the dependency tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DepTreeNode {
    /// The stable key of the node — the expansion authority.
    pub(crate) key: String,
    /// The display label.
    pub(crate) label: String,
    /// The relation that links this node to its parent.
    pub(crate) relation: Option<ServiceRelationKind>,
    /// The number of direct children.
    pub(crate) count: usize,
    /// Whether the node starts open.
    pub(crate) default_expanded: bool,
    /// The nested nodes.
    pub(crate) children: Vec<DepTreeNode>,
}

impl DepTreeNode {
    /// Whether the node can be expanded at all.
    #[must_use]
    pub(crate) fn expandable(&self) -> bool {
        !self.children.is_empty()
    }
}

/// The accordion state of the dependency tree: the reader's explicit
/// overrides over each node's default.
#[derive(Resource, Default, Clone, Debug)]
pub(crate) struct ServiceDepsExpansion {
    /// The per-node overrides: `true` = opened, `false` = closed.
    pub(crate) overrides: BTreeMap<String, bool>,
}

impl ServiceDepsExpansion {
    /// Toggle one node, returning the state it now holds.
    pub(crate) fn toggle(&mut self, key: &str, default_expanded: bool) -> bool {
        let current = self.is_expanded(key, default_expanded);
        self.overrides.insert(key.to_owned(), !current);
        !current
    }

    /// Whether one node is open.
    #[must_use]
    pub(crate) fn is_expanded(&self, key: &str, default_expanded: bool) -> bool {
        self.overrides.get(key).copied().unwrap_or(default_expanded)
    }
}

/// Marker on one disclosure row: the node key it toggles and the node's
/// default state (the toggle flips the CURRENT state, so the default is
/// part of the row's identity).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceDepsToggle(pub(crate) String, pub(crate) bool);

/// The relation vocabulary the tree projects — the four kinds the shared
/// copy catalog names.
fn relation_kinds() -> Vec<ServiceRelationKind> {
    vec![
        ServiceRelationKind::Requires,
        ServiceRelationKind::Wants,
        ServiceRelationKind::WantedBy,
        ServiceRelationKind::After,
    ]
}

/// The stable slug of one relation kind — the expansion key must not depend
/// on the active locale.
fn relation_slug(kind: &ServiceRelationKind) -> &'static str {
    match kind {
        ServiceRelationKind::Requires => "requires",
        ServiceRelationKind::Wants => "wants",
        ServiceRelationKind::WantedBy => "wanted_by",
        ServiceRelationKind::After => "after",
        _ => "other",
    }
}

/// The catalog label of one relation kind.
fn relation_label(kind: &ServiceRelationKind) -> &'static str {
    match kind {
        ServiceRelationKind::Requires => t("svc.requires"),
        ServiceRelationKind::Wants => t("svc.wants"),
        ServiceRelationKind::WantedBy => t("svc.wanted_by"),
        ServiceRelationKind::After => t("svc.after"),
        _ => t("svc.dependencies"),
    }
}

/// The display name of one unit: the inventory name when the unit is known,
/// the provider-issued id otherwise.
fn unit_label(shell: &ShellApp, id: &ServiceId) -> String {
    shell
        .sorted_services()
        .into_iter()
        .find(|item| &item.id == id)
        .map(|item| item.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// The relation facts the inventory knows about one unit.
fn inventory_relations(shell: &ShellApp, id: &ServiceId) -> ServiceDeps {
    shell
        .sorted_services()
        .into_iter()
        .find(|item| &item.id == id)
        .map(|item| ServiceDeps::from_relations(item.relations().clone()))
        .unwrap_or_default()
}

/// Build the dependency tree of one unit.
#[must_use]
pub(crate) fn dependency_tree(
    shell: &ShellApp,
    root: &ServiceId,
    deps: &ServiceDeps,
) -> DepTreeNode {
    let mut visited = std::collections::BTreeSet::new();
    visited.insert(root.clone());
    let children = relation_groups(shell, root.as_ref(), deps, 1, &mut visited);
    DepTreeNode {
        key: root.to_string(),
        label: unit_label(shell, root),
        relation: None,
        count: children.len(),
        default_expanded: true,
        children,
    }
}

/// The relation sections of one unit, with their related units nested.
fn relation_groups(
    shell: &ShellApp,
    parent_key: &str,
    deps: &ServiceDeps,
    depth: usize,
    visited: &mut std::collections::BTreeSet<ServiceId>,
) -> Vec<DepTreeNode> {
    let mut groups: Vec<DepTreeNode> = Vec::new();
    for kind in relation_kinds() {
        let targets: Vec<ServiceId> = deps.relation_targets(&kind).cloned().collect();
        let key = format!("{parent_key}/{}", relation_slug(&kind));
        let mut children: Vec<DepTreeNode> = Vec::new();
        for target in &targets {
            let child_key = format!("{key}/{target}");
            let nested = if depth + 1 < MAX_DEPTH && !visited.contains(target) {
                visited.insert(target.clone());
                let child_deps = inventory_relations(shell, target);
                relation_groups(shell, &child_key, &child_deps, depth + 1, visited)
            } else {
                Vec::new()
            };
            children.push(DepTreeNode {
                key: child_key,
                label: unit_label(shell, target),
                relation: Some(kind.clone()),
                count: nested.len(),
                default_expanded: false,
                children: nested,
            });
        }
        // An untargeted kind still names itself — an honest dash, never a
        // borrowed neighbour's targets.
        let label = if children.is_empty() {
            format!("{}: —", relation_label(&kind))
        } else {
            format!("{} ({}):", relation_label(&kind), children.len())
        };
        groups.push(DepTreeNode {
            key,
            label,
            relation: Some(kind),
            count: children.len(),
            default_expanded: depth <= 1,
            children,
        });
    }
    groups
}

/// The disclosure marker of one node: the semantic direction plate, never a
/// text glyph.
fn disclosure_scene(expanded: bool, palette: &UiPalette) -> Box<dyn Scene> {
    let icon = if expanded {
        IconId::NavigateDown
    } else {
        IconId::NavigateUp
    };
    crate::icons::icon_scene(icon, palette, palette.dim_color)
}

/// One tree row: the disclosure control and the node label.
fn node_row_scene(
    node: &DepTreeNode,
    expansion: &ServiceDepsExpansion,
    depth: usize,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let expanded = expansion.is_expanded(&node.key, node.default_expanded);
    let marker = if node.expandable() {
        disclosure_scene(expanded, palette)
    } else {
        Box::new(bsn! {
            Node { width: rem(palette.icon_rem), height: rem(palette.icon_rem), flex_shrink: 0.0 }
        })
    };
    let label = node.label.clone();
    let indent = space_12() * depth as f32;
    let key = node.key.clone();
    let default_expanded = node.default_expanded;
    let radius = palette.control_radius_px;
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            padding: UiRect::horizontal(Val::Px(indent)),
            overflow: Overflow::clip_x(),
        }
        Pickable::IGNORE
        Children [
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_4()),
                height: rem(palette.control_height_rem),
                padding: UiRect::horizontal(Val::Px(space_8())),
                border_radius: BorderRadius::all(Val::Px(radius)),
            }
            BackgroundColor({
                if expanded { palette.nav_active_bg } else { palette.content_bg }
            })
            ControlVisual(ControlTone::Surface, expanded)
            Button
            on(on_deps_toggle)
            ServiceDepsToggle(key, default_expanded)
            Children [
                 @{ marker } --
                 Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
            ]
        ]
    })
}

/// The whole tree: the root's sections plus every opened branch.
pub(crate) fn dependency_tree_scene(
    tree: &DepTreeNode,
    expansion: &ServiceDepsExpansion,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    for child in &tree.children {
        rows.push(node_row_scene(child, expansion, 1, palette));
        if expansion.is_expanded(&child.key, child.default_expanded) {
            collect_rows(child, expansion, 2, palette, &mut rows);
        }
    }
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            { rows }
        ]
    })
}

/// The rows of every opened branch below one node.
fn collect_rows(
    node: &DepTreeNode,
    expansion: &ServiceDepsExpansion,
    depth: usize,
    palette: &UiPalette,
    rows: &mut Vec<Box<dyn Scene>>,
) {
    for child in &node.children {
        rows.push(node_row_scene(child, expansion, depth, palette));
        if expansion.is_expanded(&child.key, child.default_expanded) {
            collect_rows(child, expansion, depth + 1, palette, rows);
        }
    }
}

/// Disclosure tail: toggle one node and repaint the panel.
pub(crate) fn on_deps_toggle(
    activate: On<Activate>,
    buttons: Query<&ServiceDepsToggle>,
    mut expansion: ResMut<ServiceDepsExpansion>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    expansion.toggle(&button.0, button.1);
    commands.trigger(super::dependencies_panel::DependenciesPanelRepaintRequired);
}
