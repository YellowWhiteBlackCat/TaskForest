//! Applications-page tree projection for the Bevy frontend.
//!
//! The projection consumes the shared category and process-tree algorithms,
//! keeps row identity typed, and stores expansion by locale-neutral keys. The
//! render adapter is a small `bsn!` scene mounted in the formal Applications
//! route; the flat table below it remains the search/sort reducer surface.

use std::collections::HashSet;

use crate::widgets::controls::{ControlTone, ControlVisual};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res};
use bevy::ecs::world::DeferredWorld;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::prelude::{AlignItems, BorderRadius, Node, UiRect, Val, percent, px};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::t;
use taskmanager_application::process_category_projection::{
    category_buckets, category_expansion_key,
};
use taskmanager_core::core::process::{
    ProcessCategory, ProcessItem, build_process_tree, flatten_tree_visible, process_category,
};

use taskmanager_shell::ProcessRowId;

use crate::app::{FrontendTrack, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::palette::{UiPalette, space_8};
use crate::widgets::control_contract::ControlSurface;
use crate::window::{Role, TextRole, WindowPalette};

/// Stable expansion state for the Applications tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessTreeExpansion {
    expanded_categories: HashSet<String>,
    expanded_applications: HashSet<u32>,
    collapsed_processes: HashSet<u32>,
}

impl ProcessTreeExpansion {
    #[must_use]
    pub(crate) fn category_expanded(&self, category: ProcessCategory) -> bool {
        self.expanded_categories
            .contains(&category_expansion_key(category))
    }

    #[allow(dead_code)]
    pub(crate) fn toggle_category(&mut self, category: ProcessCategory) {
        let key = category_expansion_key(category);
        if !self.expanded_categories.insert(key.clone()) {
            self.expanded_categories.remove(&key);
        }
    }

    #[must_use]
    pub(crate) fn application_expanded(&self, root_pid: u32) -> bool {
        self.expanded_applications.contains(&root_pid)
    }

    #[allow(dead_code)]
    pub(crate) fn toggle_application(&mut self, root_pid: u32) {
        if !self.expanded_applications.insert(root_pid) {
            self.expanded_applications.remove(&root_pid);
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn process_collapsed(&self, pid: u32) -> bool {
        self.collapsed_processes.contains(&pid)
    }

    #[allow(dead_code)]
    pub(crate) fn toggle_process(&mut self, pid: u32) {
        if !self.collapsed_processes.insert(pid) {
            self.collapsed_processes.remove(&pid);
        }
    }

    fn collapsed_set(&self) -> &HashSet<u32> {
        &self.collapsed_processes
    }
}

/// One row in the tree projection. Category and application rows have no
/// process reference; only a real process row carries an item, so a visual
/// aggregate can never be mistaken for an executable target.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessTreeRow<'a> {
    pub(crate) key: ProcessRowId,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) member_count: usize,
    pub(crate) has_children: bool,
    pub(crate) expanded: bool,
    pub(crate) item: Option<&'a ProcessItem>,
}

/// Project the complete visible process inventory into category/tree rows.
///
/// Category order and classification come from the application layer. Empty
/// buckets are omitted; unavailable application identity remains in the
/// explicit Uncategorized bucket. The caller owns the input snapshot, while
/// rows borrow it without cloning every process.
#[must_use]
pub(crate) fn project_items<'a>(
    items: &'a [ProcessItem],
    expansion: &ProcessTreeExpansion,
) -> Vec<ProcessTreeRow<'a>> {
    let buckets = category_buckets(items, process_category);
    let mut rows = Vec::new();

    let bucket_specs: Vec<(ProcessCategory, usize)> = buckets
        .iter()
        .map(|bucket| (bucket.category(), bucket.member_count()))
        .collect();
    for (category, member_count) in bucket_specs {
        // Reborrow from the caller's slice rather than from the temporary
        // bucket projection. This keeps the returned rows tied to `items`,
        // never to the local category-bucket container.
        let members: Vec<&ProcessItem> = items
            .iter()
            .filter(|item| process_category(item) == category)
            .collect();
        let category_expanded = expansion.category_expanded(category);
        rows.push(ProcessTreeRow {
            key: ProcessRowId::Category(category),
            depth: 0,
            label: category_label(category),
            member_count,
            has_children: true,
            expanded: category_expanded,
            item: None,
        });
        if !category_expanded {
            continue;
        }

        let roots = build_process_tree(&members);
        if category == ProcessCategory::Application {
            for root in roots {
                let application_expanded = expansion.application_expanded(root.item.pid);
                rows.push(ProcessTreeRow {
                    key: root
                        .item
                        .current_start_token()
                        .and_then(|token| {
                            taskmanager_shell::ProcessRowIdentity::from_parts(root.item.pid, token)
                        })
                        .map(ProcessRowId::Application)
                        .unwrap_or(ProcessRowId::Category(
                            taskmanager_core::core::process::ProcessCategory::Application,
                        )),
                    depth: 1,
                    label: root
                        .item
                        .current_application_name()
                        .unwrap_or(root.item.name.as_str())
                        .to_owned(),
                    member_count: tree_size(&root),
                    has_children: !root.children.is_empty(),
                    expanded: application_expanded,
                    item: None,
                });
                if application_expanded {
                    push_process_rows(&root, 2, expansion.collapsed_set(), &mut rows);
                }
            }
        } else {
            for root in roots {
                push_process_rows(&root, 1, expansion.collapsed_set(), &mut rows);
            }
        }
    }

    rows
}

fn push_process_rows<'a>(
    root: &taskmanager_core::core::process::ProcessNode<'a>,
    depth_offset: usize,
    collapsed: &HashSet<u32>,
    rows: &mut Vec<ProcessTreeRow<'a>>,
) {
    for flat in flatten_tree_visible(std::slice::from_ref(root), collapsed) {
        rows.push(ProcessTreeRow {
            key: ProcessRowId::from_process(flat.item).unwrap_or(ProcessRowId::Category(
                taskmanager_core::core::process::ProcessCategory::Uncategorized,
            )),
            depth: depth_offset + flat.depth,
            label: flat.item.name.clone(),
            member_count: 1,
            has_children: flat.has_children,
            expanded: !collapsed.contains(&flat.item.pid),
            item: Some(flat.item),
        });
    }
}

fn tree_size(node: &taskmanager_core::core::process::ProcessNode<'_>) -> usize {
    1 + node.children.iter().map(tree_size).sum::<usize>()
}

fn category_label(category: ProcessCategory) -> String {
    match category {
        ProcessCategory::Application => "Applications".to_owned(),
        ProcessCategory::Background => "Background".to_owned(),
        ProcessCategory::Uncategorized => "Uncategorized".to_owned(),
    }
}

/// Minimal themed row scene. The full page will add selection and observer
/// components when this projection is mounted into the route.
pub(crate) fn row_scene(row: &ProcessTreeRow<'_>, palette: &UiPalette) -> impl Scene + use<> {
    let label = if row.member_count > 1 || row.item.is_none() {
        format!("{} · {}", row.label, row.member_count)
    } else {
        row.label.clone()
    };
    let left_padding = space_8() * (row.depth as f32 + 1.0);
    let semantic = SemanticAddress(stable_semantic_address(
        "process-tree",
        &semantic_key(row.key),
    ));
    bsn! {
        Node {
            width: percent(100),
            height: px(palette.control_height_px),
            align_items: AlignItems::Center,
            padding: UiRect::left(Val::Px(left_padding)),
        }
        ProcessTreeRowMarker({ row.key })
        SemanticAddress({ semantic.0.clone() })
        Children [
            ( Text(label) TextRole(Role::Body) ),
        ]
    }
}

fn semantic_key(key: ProcessRowId) -> String {
    match key {
        ProcessRowId::Category(category) => format!("category:{category:?}"),
        ProcessRowId::Application(identity) => format!("application:{}", identity.pid()),
        ProcessRowId::Process(identity) => format!("process:{}", identity.pid()),
    }
}

/// Compact tree strip mounted above the existing Applications table. The
/// flat table remains the interaction/detail surface (search, sort, and row
/// reducers), while this strip makes category/application/process identity
/// and the typed control/input anchors part of the formal route.
pub(crate) fn panel_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let items = context.shell.projection().processes_slice();
    let expansion = ProcessTreeExpansion::default();
    let rows = project_items(items, &expansion);
    let row_scenes: Vec<Box<dyn Scene>> = rows
        .iter()
        .map(|row| Box::new(row_scene(row, context.palette)) as Box<dyn Scene>)
        .collect();
    let title = format!("Process tree · {} processes", items.len());
    let row_scenes = row_scenes;
    let end_label = t("proc.end_process_tree").to_owned();
    let end_height = context.palette.control_height_px;
    let end_radius = context.palette.control_radius_px;
    bsn! {
        Node {
            width: percent(100),
            height: px(context.palette.control_height_px * 4.0),
            flex_direction: bevy::ui::prelude::FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        ProcessTreeSurface
        ControlSurface
        Children [
            (
                Node {
                    width: percent(100),
                    flex_direction: bevy::ui::prelude::FlexDirection::Row,
                    justify_content: bevy::ui::prelude::JustifyContent::SpaceBetween,
                    align_items: bevy::ui::prelude::AlignItems::Center,
                }
                Children [
                    ( Text(title) ProcessTreeCountLine TextRole(Role::Caption) ),
                    (
                        Node {
                            height: px(end_height),
                            padding: UiRect::horizontal(Val::Px(space_8())),
                            align_items: bevy::ui::prelude::AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(end_radius)),
                        }
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_end_tree_activated)
                        Children [
                            ( Text(end_label) TextRole(Role::Caption) ),
                        ]
                    ),
                ]
            ),
            (
                Node { flex_direction: bevy::ui::prelude::FlexDirection::Column }
                ProcessTreeRows
                Children [{ row_scenes }]
            ),
        ]
    }
}

/// Marker for the formal Applications route's hierarchy strip.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(on_insert = bind_tree_observer)]
pub(crate) struct ProcessTreeSurface;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProcessTreeRows;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProcessTreeCountLine;

#[derive(Resource, Default)]
struct TreeObserverBound;

fn bind_tree_observer(mut world: DeferredWorld<'_>, _context: HookContext) {
    if world.get_resource::<TreeObserverBound>().is_some() {
        return;
    }
    world.commands().insert_resource(TreeObserverBound);
    world.commands().add_observer(refresh_tree_on_fold);
}

fn refresh_tree_on_fold(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    roots: Query<(Entity, &Children), With<ProcessTreeRows>>,
    mut counts: Query<&mut Text, With<ProcessTreeCountLine>>,
    mut commands: Commands,
) {
    let items = track.shell().projection().processes_slice();
    let expansion = ProcessTreeExpansion::default();
    let rows = project_items(items, &expansion);
    for (root, children) in roots.iter() {
        let stale: Vec<Entity> = children.iter().copied().collect();
        for entity in stale {
            commands.entity(entity).despawn();
        }
        for row in &rows {
            let child = commands.spawn_scene(row_scene(row, &palette.inner)).id();
            commands.entity(root).add_one_related::<ChildOf>(child);
        }
    }
    if let Ok(mut count) = counts.single_mut() {
        count.0 = format!("Process tree · {} processes", items.len());
    }
}

/// The End-tree affordance: freeze the selected process's tree into the
/// shared ProcessBatch gate (`request_process_tree_end`). The confirmation
/// modal the gate arms is the shared one; this button only arms.
fn on_end_tree_activated(
    _activate: On<bevy::ui_widgets::Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let shell = &mut track.shell;
    let Some(process) = shell.visible_process_at(shell.selected) else {
        return;
    };
    let pid = process.pid;
    let _ = shell.request_process_tree_end(pid);
    let view = shell
        .pending_confirmation()
        .and_then(crate::confirmation::PendingConfirmationView::from_pending);
    commands.trigger(crate::confirmation::ConfirmationChanged(view));
}

/// Identity marker for later pointer/keyboard observers.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessTreeRowMarker(pub(crate) ProcessRowId);

impl Default for ProcessTreeRowMarker {
    fn default() -> Self {
        Self(ProcessRowId::Category(ProcessCategory::Application))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/process_tree.rs"]
mod tests;
