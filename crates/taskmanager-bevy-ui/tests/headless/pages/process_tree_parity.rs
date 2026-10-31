//! test-intent: behavior
//!
//! The Bevy process-tree page owes the shared parity ledger seven behaviors:
//! the hierarchy guide that shows which ancestor owns a row (BEVY-125), the
//! disclosure turn (BEVY-126), the aggregate emphasis (BEVY-127), the
//! progressive mount (BEVY-128), the zombie badge (BEVY-129), the hover
//! group hint (BEVY-130), and the collateral tree of an affected root
//! (BEVY-131).
//!
//! Every check below proves a behavior of the production scene or system —
//! never the source text, never a fabricated value.

use std::time::Duration;

use bevy::app::{App, PostUpdate};
use bevy::asset::{AssetPlugin, Assets};
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{ScenePlugin, WorldSceneExt, bsn};
use bevy::text::{Font, TextFont};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::prelude::{Node, UiTransform, Val};
use bevy::ui::widget::Text;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessCategory, ProcessItem, ProcessLiveKey,
    ProcessMetadataObservation, ProcessScalarObservations,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::edit_processes;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::aggregate::row_scene;
use super::badge::{TreeAggregateBadge, TreeAggregateValue, TreeZombieBadge};
use super::chevron::{ChevronAnimations, TreeChevron};
use super::guide::TreeGuideColumn;
use super::hover::TreeHoverFrame;
use super::plan::GuideStroke;
use super::{ProcessTreeExpansion, ProcessTreeRowMarker, ProcessTreeRows};

/// The fixture snapshot timestamp of the rows below.
const SNAPSHOT_MS: u64 = 1;

/// The shared application identity of the fixture tree.
const FIXTURE_APP_ID: &str = "org.example.editor";

/// The application name the fixture tree is shown under.
const FIXTURE_APP_NAME: &str = "Fixture App";

/// One process with a typed identity, a parent link and the shared
/// application identity, so the fixture nests like a real application tree.
fn process(pid: u32, name: &str, parent: Option<u32>) -> ProcessItem {
    let identity = ProcessApplicationIdentity::new(FIXTURE_APP_ID, FIXTURE_APP_NAME, None)
        .expect("the fixture identity is non-empty");
    let mut item = ProcessItem::new(pid, name);
    item.parent_pid = parent;
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10 + 1, SNAPSHOT_MS),
        ..Default::default()
    });
    item.with_application_identity_observation(ProcessMetadataObservation::available(
        identity,
        SNAPSHOT_MS,
    ))
}

/// One defunct process: the shared status authority names it a zombie.
fn zombie(pid: u32, name: &str, parent: Option<u32>) -> ProcessItem {
    let mut item = process(pid, name, parent);
    item.status = "Z".to_owned();
    item
}

/// The live identity of one fixture process.
fn identity_of(item: &ProcessItem) -> ProcessLiveKey {
    ProcessLiveKey::from_process(item).expect("the fixture process has a live identity")
}

/// The theme palette the scenes are authored against.
fn palette() -> crate::palette::UiPalette {
    crate::palette::ui_palette(&Theme::default(), UiSize::Standard)
}

/// The expansion state that shows the whole fixture tree.
fn expanded_tree(items: &[ProcessItem]) -> ProcessTreeExpansion {
    let mut expansion = ProcessTreeExpansion::default();
    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(identity_of(&items[0]));
    expansion
}

/// The rows of a projected tree.
fn rows_of<'a>(
    items: &'a [ProcessItem],
    expansion: &ProcessTreeExpansion,
) -> Vec<super::ProcessTreeRowView<'a>> {
    super::project_items(items, expansion, SNAPSHOT_MS)
}

/// The number of mounted tree rows.
fn mounted_rows(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&ProcessTreeRowMarker>().iter(world).count()
}

/// A headless app that mounts scenes without a window.
fn scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

#[test]
fn a_nested_row_draws_one_guide_column_per_ancestor_level() {
    let items = vec![
        process(10, "editor", None),
        process(11, "editor-worker", Some(10)),
        process(12, "editor-helper", Some(10)),
    ];
    let expansion = expanded_tree(&items);
    let rows = rows_of(&items, &expansion);
    let guide_of = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .expect("the fixture row is projected")
            .guide
            .clone()
    };

    assert!(
        guide_of("Applications").is_empty(),
        "the category owns no nesting level of its own"
    );
    let application = guide_of(FIXTURE_APP_NAME);
    assert_eq!(
        application.len(),
        1,
        "the application row carries one level"
    );
    assert!(
        application[0].elbow,
        "the deepest column connects the row to its parent"
    );
    assert_eq!(
        application[0].stroke,
        GuideStroke::Half,
        "the last child closes its block at the row's middle"
    );

    let worker = guide_of("editor-worker");
    assert_eq!(
        worker.len(),
        3,
        "a grandchild carries the category, the application and its own level"
    );
    assert!(
        !worker[0].elbow && !worker[1].elbow && worker[2].elbow,
        "only the deepest column carries the row's own elbow"
    );
    assert_eq!(
        worker[0].stroke,
        GuideStroke::Full,
        "the ancestor's connector survives while its block continues below"
    );
    assert_eq!(
        worker[2].stroke,
        GuideStroke::Full,
        "a row with a following sibling keeps its connector open"
    );
    let helper = guide_of("editor-helper");
    assert_eq!(
        helper[2].stroke,
        GuideStroke::Half,
        "the last sibling closes its block"
    );

    // The production scene draws one column node per level.
    let palette = palette();
    let mut app = scene_app();
    for row in &rows {
        app.world_mut()
            .spawn_scene(row_scene(row, &palette))
            .expect("the production tree row scene resolves");
    }
    app.update();
    let world = app.world_mut();
    let columns: Vec<(usize, bool)> = world
        .query::<&TreeGuideColumn>()
        .iter(world)
        .map(|column| (column.index, column.elbow))
        .collect();
    assert_eq!(
        columns.len(),
        9,
        "the mounted rows draw one guide column per nesting level: {columns:?}"
    );
    assert_eq!(
        columns.iter().filter(|(_, elbow)| *elbow).count(),
        4,
        "exactly one column per nested row carries its elbow: {columns:?}"
    );
}

#[test]
fn the_disclosure_plate_turns_between_the_collapsed_and_expanded_angles() {
    let palette = palette();
    let mut app = scene_app();
    app.init_resource::<ChevronAnimations>();
    app.add_systems(PostUpdate, super::chevron::animate_chevrons);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    let entity = app
        .world_mut()
        .spawn_scene(super::chevron::chevron_scene("row", false, true, &palette))
        .expect("the chevron scene resolves")
        .id();
    app.update();
    let collapsed = plate_angle(&app, entity);
    assert!(
        collapsed < 0.0,
        "the collapsed plate rests pointing right: {collapsed}"
    );

    // The group opens: the plate turns instead of snapping.
    app.world_mut().entity_mut(entity).insert(TreeChevron {
        semantic_key: "row".to_owned(),
        expanded: true,
    });
    let mut turned = false;
    for _ in 0..12 {
        app.update();
        let angle = plate_angle(&app, entity);
        if angle > collapsed {
            turned = true;
        }
    }
    let settled = plate_angle(&app, entity);
    assert!(
        turned,
        "the plate moved between the two angles instead of jumping"
    );
    assert!(
        settled > collapsed,
        "the plate turned toward the expanded angle: {settled} vs {collapsed}"
    );
    assert!(
        settled.abs() < 0.01,
        "the plate rests at the expanded angle: {settled}"
    );
}

/// The angle of one mounted plate, in radians.
fn plate_angle(app: &App, entity: Entity) -> f32 {
    app.world()
        .get::<UiTransform>(entity)
        .expect("the plate carries a transform")
        .rotation
        .as_radians()
}

#[test]
fn a_group_row_states_its_total_with_a_badge_and_a_stronger_reading() {
    pin_english();
    let items = vec![process(10, "editor", None), process(11, "helper", Some(10))];
    let expansion = expanded_tree(&items);
    let rows = rows_of(&items, &expansion);
    let category = rows
        .iter()
        .find(|row| row.member_count == 2)
        .expect("the aggregated category is projected");

    let palette = palette();
    let mut app = crate::window::tests::scripted_frontend_app();
    app.world_mut()
        .spawn_scene(row_scene(category, &palette))
        .expect("the production tree row scene resolves");
    app.update();

    let world = app.world_mut();
    let badges: Vec<String> = world
        .query::<&TreeAggregateBadge>()
        .iter(world)
        .map(|badge| badge.0.clone())
        .collect();
    assert_eq!(
        badges,
        vec!["x2".to_owned()],
        "the aggregate row states how many processes it totals"
    );
    let weights: Vec<_> = world
        .query::<(&TreeAggregateValue, &TextFont)>()
        .iter(world)
        .map(|(_, font)| font.weight)
        .collect();
    assert_eq!(
        weights.len(),
        2,
        "both aggregate readings carry the emphasis marker"
    );
    assert!(
        weights
            .iter()
            .all(|weight| *weight == palette.heading.weight),
        "the aggregate readings wear the palette's medium-bold weight: {weights:?}"
    );
    assert!(
        palette.heading.weight > palette.body.weight,
        "the emphasis is stronger than an ordinary reading"
    );
}

#[test]
fn expanding_a_large_tree_mounts_its_rows_in_bounded_batches() {
    pin_english();
    let mut app = crate::window::tests::scripted_frontend_app();
    let mut items: Vec<ProcessItem> = Vec::new();
    for offset in 0..120u32 {
        items.push(process(40_000 + offset, &format!("worker-{offset}"), None));
    }
    let mut shell = ShellApp::new();
    edit_processes(&mut shell, |processes| {
        *processes = Some(items);
    });
    app.world_mut()
        .non_send_mut::<crate::app::FrontendTrack>()
        .shell = shell;
    app.update();
    app.update();

    let open = app
        .world_mut()
        .query::<(Entity, &super::TreeWindowControl)>()
        .iter(app.world())
        .find(|(_, control)| control.0)
        .map(|(entity, _)| entity)
        .expect("the normal tree entry is mounted");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: open });
    app.update();
    app.update();
    assert!(mounted_rows(&mut app) > 0, "the collapsed tree is mounted");

    let expand = app
        .world_mut()
        .query::<(Entity, &super::TreeExpansionControl)>()
        .iter(app.world())
        .find(|(_, control)| control.0 == super::TreeExpansionCommand::ExpandAll)
        .map(|(entity, _)| entity)
        .expect("the expand-all control is mounted");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: expand });

    let mut counts = Vec::new();
    for _ in 0..8 {
        app.update();
        counts.push(mounted_rows(&mut app));
    }
    let total = {
        let track = app.world().non_send::<crate::app::FrontendTrack>();
        let projection = track.shell.projection();
        super::project_items(
            projection.processes_slice(),
            &track.process_tree_expansion,
            projection.processes_observed_at_ms,
        )
        .len()
    };
    assert!(
        counts[0] < total,
        "the bulk expansion did not mount every row in one frame: {counts:?} of {total}"
    );
    assert!(
        counts.windows(2).all(|pair| pair[1] >= pair[0]),
        "the mounted rows never shrink while the batches drain: {counts:?}"
    );
    assert!(
        counts[1] > counts[0],
        "the batches keep mounting across frames: {counts:?}"
    );
    assert_eq!(
        counts.last(),
        Some(&total),
        "the batches drain to the whole tree: {counts:?}"
    );
}

#[test]
fn a_zombie_process_row_carries_the_status_badge() {
    pin_english();
    let items = vec![zombie(10, "defunct", None), process(11, "alive", Some(10))];
    let expansion = expanded_tree(&items);
    let rows = rows_of(&items, &expansion);
    let defunct = rows
        .iter()
        .find(|row| row.label == "defunct")
        .expect("the zombie row is projected");
    assert!(
        defunct
            .item
            .is_some_and(|item| item.status_kind().is_zombie()),
        "the shared status authority names the process a zombie"
    );

    let palette = palette();
    let mut app = scene_app();
    app.world_mut()
        .spawn_scene(row_scene(defunct, &palette))
        .expect("the production tree row scene resolves");
    app.update();
    let world = app.world_mut();
    let badges = world.query::<&TreeZombieBadge>().iter(world).count();
    assert_eq!(badges, 1, "the zombie row carries the status badge");
    let labels: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        labels.iter().any(|label| label == t("proc.status_zombie")),
        "the badge names the shared status: {labels:?}"
    );
}

#[test]
fn hovering_a_parent_row_outlines_the_rows_it_owns() {
    pin_english();
    let items = vec![
        process(10, "editor", None),
        process(11, "editor-worker", Some(10)),
        process(12, "editor-helper", Some(10)),
    ];
    let expansion = expanded_tree(&items);
    let rows = rows_of(&items, &expansion);
    let palette = palette();
    let mut app = scene_app();
    app.insert_resource(crate::window::WindowPalette {
        inner: palette.clone(),
    });
    app.add_systems(PostUpdate, super::hover::paint_hover_group);

    let root = app
        .world_mut()
        .spawn_scene(bsn! {
            Node { width: bevy::ui::prelude::percent(100.0) }
            ProcessTreeRows
        })
        .expect("the tree rows root resolves")
        .id();
    let mut row_entities: Vec<(String, Entity)> = Vec::new();
    for row in &rows {
        let child = app
            .world_mut()
            .spawn_scene(row_scene(row, &palette))
            .expect("the production tree row scene resolves")
            .id();
        app.world_mut().entity_mut(root).add_child(child);
        row_entities.push((row.label.clone(), child));
    }
    app.update();

    let entity_of = |label: &str| {
        row_entities
            .iter()
            .find(|(row_label, _)| row_label == label)
            .map(|(_, entity)| *entity)
            .expect("the fixture row is mounted")
    };
    let application = entity_of(FIXTURE_APP_NAME);
    let worker = entity_of("editor-worker");
    let category = entity_of("Applications");

    app.world_mut()
        .entity_mut(application)
        .insert(PickingInteraction::Hovered);
    app.update();

    let world = app.world_mut();
    let frames: Vec<(Entity, Color)> = world
        .query_filtered::<(&ChildOf, &bevy::ui::Outline), With<TreeHoverFrame>>()
        .iter(world)
        .map(|(parent, outline)| (parent.parent(), outline.color))
        .collect();
    assert_eq!(
        frames.len(),
        rows.len(),
        "every mounted row carries a hover frame: {frames:?}"
    );
    let color_of = |row: Entity| {
        frames
            .iter()
            .find(|(owner, _)| *owner == row)
            .map(|(_, color)| *color)
            .expect("the row owns a frame")
    };
    assert_ne!(
        color_of(worker),
        Color::NONE,
        "the hovered row's own subtree is marked"
    );
    assert_eq!(
        color_of(category),
        Color::NONE,
        "a row outside the hovered subtree is not marked"
    );
    assert_eq!(
        color_of(application),
        Color::NONE,
        "the hovered row itself is not marked"
    );
}

#[test]
fn the_collateral_tree_names_every_process_below_the_affected_root() {
    pin_english();
    let items = vec![
        process(10, "editor", None),
        process(11, "editor-worker", Some(10)),
        process(12, "editor-helper", Some(11)),
        process(20, "unrelated", None),
    ];
    let rows = super::collateral::collateral_rows(&items, identity_of(&items[0]));
    assert_eq!(
        rows.iter().map(|row| row.pid).collect::<Vec<_>>(),
        vec![10, 11, 12],
        "the collateral closure names the root and every process below it"
    );
    assert_eq!(
        rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "the collateral tree carries the nesting depth of every member"
    );

    let palette = palette();
    let mut app = scene_app();
    app.world_mut()
        .spawn_scene(super::collateral::collateral_scene(&rows, &palette))
        .expect("the collateral tree scene resolves");
    app.update();
    let world = app.world_mut();
    let heights: Vec<Val> = world
        .query_filtered::<&Node, With<super::collateral::TreeCollateralRow>>()
        .iter(world)
        .map(|node| node.height)
        .collect();
    assert_eq!(heights.len(), 3, "every member of the closure has a row");
    for height in &heights {
        let Val::Rem(value) = height else {
            panic!("the collateral row rides the rem axis: {height:?}");
        };
        assert!(
            *value < palette.control_height_rem,
            "the collateral rows are denser than the process rows: {value}"
        );
    }
}
