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
use bevy::ecs::event::Event;
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
use taskmanager_application::process_category_projection::category_expansion_key;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessLiveKey};

use taskmanager_shell::presentation::{MISSING_VALUE, bytes};
use taskmanager_shell::{
    ProcessRowId, ProcessTreeRow, SortCol, SortDir, app_tree_expansion_key_for_identity,
    process_semantic_key, project_process_tree_rows,
};

use crate::app::{FrontendTrack, PageContext, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::palette::{UiPalette, space_8};
use crate::widgets::control_contract::ControlSurface;
use crate::window::{Role, TextRole, WindowPalette};

/// Stable expansion state for the Applications tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessTreeExpansion {
    expanded_groups: HashSet<String>,
    collapsed_processes: HashSet<ProcessLiveKey>,
}

impl ProcessTreeExpansion {
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn category_expanded(&self, category: ProcessCategory) -> bool {
        self.expanded_groups
            .contains(&category_expansion_key(category))
    }

    pub(crate) fn toggle_category(&mut self, category: ProcessCategory) {
        let key = category_expansion_key(category);
        if !self.expanded_groups.insert(key.clone()) {
            self.expanded_groups.remove(&key);
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn application_expanded(&self, root: ProcessLiveKey) -> bool {
        self.expanded_groups
            .contains(&app_tree_expansion_key_for_identity(root))
    }

    pub(crate) fn toggle_application(&mut self, root: ProcessLiveKey) {
        let key = app_tree_expansion_key_for_identity(root);
        if !self.expanded_groups.insert(key.clone()) {
            self.expanded_groups.remove(&key);
        }
    }

    pub(crate) fn toggle_group_key(&mut self, key: String) {
        if !self.expanded_groups.insert(key.clone()) {
            self.expanded_groups.remove(&key);
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn process_collapsed(&self, identity: ProcessLiveKey) -> bool {
        self.collapsed_processes.contains(&identity)
    }

    pub(crate) fn toggle_process(&mut self, identity: ProcessLiveKey) {
        if !self.collapsed_processes.insert(identity) {
            self.collapsed_processes.remove(&identity);
        }
    }

    fn collapsed_set(&self) -> &HashSet<ProcessLiveKey> {
        &self.collapsed_processes
    }

    fn shared_expanded_groups(&self) -> HashSet<String> {
        self.expanded_groups.clone()
    }
}

/// Bevy's scene-facing adaptation of one shell-owned structural row. The
/// shared row remains the source of identity, hierarchy, and typed metrics;
/// this view adds only borrowed labels and Bevy-independent scene data.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProcessTreeRowView<'a> {
    pub(crate) key: Option<ProcessRowId>,
    pub(crate) semantic_key: String,
    pub(crate) expansion_key: Option<String>,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) member_count: usize,
    pub(crate) cpu: Option<AggregateMetric<f32>>,
    pub(crate) memory: Option<AggregateMetric<u64>>,
    pub(crate) has_children: bool,
    pub(crate) expanded: bool,
    pub(crate) item: Option<&'a ProcessItem>,
}

/// Typed local action emitted by a tree row. It changes only Bevy's
/// expansion state; process controls continue to use the shell's atomic
/// frozen-identity requests.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ProcessTreeToggle {
    Category(ProcessCategory),
    Application(ProcessLiveKey),
    Process(ProcessLiveKey),
    UnknownGroup(String),
}

/// Adapt the shell-owned structural projection into Bevy's local row payload.
/// Bevy owns only its scene-facing label and borrowed process reference; it no
/// longer decides category order, tree depth, expansion, or aggregate rules.
#[must_use]
pub(crate) fn project_items<'a>(
    items: &'a [ProcessItem],
    expansion: &ProcessTreeExpansion,
    observed_at_ms: u64,
) -> Vec<ProcessTreeRowView<'a>> {
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let expanded_groups = expansion.shared_expanded_groups();
    project_process_tree_rows(
        &refs,
        &expanded_groups,
        expansion.collapsed_set(),
        (SortCol::Pid, SortDir::Asc),
        observed_at_ms,
    )
    .into_iter()
    .filter_map(|row| match row {
        ProcessTreeRow::Category {
            category,
            expansion_key,
            expanded,
            member_count,
            aggregate,
            ..
        } => Some(ProcessTreeRowView {
            key: Some(ProcessRowId::Category(category)),
            semantic_key: ProcessRowId::Category(category).stable_key(),
            expansion_key: Some(expansion_key),
            depth: 0,
            label: category_label(category),
            member_count,
            cpu: Some(aggregate.cpu().clone()),
            memory: Some(aggregate.memory().clone()),
            has_children: true,
            expanded,
            item: None,
        }),
        ProcessTreeRow::Application {
            visible_index,
            row_key,
            expansion_key,
            expanded,
            member_count,
            aggregate,
            has_children,
            ..
        } => {
            let root = items.get(visible_index)?;
            let semantic_key = row_key.map_or_else(
                || format!("application:{expansion_key}"),
                |key| key.stable_key(),
            );
            Some(ProcessTreeRowView {
                key: row_key,
                semantic_key,
                expansion_key: Some(expansion_key),
                depth: 1,
                label: root
                    .current_application_name()
                    .unwrap_or(root.name.as_str())
                    .to_owned(),
                member_count,
                cpu: Some(aggregate.cpu().clone()),
                memory: Some(aggregate.memory().clone()),
                has_children,
                expanded,
                item: None,
            })
        }
        ProcessTreeRow::Process {
            visible_index,
            row_key,
            depth,
            has_children,
            collapsed,
            ..
        } => {
            let process = items.get(visible_index)?;
            let semantic_key =
                row_key.map_or_else(|| process_semantic_key(process), |key| key.stable_key());
            Some(ProcessTreeRowView {
                key: row_key,
                semantic_key,
                expansion_key: None,
                depth,
                label: process.name.clone(),
                member_count: 1,
                cpu: None,
                memory: None,
                has_children,
                expanded: !collapsed,
                item: Some(process),
            })
        }
    })
    .collect()
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
pub(crate) fn row_scene(row: &ProcessTreeRowView<'_>, palette: &UiPalette) -> impl Scene + use<> {
    let mut label = if row.member_count > 1 || row.item.is_none() {
        format!("{} · {}", row.label, row.member_count)
    } else {
        row.label.clone()
    };
    if let (Some(cpu), Some(memory)) = (&row.cpu, &row.memory) {
        label.push_str(&format!(
            " · {} · {}",
            metric_text(cpu.current_value().map(|value| format!("{value:.1}%"))),
            metric_text(memory.current_value().map(|value| bytes(*value))),
        ));
    }
    let left_padding = space_8() * (row.depth as f32 + 1.0);
    let semantic = SemanticAddress(stable_semantic_address("process-tree", &row.semantic_key));
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
            height: px(palette.control_height_px),
            align_items: AlignItems::Center,
            padding: UiRect::left(Val::Px(left_padding)),
        }
        ProcessTreeRowMarker({ toggle })
        Button
        on(on_tree_row_activated)
        SemanticAddress({ semantic.0.clone() })
        Children [
            ( Text(label) TextRole(Role::Body) ),
        ]
    }
}

/// Format one aggregate cell. A missing current value renders the shared
/// unavailable-value dash, never a fabricated zero.
fn metric_text(value: Option<String>) -> String {
    value.unwrap_or_else(|| MISSING_VALUE.to_owned())
}

/// Compact tree strip mounted above the existing Applications table. The
/// flat table remains the interaction/detail surface (search, sort, and row
/// reducers), while this strip makes category/application/process identity
/// and the typed control/input anchors part of the formal route.
pub(crate) fn panel_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let items = context.shell.projection().processes_slice();
    let observed_at_ms = context.shell.projection().processes_observed_at_ms;
    let rows = project_items(items, context.process_tree_expansion, observed_at_ms);
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
    world.commands().add_observer(refresh_tree_on_expansion);
}

#[derive(Event)]
struct ProcessTreeExpansionChanged;

fn on_tree_row_activated(
    activate: On<bevy::ui_widgets::Activate>,
    markers: Query<&ProcessTreeRowMarker>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(marker) = markers.get(activate.event().entity) else {
        return;
    };
    let Some(toggle) = marker.0.clone() else {
        return;
    };
    match toggle {
        ProcessTreeToggle::Category(category) => {
            track.process_tree_expansion.toggle_category(category);
        }
        ProcessTreeToggle::Application(identity) => {
            track.process_tree_expansion.toggle_application(identity);
        }
        ProcessTreeToggle::Process(identity) => {
            track.process_tree_expansion.toggle_process(identity);
        }
        ProcessTreeToggle::UnknownGroup(key) => {
            track.process_tree_expansion.toggle_group_key(key);
        }
    }
    commands.trigger(ProcessTreeExpansionChanged);
}

fn refresh_tree_on_fold(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    roots: Query<(Entity, &Children), With<ProcessTreeRows>>,
    counts: Query<&mut Text, With<ProcessTreeCountLine>>,
    mut commands: Commands,
) {
    let items = track.shell().projection().processes_slice();
    let observed_at_ms = track.shell().projection().processes_observed_at_ms;
    let rows = project_items(items, track.process_tree_expansion(), observed_at_ms);
    refresh_tree_rows(&palette, roots, counts, &mut commands, rows, items);
}

fn refresh_tree_on_expansion(
    _changed: On<ProcessTreeExpansionChanged>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    roots: Query<(Entity, &Children), With<ProcessTreeRows>>,
    counts: Query<&mut Text, With<ProcessTreeCountLine>>,
    mut commands: Commands,
) {
    let items = track.shell().projection().processes_slice();
    let observed_at_ms = track.shell().projection().processes_observed_at_ms;
    let rows = project_items(items, track.process_tree_expansion(), observed_at_ms);
    refresh_tree_rows(&palette, roots, counts, &mut commands, rows, items);
}

fn refresh_tree_rows(
    palette: &WindowPalette,
    roots: Query<(Entity, &Children), With<ProcessTreeRows>>,
    mut counts: Query<&mut Text, With<ProcessTreeCountLine>>,
    commands: &mut Commands,
    rows: Vec<ProcessTreeRowView<'_>>,
    items: &[ProcessItem],
) {
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
    let Some(identity) = ProcessLiveKey::from_process(process) else {
        return;
    };
    shell.request_process_tree_end(identity);
    let view = shell
        .pending_confirmation()
        .and_then(crate::confirmation::PendingConfirmationView::from_pending);
    commands.trigger(crate::confirmation::ConfirmationChanged(view));
}

/// Identity marker for later pointer/keyboard observers.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessTreeRowMarker(Option<ProcessTreeToggle>);

impl Default for ProcessTreeRowMarker {
    fn default() -> Self {
        Self(Some(ProcessTreeToggle::Category(
            ProcessCategory::Application,
        )))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/process_tree.rs"]
mod tests;
