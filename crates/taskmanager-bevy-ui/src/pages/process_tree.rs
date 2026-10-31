//! Applications-page tree projection for the Bevy frontend.
//!
//! The projection consumes the shared category and process-tree algorithms,
//! keeps row identity typed, and stores expansion by locale-neutral keys. The
//! render adapter is a small `bsn!` scene mounted in the formal Applications
//! route; the flat table below it remains the search/sort reducer surface.
//!
//! The tree's own modules own one concern each: the owned row plan, the
//! hierarchy guide, the disclosure chevron, the row badges, the hover group
//! hint, the progressive mount, and the collateral tree of an affected root.

use bevy::picking::Pickable;
use std::collections::HashSet;

use bevy::app::{Last, PostUpdate, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{AlignSelf, FlexDirection, Node, Overflow, UiRect, Val, percent, px, rem};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Button, ScrollArea};
use taskforest_core::core::process::aggregate::AggregateMetric;
use taskforest_core::core::process::{ProcessCategory, ProcessItem, ProcessLiveKey};
use taskmanager_application::i18n::t;
use taskmanager_application::process_category_projection::category_expansion_key;

use taskmanager_shell::{
    ProcessRowId, ProcessTreeRow, SortCol, SortDir, app_tree_expansion_key_for_identity,
    process_semantic_key, project_process_tree_rows,
};

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use taskmanager_shell::ShellApp;

mod aggregate;
mod badge;
mod chevron;
mod collateral;
mod guide;
mod hover;
mod mount;
mod plan;

use aggregate::row_scene;
use chevron::ChevronAnimations;
use mount::TreeRowsState;
use plan::{GuideColumn, guide_columns};

/// Stable expansion state for the Applications tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessTreeExpansion {
    expanded_groups: HashSet<String>,
    collapsed_processes: HashSet<ProcessLiveKey>,
}

impl ProcessTreeExpansion {
    pub(crate) fn toggle_category(&mut self, category: ProcessCategory) {
        let key = category_expansion_key(category);
        if !self.expanded_groups.insert(key.clone()) {
            self.expanded_groups.remove(&key);
        }
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

    pub(crate) fn toggle_process(&mut self, identity: ProcessLiveKey) {
        if !self.collapsed_processes.insert(identity) {
            self.collapsed_processes.remove(&identity);
        }
    }

    fn expand_all(&mut self, items: &[ProcessItem], observed_at_ms: u64) {
        self.collapsed_processes.clear();
        loop {
            let keys: Vec<_> = project_items(items, self, observed_at_ms)
                .into_iter()
                .filter_map(|row| row.expansion_key)
                .collect();
            let before = self.expanded_groups.len();
            self.expanded_groups.extend(keys);
            if self.expanded_groups.len() == before {
                break;
            }
        }
    }
    fn collapse_all(&mut self) {
        self.expanded_groups.clear();
        self.collapsed_processes.clear();
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
    /// The hierarchy guide columns of this row (BEVY-125).
    pub(crate) guide: Vec<GuideColumn>,
}

/// Typed local action emitted by a tree row. It changes only Bevy's
/// expansion state; process controls continue to use the shell's atomic
/// frozen-identity requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProcessTreeToggle {
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
    let mut rows: Vec<ProcessTreeRowView<'a>> = project_process_tree_rows(
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
            guide: Vec::new(),
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
                guide: Vec::new(),
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
                guide: Vec::new(),
            })
        }
    })
    .collect();
    // The guide needs the whole ordered projection: a vertical connector
    // belongs to the rows that share one ancestor.
    let columns = guide_columns(&rows);
    for (row, guide) in rows.iter_mut().zip(columns) {
        row.guide = guide;
    }
    rows
}

fn category_label(category: ProcessCategory) -> String {
    match category {
        ProcessCategory::Application => "Applications".to_owned(),
        ProcessCategory::Background => "Background".to_owned(),
        ProcessCategory::Uncategorized => "Uncategorized".to_owned(),
    }
}

#[derive(Event, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum TreeExpansionCommand {
    #[default]
    ExpandAll,
    CollapseAll,
}
#[derive(Component, Clone, Default)]
pub(crate) struct TreeExpansionControl(pub(crate) TreeExpansionCommand);
fn tree_expansion_button(
    label: &'static str,
    action: TreeExpansionCommand,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {Node {min_height:rem(palette.control_height_rem),padding:UiRect::horizontal(px(space_8())),flex_shrink:0.0} Button TreeExpansionControl({action}) on(on_tree_expansion_activated) Children [Text(label) TextRole(Role::Caption) Pickable::IGNORE]}
}
fn on_tree_expansion_activated(
    event: On<bevy::ui_widgets::Activate>,
    controls: Query<&TreeExpansionControl>,
    mut commands: Commands,
) {
    if let Ok(control) = controls.get(event.entity) {
        commands.trigger(control.0);
    }
}
fn on_tree_expansion_command(
    event: On<TreeExpansionCommand>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    match *event.event() {
        TreeExpansionCommand::ExpandAll => {
            let projection = track.shell.projection();
            let items = projection.processes_slice().to_vec();
            let observed_at_ms = projection.processes_observed_at_ms;
            track
                .process_tree_expansion
                .expand_all(&items, observed_at_ms);
        }
        TreeExpansionCommand::CollapseAll => track.process_tree_expansion.collapse_all(),
    }
    commands.trigger(ProcessTreeExpansionChanged);
}

/// The hierarchy inspection uses the shared bounded modal body.
pub(crate) fn scene(
    shell: &ShellApp,
    expansion: &ProcessTreeExpansion,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let items: Vec<_> = shell.visible_processes().into_iter().cloned().collect();
    let rows = project_items(
        &items,
        expansion,
        shell.projection().processes_observed_at_ms,
    );
    let row_scenes: Vec<Box<dyn Scene>> = rows
        .iter()
        .map(|row| Box::new(row_scene(row, palette)) as Box<dyn Scene>)
        .collect();
    let mut sections: Vec<Box<dyn Scene>> = vec![Box::new(
        bsn! {Node {width:percent(100),flex_direction:FlexDirection::Column} ProcessTreeRows Children [{row_scenes}]},
    )];
    // The collateral subtree of the selected process: the dense, indented
    // list of the processes an action on the selection would reach.
    let collateral = collateral_preview(shell, &items);
    if !collateral.is_empty() {
        sections.push(collateral::collateral_scene(&collateral, palette));
    }
    // The tree's own bounded viewport: the hierarchy is a high-density
    // inspection surface, so it declares its OWN scroll owner and the
    // visible rail instead of letting the shared modal body clip it. The
    // body stretches to the modal body's inner height, so exactly one
    // viewport scrolls — there is no nested scrollbar.
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::bottom(px(space_8())));
    let body = Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            align_self: AlignSelf::Stretch,
            flex_direction: FlexDirection::Column,
            row_gap: px(space_8()),
        }
        Children [
            // The viewport wrapper owns the scroll area AND the floating
            // rail: the rail is a sibling of the scroll area, so it never
            // scrolls with the content and never takes layout space away
            // from it.
            Node {
                width: percent(100),
                min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                flex_direction: FlexDirection::Column,
            }
            Children [
                Node {
                    width: percent(100),
                    min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(space_8()),
                    overflow: Overflow::scroll_y(),
                    padding: { lane },
                }
                ScrollArea ProcessTreeViewport
                Children [{sections}]
                --
                @{ rail }
            ]
        ]
    });
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(tree_expansion_button(
            t("proc.expand_all"),
            TreeExpansionCommand::ExpandAll,
            palette,
        )),
        Box::new(tree_expansion_button(
            t("proc.collapse_all"),
            TreeExpansionCommand::CollapseAll,
            palette,
        )),
        Box::new(close_button(palette)),
    ];
    crate::window_surface::modal_scene(
        crate::window_surface::WindowSurfaceKind::ProcessTree,
        "Process tree",
        body,
        actions,
        palette,
    )
}
/// The collateral subtree of the shell's selected process.
///
/// The frozen closure is the shared core authority: the list names every
/// process below the selection that an action would reach.
fn collateral_preview(shell: &ShellApp, items: &[ProcessItem]) -> Vec<collateral::CollateralRow> {
    shell
        .selected_row
        .and_then(ProcessRowId::live_key)
        .map(|root| collateral::collateral_rows(items, root))
        .unwrap_or_default()
}

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct TreeWindowControl(pub(crate) bool);

pub(crate) fn open_button(palette: &UiPalette) -> impl Scene + use<> {
    window_button("Process tree", true, palette)
}
fn close_button(palette: &UiPalette) -> impl Scene + use<> {
    window_button(t("common.close"), false, palette)
}
fn window_button(label: &'static str, open: bool, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {Node {min_height:rem(palette.control_height_rem),padding:UiRect::horizontal(px(space_8())),flex_shrink:0.0} Button TreeWindowControl(open) on(on_window_activated) Children [Text(label) TextRole(Role::Caption) Pickable::IGNORE]}
}
fn on_window_activated(
    event: On<bevy::ui_widgets::Activate>,
    controls: Query<&TreeWindowControl>,
    mut commands: Commands,
) {
    if let Ok(control) = controls.get(event.entity) {
        commands.trigger(if control.0 {
            crate::window_surface::WindowSurfaceCommand::ProcessTree
        } else {
            crate::window_surface::WindowSurfaceCommand::Close(
                crate::window_surface::WindowSurfaceKind::ProcessTree,
            )
        });
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProcessTreeRows;

/// The tree's own scroll owner: the bounded viewport the hierarchy scrolls
/// inside, with the visible rail beside it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessTreeViewport;

pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<TreeRowsState>();
    app.init_resource::<ChevronAnimations>();
    app.add_observer(on_tree_expansion_command);
    app.add_observer(refresh_tree_on_fold);
    app.add_observer(refresh_tree_on_expansion);
    app.add_systems(Update, mount::sync_tree_rows);
    app.add_systems(
        PostUpdate,
        (
            chevron::animate_chevrons,
            badge::emphasize_aggregate_values,
            hover::paint_hover_group,
        ),
    );
    app.add_systems(Last, mount::flush_hidden_rows);
}

#[derive(Event)]
struct ProcessTreeExpansionChanged;

fn on_tree_row_activated(
    activate: On<bevy::ui_widgets::Activate>,
    markers: Query<&ProcessTreeRowMarker>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let entity: Entity = activate.event().entity;
    let Ok(marker) = markers.get(entity) else {
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

/// The projection folded: the tree's rows have to be recomputed.
fn refresh_tree_on_fold(_fold: On<ShellProjectionFolded>, mut state: ResMut<TreeRowsState>) {
    state.dirty = true;
}

/// The expansion changed: the tree's rows have to be recomputed.
fn refresh_tree_on_expansion(
    _changed: On<ProcessTreeExpansionChanged>,
    mut state: ResMut<TreeRowsState>,
) {
    state.dirty = true;
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

#[cfg(test)]
#[path = "../../tests/headless/pages/process_tree_parity.rs"]
mod parity;
