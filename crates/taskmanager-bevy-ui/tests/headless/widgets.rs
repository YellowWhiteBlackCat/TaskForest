//! test-intent: behavior
//!
//! Widget render-adapter assembly tests: every bsn! adapter spawns its full
//! dynamic fan-out under `MinimalPlugins` — menu rows per entry and table
//! header/row cells per contract column. This is the dynamic-children seam
//! (`Children [ { vec } ]`) every page agent copies; a regression in that
//! pattern fails here without a compositor.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin, Assets};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::Has;
use bevy::image::Image;
use bevy::picking::Pickable;
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::text::{InlineBox, InlineBoxKind};
use bevy::ui::widget::{InlineImage, Text};
use bevy::ui::{Node, Val};
use bevy::ui_widgets::Button;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, Theme};
use taskmanager_ui_contract::IconId;

use super::menu::{MenuItem, MenuSpec, MenuState, menu_scene_at};
use super::table::{SortProjection, header_scene, row_scene};
use crate::icons::{IconPlates, inline_icon_scene, stamp_inline_icon};
use crate::palette::{space_12, ui_palette};
use crate::widgets::controls::{ControlTone, ControlVisual, control_background, pill_scene};
use crate::widgets::table::visible_columns;

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
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
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
    let texts = text_census(
        &mut app,
        menu_scene_at(&spec, &MenuState::default(), &palette),
    );
    assert_eq!(texts.len(), 3, "title + one row per menu entry");
    assert_eq!(texts[0], "Row actions");
    assert!(texts.contains(&"End task".to_owned()));
    assert!(texts.contains(&"Properties".to_owned()));
}

#[test]
fn table_header_spawns_one_cell_per_contract_column() {
    let columns: Vec<_> = visible_columns(&[]);
    let mut app = headless_scene_app();
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let texts = text_census(&mut app, header_scene(&columns, None, &palette));
    assert_eq!(
        texts.len(),
        visible_columns(&[]).len(),
        "one header cell per contract column, no duplicates"
    );
    assert_eq!(texts[0], "Name", "the identity column leads");
}

#[test]
fn sorted_header_flows_the_direction_plate_inside_the_label_text() {
    let columns: Vec<_> = visible_columns(&[]);
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let sort = Some(SortProjection {
        column: "CPU",
        descending: false,
    });
    let mut app = headless_scene_app();
    let world = app.world_mut();
    world
        .spawn_scene(header_scene(&columns, sort, &palette))
        .expect("the header scene resolves without assets");

    let mut texts = world.query::<(&Text, &Children)>();
    let label_children: Vec<bevy::ecs::entity::Entity> = texts
        .iter(world)
        .find(|(text, _)| text.0 == "CPU")
        .expect("the sorted column's label spawns")
        .1
        .iter()
        .copied()
        .collect();

    let mut inline = world.query::<(&InlineBox, Option<&InlineImage>)>();
    let flow: Vec<_> = label_children
        .iter()
        .filter_map(|child| inline.get(world, *child).ok())
        .collect();
    assert_eq!(flow.len(), 1, "exactly one direction plate rides the label");
    let (box_, stamped) = flow[0];
    assert_eq!(
        box_.kind,
        InlineBoxKind::InFlow,
        "the plate takes space in the text layout, not out-of-flow"
    );
    assert!(
        box_.size.x > 0.0 && box_.size.y > 0.0,
        "the plate reserves real space"
    );
    assert!(
        stamped.is_none(),
        "the bitmap handle is async payload: the declared scene never fabricates one"
    );
}

#[test]
fn inline_icon_stamps_a_typed_bitmap_payload_on_the_declared_flow_box() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut app = headless_scene_app();
    app.add_observer(stamp_inline_icon);
    app.init_asset::<Image>();
    let (plates, handle) = {
        let world = app.world_mut();
        let mut images = world.resource_mut::<Assets<Image>>();
        let plates = IconPlates::build(&mut images);
        let handle = plates.handle(IconId::Cpu).expect("the Cpu plate decodes");
        (plates, handle)
    };
    app.insert_resource(plates);

    let world = app.world_mut();
    let root = world
        .spawn_scene(inline_icon_scene(
            IconId::Cpu,
            &palette,
            bevy::color::Color::WHITE,
        ))
        .expect("the inline icon scene resolves")
        .id();
    app.update();

    let world = app.world_mut();
    let (box_, image, pickable) = world
        .query::<(&InlineBox, &InlineImage, Has<Pickable>)>()
        .get(world, root)
        .expect("the inline icon entity survives the stamp");
    assert_eq!(box_.kind, InlineBoxKind::InFlow);
    assert_eq!(image.image, handle, "the semantic plate handle is stamped");
    assert_eq!(
        image.width,
        Some(palette.icon_size_px()),
        "the extent follows the palette icon step"
    );
    assert_eq!(image.height, Some(palette.icon_size_px()));
    assert!(pickable, "the flow box never intercepts button clicks");
}

#[test]
fn table_row_spawns_one_cell_per_column_in_order() {
    let columns: Vec<_> = visible_columns(&[]).into_iter().take(3).collect();
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
fn pill_scene_keeps_token_padding_and_interaction_markers_on_one_root() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
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

#[test]
fn table_rows_read_through_the_canvas_and_alternate_the_theme_zebra() {
    // The reference surface's row contract: an even unselected row is open
    // (the page canvas reads through it), an odd one wears the theme's zebra
    // tint, selection and hover own the two interaction tints. A row is never
    // a solid control panel — that flattens the whole table into one block.
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let idle = PickingInteraction::None;
    let open = control_background(
        &ControlVisual(ControlTone::Row { zebra: false }, false),
        idle,
        false,
        &palette,
    );
    let striped = control_background(
        &ControlVisual(ControlTone::Row { zebra: true }, false),
        idle,
        false,
        &palette,
    );
    assert_eq!(open, bevy::color::Color::NONE, "an even row stays open");
    assert_eq!(striped, palette.zebra_bg, "an odd row carries the stripe");
    assert!(
        striped.to_srgba().alpha >= 1.0,
        "the stripe is pre-composited, not a second translucent veil"
    );
    let selected = control_background(
        &ControlVisual(ControlTone::Row { zebra: true }, true),
        idle,
        false,
        &palette,
    );
    assert_eq!(
        selected, palette.selection_bg,
        "selection wins over the stripe"
    );
    let hovered = control_background(
        &ControlVisual(ControlTone::Row { zebra: false }, false),
        PickingInteraction::Hovered,
        false,
        &palette,
    );
    assert_eq!(hovered, palette.hover_bg, "hover wins over the stripe");
}

#[test]
fn an_atomic_card_keeps_the_shared_glass_fill() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let idle = PickingInteraction::None;
    let card = control_background(
        &ControlVisual(ControlTone::Card, false),
        idle,
        false,
        &palette,
    );
    assert_eq!(
        card,
        palette.glass_fill(GlassTier::Card),
        "the control skin does not erase a card's painted glass surface"
    );
    let selected = control_background(
        &ControlVisual(ControlTone::Card, true),
        idle,
        false,
        &palette,
    );
    assert_eq!(selected, palette.selection_bg, "selection wins on a card");
}
