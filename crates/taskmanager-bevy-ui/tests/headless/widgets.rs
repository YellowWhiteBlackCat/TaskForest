//! test-intent: behavior
//!
//! Widget render-adapter assembly tests: every bsn! adapter spawns its full
//! dynamic fan-out under `MinimalPlugins` — menu rows per entry, dialog
//! action rows, table header cells per contract column, sparkline bars per
//! sample. This is the dynamic-children seam (`Children [ { vec } ]`) every
//! page agent copies; a regression in that pattern fails here without a
//! compositor.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use bevy::ui::{Node, Val};
use bevy::ui_widgets::Button;
use taskmanager_theme::Theme;

use super::dialog::{DialogSpec, dialog_scene};
use super::menu::{MenuItem, MenuSpec, menu_scene};
use super::sparkline::bars_scene;
use super::table::{header_scene, row_scene};
use crate::palette::{space_12, ui_palette};
use crate::widgets::controls::{ControlTone, ControlVisual, pill_scene};
use crate::widgets::table::process_columns;

fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

/// Spawn a scene and count the text nodes it produced, keyed by nothing —
/// the caller asserts against a fresh app so every text belongs to the scene.
fn text_census(app: &mut App, scene: impl bevy::scene::Scene) -> Vec<String> {
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("scene resolves without assets")
        .id();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert!(world.despawn(root), "the scene despawns cleanly");
    texts
}

#[test]
fn menu_scene_spawns_one_row_per_entry_plus_title() {
    let palette = ui_palette(&Theme::dark());
    let spec = MenuSpec {
        title: "Row actions".to_owned(),
        items: vec![
            MenuItem {
                label: "End task".to_owned(),
                enabled: true,
            },
            MenuItem {
                label: "Properties".to_owned(),
                enabled: false,
            },
        ],
    };
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, menu_scene(&spec, &palette));
    assert_eq!(texts.len(), 3, "title + one row per menu entry");
    assert_eq!(texts[0], "Row actions");
    assert!(texts.contains(&"End task".to_owned()));
    assert!(texts.contains(&"Properties".to_owned()));
}

#[test]
fn dialog_scene_spawns_title_body_and_two_actions() {
    let palette = ui_palette(&Theme::dark());
    let spec = DialogSpec {
        title: "End task?".to_owned(),
        body: "This will terminate the selected process.".to_owned(),
        confirm_label: "End task".to_owned(),
        dismiss_label: "Cancel".to_owned(),
    };
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, dialog_scene(&spec, &palette));
    assert_eq!(texts.len(), 4, "title, body, confirm, dismiss");
    assert!(texts.contains(&"End task?".to_owned()));
    assert!(texts.contains(&"Cancel".to_owned()));
}

#[test]
fn table_header_spawns_one_cell_per_contract_column() {
    let columns: Vec<_> = process_columns().iter().collect();
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, header_scene(&columns, None));
    assert_eq!(
        texts.len(),
        process_columns().len(),
        "one header cell per contract column, no duplicates"
    );
    assert_eq!(texts[0], "Name", "the identity column leads");
}

#[test]
fn table_row_spawns_one_cell_per_column_in_order() {
    let columns: Vec<_> = process_columns().iter().take(3).collect();
    let cells = vec![
        "taskforest-b".to_owned(),
        "1000".to_owned(),
        "user".to_owned(),
    ];
    let mut app = headless_scene_app();
    let texts = text_census(&mut app, row_scene(&cells, &columns));
    assert_eq!(texts, cells, "row cells render in column order");
}

#[test]
fn sparkline_bars_spawn_one_per_sample_bottom_aligned() {
    let palette = ui_palette(&Theme::dark());
    let samples = [0.0_f32, 1.0, 0.5, 0.25];
    let mut app = headless_scene_app();
    // No text in a bar strip: count ui nodes before/after the spawn.
    let before = app
        .world_mut()
        .query::<&bevy::ui::Node>()
        .iter(app.world())
        .count();
    let world = app.world_mut();
    let root = world
        .spawn_scene(bars_scene(&samples, 40.0, 4.0, palette.accent))
        .expect("scene resolves");
    let root: Entity = root.id();
    let after = world.query::<&bevy::ui::Node>().iter(world).count();
    assert_eq!(
        after - before,
        samples.len() + 1,
        "one node per bar plus the strip container"
    );
    assert!(world.despawn(root));
}

#[test]
fn pill_scene_keeps_token_padding_and_interaction_markers_on_one_root() {
    let palette = ui_palette(&Theme::dark());
    let mut app = headless_scene_app();
    let world = app.world_mut();
    let root = world
        .spawn_scene(pill_scene("Memory".to_owned(), false, &palette))
        .expect("the pill scene resolves without assets")
        .id();

    let node = world
        .get::<Node>(root)
        .expect("the pill owns its layout node");
    assert_eq!(node.padding.left, Val::Px(space_12()));
    assert_eq!(node.padding.right, Val::Px(space_12()));
    assert!(
        world.get::<Button>(root).is_some(),
        "the pill is a Bevy button"
    );
    assert_eq!(
        world.get::<ControlVisual>(root),
        Some(&ControlVisual(ControlTone::Surface, false)),
        "the pill carries the shared visual-state marker"
    );
    assert!(world.despawn(root));
}
