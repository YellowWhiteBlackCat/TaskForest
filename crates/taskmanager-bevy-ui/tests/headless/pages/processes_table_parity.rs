//! test-intent: behavior
//!
//! The processes table's parity behaviors, one assertion per behavior:
//!
//! - the column drag resize really changes the column width;
//! - an overflowing cell ends with an ellipsis instead of a hard clip;
//! - the app icon flows INLINE with the process name;
//! - numeric values force the tabular figures;
//! - a double click selects a cell (visibly) and a secondary click copies the
//!   row summary;
//! - the dimmed zero-value ink clears the WCAG AA contrast ratio;
//! - the search match carries a background highlight;
//! - the context menu carries its layered drop shadow.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::math::Vec2;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, PointerClick, PointerDrag, PointerPress};
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::scene::ScenePlugin;
use bevy::text::{Font, FontFeatureTag, FontFeatures, TextFont};
use bevy::ui::prelude::{BackgroundColor, Node, TextBackgroundColor, Val};
use bevy::ui::widget::Text;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation, ProcessScalarObservations,
};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::LightDark;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::ProcessRowLink;
use crate::app::FrontendTrack;
use crate::palette::ui_palette;
use crate::widgets::table::contrast::{AA_CONTRAST_RATIO, contrast_ratio};
use crate::widgets::table::ellipsis::ELLIPSIS;
use crate::widgets::table::resize::{ColumnResizeHandle, ColumnWidthNode, ColumnWidthOverrides};
use crate::widgets::table::selection::CellSelection;
use crate::window::FrontendWindowPlugin;

// ---- fixtures -----------------------------------------------------------

fn process(pid: u32, name: &str) -> ProcessItem {
    let mut item = ProcessItem::new(pid, name);
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        ..ProcessScalarObservations::default()
    });
    item
}

fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}

/// The table app: the real window composition (fonts, palette, page mount)
/// plus the UI layout and the text pipeline, so the measured slot and the
/// shaped paragraph are the real ones.
fn table_app(shell: ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        bevy::input::InputPlugin,
        bevy::picking::PickingPlugin,
        bevy::picking::InteractionPlugin,
        bevy::ui::UiPlugin,
        bevy::text::TextPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.init_asset::<bevy::image::Image>();
    app.init_asset::<bevy::image::TextureAtlasLayout>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;
    app.update();
    // The headless composition has no window, so the camera's viewport is
    // what the UI layout measures against: give it a real one.
    {
        let world = app.world_mut();
        let mut cameras = world.query::<&mut bevy::camera::Camera>();
        for mut camera in cameras.iter_mut(world) {
            camera.viewport = Some(bevy::camera::Viewport {
                physical_size: bevy::math::UVec2::new(1280, 720),
                physical_position: bevy::math::UVec2::ZERO,
                ..Default::default()
            });
        }
    }
    app.update();
    app.update();
    app
}

/// A pointer sample at a logical position.
fn pointer_at(x: f32, y: f32) -> Pointer {
    Pointer::new(
        PointerId::Mouse,
        Location {
            target: bevy::camera::NormalizedRenderTarget::Window(
                bevy::window::WindowRef::Primary
                    .normalize(Some(Entity::PLACEHOLDER))
                    .expect("a normalized window reference"),
            ),
            position: Vec2::new(x, y),
        },
    )
}

fn hit_data() -> HitData {
    HitData {
        camera: Entity::PLACEHOLDER,
        depth: 0.0,
        position: None,
        normal: None,
        extra: None,
    }
}

/// The width a node carries, in logical pixels.
fn node_width(node: &Node) -> f32 {
    match node.width {
        Val::Px(value) => value,
        _ => 0.0,
    }
}

/// The header cell of one column and its text node: the header cells carry
/// the column marker but no cell selection (the body cells do).
fn header_cell(app: &mut App, column: &str) -> Option<(Entity, Entity)> {
    let world = app.world_mut();
    let mut found = None;
    for (cell, marker) in world.query::<(Entity, &ColumnWidthNode)>().iter(world) {
        if marker.0 != column {
            continue;
        }
        if world.get::<CellSelection>(cell).is_some() {
            continue;
        }
        let Some(children) = world.get::<Children>(cell) else {
            continue;
        };
        let Some(text) = children
            .iter()
            .find(|child| world.get::<Text>(**child).is_some())
        else {
            continue;
        };
        found = Some((cell, *text));
        break;
    }
    found
}

/// The body cell of one column and its text node: the body cells carry the
/// cell selection marker (the header cells do not).
fn body_cell(app: &mut App, column: &str) -> Option<(Entity, Entity)> {
    let world = app.world_mut();
    let mut found = None;
    for (cell, marker) in world.query::<(Entity, &ColumnWidthNode)>().iter(world) {
        if marker.0 != column {
            continue;
        }
        if world.get::<CellSelection>(cell).is_none() {
            continue;
        }
        let Some(children) = world.get::<Children>(cell) else {
            continue;
        };
        let Some(text) = children
            .iter()
            .find(|child| world.get::<Text>(**child).is_some())
        else {
            continue;
        };
        found = Some((cell, *text));
        break;
    }
    found
}

/// The complete text a cell renders: the root text plus its spans, in the
/// order the engine shapes them into one paragraph.
fn cell_text(app: &mut App, cell: Entity) -> String {
    let world = app.world_mut();
    let mut text = world
        .get::<Text>(cell)
        .map(|text| text.0.clone())
        .unwrap_or_default();
    if let Some(children) = world.get::<Children>(cell) {
        for child in children.iter() {
            if let Some(span) = world.get::<bevy::text::TextSpan>(*child) {
                text.push_str(&span.0);
            }
        }
    }
    text
}

// ---- BEVY-116: the column drag resize -----------------------------------

#[test]
fn dragging_the_header_gutter_changes_the_column_width() {
    let mut app = table_app(shell_with(vec![process(11, "alpha")]));
    let handle = {
        let world = app.world_mut();
        world
            .query_filtered::<(Entity, &ColumnResizeHandle), With<ColumnResizeHandle>>()
            .iter(world)
            .find(|(_, marker)| marker.0 == "PID")
            .map(|(entity, _)| entity)
            .expect("the PID header carries a resize gutter")
    };
    let (cell, _) = header_cell(&mut app, "PID").expect("the PID header cell");
    let before = node_width(
        app.world()
            .get::<Node>(cell)
            .expect("the header cell is a node"),
    );

    // Press the gutter, then drag it to the right: the first drag sample
    // anchors the session, the second one moves the column.
    app.world_mut().commands().trigger(PointerPress {
        entity: handle,
        pointer: pointer_at(100.0, 10.0),
        button: PointerButton::Primary,
        hit: hit_data(),
        count: 1,
    });
    app.update();
    app.world_mut().commands().trigger(PointerDrag {
        entity: handle,
        pointer: pointer_at(100.0, 10.0),
        button: PointerButton::Primary,
        distance: Vec2::ZERO,
        delta: Vec2::ZERO,
    });
    app.update();
    app.world_mut().commands().trigger(PointerDrag {
        entity: handle,
        pointer: pointer_at(160.0, 10.0),
        button: PointerButton::Primary,
        distance: Vec2::new(60.0, 0.0),
        delta: Vec2::new(60.0, 0.0),
    });
    app.update();

    let overrides = app.world().resource::<ColumnWidthOverrides>();
    let width = overrides
        .widths
        .get("PID")
        .copied()
        .expect("the drag committed a width for PID");
    assert!(
        width > before,
        "the dragged column is wider than it was ({width} vs {before})"
    );
    let cell_width = node_width(
        app.world()
            .get::<Node>(cell)
            .expect("the header cell is a node"),
    );
    assert_eq!(
        cell_width, width,
        "the live header cell carries the dragged width"
    );
}

// ---- BEVY-118: the overflow ellipsis ------------------------------------

#[test]
fn an_overflowing_cell_ends_with_an_ellipsis() {
    let long_name = "a-very-long-process-name-that-does-not-fit-the-name-column".repeat(4);
    let mut app = table_app(shell_with(vec![process(12, &long_name)]));
    // The layout settles over a few frames: the shaped width is measured
    // after the first layout, and the ellipsis width is measured on the pass
    // that follows it.
    for _ in 0..8 {
        app.update();
    }

    let (_, text_entity) = body_cell(&mut app, "Name").expect("the name cell");
    let text = cell_text(&mut app, text_entity);
    assert!(
        text.ends_with(ELLIPSIS),
        "the overflowing value ends with an ellipsis: {text:?}"
    );
    assert!(
        text.len() < long_name.len(),
        "the value was shortened instead of hard-clipped"
    );
}

// ---- BEVY-119: the inline app icon --------------------------------------

#[test]
fn the_app_icon_flows_inline_with_the_process_name() {
    let mut item = process(13, "cargo");
    let identity =
        ProcessApplicationIdentity::new("cargo.desktop", "Cargo", None).expect("a real desktop id");
    item.apply_application_identity(ProcessMetadataObservation::available(identity, 1));
    let mut app = table_app(shell_with(vec![item]));
    for _ in 0..3 {
        app.update();
    }

    let (cell, text_entity) = body_cell(&mut app, "Name").expect("the name cell");
    let world = app.world_mut();
    let children = world
        .get::<Children>(text_entity)
        .expect("the name text has children");
    let inline = children
        .iter()
        .find(|child| world.get::<bevy::text::InlineBox>(**child).is_some());
    assert!(
        inline.is_some(),
        "the app icon is an INLINE flow box inside the name text"
    );
    let siblings = world
        .get::<Children>(cell)
        .expect("the name cell has children");
    assert!(
        !siblings
            .iter()
            .any(|child| world.get::<bevy::text::InlineBox>(*child).is_some()),
        "the icon is not a flex sibling of the name text"
    );
}

// ---- BEVY-120: the tabular figures ---------------------------------------

#[test]
fn numeric_values_force_the_tabular_figures() {
    let mut item = process(14, "alpha");
    let mut scalars = *item.scalar_observations();
    scalars.cpu_percentage = ScalarObservation::available(12.5, 1);
    item.apply_scalar_observations(scalars);
    let mut app = table_app(shell_with(vec![item]));
    for _ in 0..3 {
        app.update();
    }

    let (_, text_entity) = body_cell(&mut app, "CPU").expect("the CPU cell");
    let font = app
        .world()
        .get::<TextFont>(text_entity)
        .expect("the value carries a font record");
    let expected = FontFeatures::builder()
        .enable(FontFeatureTag::TABULAR_FIGURES)
        .build();
    assert_eq!(
        font.font_features, expected,
        "the numeric value forces the tabular figures"
    );
}

// ---- BEVY-121: the cell selection and the clipboard export ---------------

#[test]
fn a_double_click_selects_a_cell_visibly_and_a_secondary_click_copies_the_row() {
    let mut app = table_app(shell_with(vec![process(15, "alpha")]));
    for _ in 0..3 {
        app.update();
    }

    let (cell, text_entity) = body_cell(&mut app, "Name").expect("the name cell");
    let cell_value = app
        .world()
        .get::<CellSelection>(cell)
        .expect("the cell carries its value")
        .content
        .clone();

    // A double click selects the cell.
    app.world_mut().commands().trigger(PointerClick {
        entity: text_entity,
        pointer: pointer_at(20.0, 30.0),
        button: PointerButton::Primary,
        hit: hit_data(),
        duration: std::time::Duration::from_millis(80),
        count: 2,
    });
    app.update();

    let selection = app
        .world()
        .resource::<crate::text_selection::TextSelectionState>();
    let session = selection
        .session
        .as_ref()
        .expect("the double click opened a selection");
    assert_eq!(
        session.content, cell_value,
        "the selection owns the cell value"
    );
    let background = app
        .world()
        .get::<BackgroundColor>(cell)
        .expect("the cell carries a surface");
    assert_ne!(
        background.0,
        bevy::color::Color::NONE,
        "the selected cell shows the selection surface"
    );

    // A secondary click on the row copies the row summary.
    let row = app
        .world_mut()
        .query_filtered::<Entity, With<ProcessRowLink>>()
        .iter(app.world())
        .next()
        .expect("the row wrapper is mounted");
    app.world_mut().commands().trigger(PointerPress {
        entity: row,
        pointer: pointer_at(20.0, 30.0),
        button: PointerButton::Secondary,
        hit: hit_data(),
        count: 1,
    });
    app.update();

    let world = app.world_mut();
    let track = world.non_send_mut::<FrontendTrack>();
    assert_eq!(
        track.shell.selected, 0,
        "the secondary click targets the row under the pointer"
    );
    let notice = track
        .shell
        .feedback_notice()
        .expect("the copy reported through the shared feedback channel");
    assert!(
        notice.text().contains("Selected Row"),
        "the row summary copy was requested: {}",
        notice.text()
    );
}

// ---- BEVY-122: the zero-value contrast calibration ----------------------

#[test]
fn the_dimmed_zero_value_ink_clears_the_aa_contrast_ratio() {
    // A dim ink that is too dark for its surface: the calibration has to mix
    // it toward the body ink until the pair clears AA.
    let surface = bevy::color::Color::srgba(0.11, 0.11, 0.13, 1.0);
    let body = bevy::color::Color::srgba(1.0, 1.0, 1.0, 1.0);
    let dim = bevy::color::Color::srgba(0.16, 0.16, 0.18, 1.0);
    assert!(
        contrast_ratio(dim, surface) < AA_CONTRAST_RATIO,
        "the fixture's dim ink really is below AA"
    );
    let calibrated = crate::widgets::table::contrast::calibrate_muted_ink(dim, body, &[surface]);
    assert!(
        contrast_ratio(calibrated, surface) >= AA_CONTRAST_RATIO,
        "the calibrated ink clears AA on its surface"
    );
    assert_ne!(
        calibrated, dim,
        "the calibration changed the ink that was below AA"
    );

    // The live skins are calibrated too.
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let surfaces = [palette.content_bg, palette.nav_active_bg];
    let calibrated = crate::widgets::table::contrast::calibrate_muted_ink(
        palette.dim_color,
        palette.body_color,
        &surfaces,
    );
    for surface in surfaces {
        let ratio = contrast_ratio(calibrated, surface);
        assert!(
            ratio >= AA_CONTRAST_RATIO,
            "the dimmed ink clears AA on the row surface ({ratio} < {AA_CONTRAST_RATIO})"
        );
    }
    // The light skin is calibrated too.
    let light = ui_palette(
        &crate::pages::settings::theme_for_mode(LightDark::Light),
        UiSize::Standard,
    );
    let light_surfaces = [light.content_bg, light.nav_active_bg];
    let light_ink = crate::widgets::table::contrast::calibrate_muted_ink(
        light.dim_color,
        light.body_color,
        &light_surfaces,
    );
    for surface in light_surfaces {
        let ratio = contrast_ratio(light_ink, surface);
        assert!(
            ratio >= AA_CONTRAST_RATIO,
            "the dimmed ink clears AA on the light row surface ({ratio})"
        );
    }
}

// ---- BEVY-123: the search-match highlight --------------------------------

#[test]
fn the_search_match_carries_a_background_highlight() {
    let mut app = table_app(shell_with(vec![process(16, "cargo-build")]));
    for _ in 0..3 {
        app.update();
    }

    // The page's own query seam drives the shell's query and the rebuild.
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<super::ProcessRowsRoot>>()
        .iter(app.world())
        .next()
        .expect("the rows root is mounted");
    app.world_mut()
        .commands()
        .trigger(super::ProcessQueryCommit {
            entity: root,
            text: "car".to_owned(),
        });
    for _ in 0..4 {
        app.update();
    }

    let (_, text_entity) = body_cell(&mut app, "Name").expect("the name cell");
    // The span list is copied out: the world borrow must end before the
    // palette is read.
    let children: Vec<Entity> = {
        let world = app.world_mut();
        world
            .get::<Children>(text_entity)
            .expect("the name text has children")
            .to_vec()
    };
    let highlighted = {
        let world = app.world_mut();
        children.iter().any(|child| {
            world.get::<bevy::text::TextSpan>(*child).is_some()
                && world
                    .get::<TextBackgroundColor>(*child)
                    .is_some_and(|background| background.0 != bevy::color::Color::NONE)
        })
    };
    assert!(
        highlighted,
        "the matched characters carry a background highlight"
    );

    // The matched run also wears the theme's own highlight INK. The window
    // layer's custom-ink seam keeps the declared ink: the role observer
    // owns the font metrics, but it must not stamp the role ink over a
    // scene that declared its own (BEVY-187).
    let highlight_ink = {
        let palette = app
            .world()
            .resource::<crate::window::WindowPalette>()
            .inner
            .highlight_fg;
        let world = app.world_mut();
        children.iter().any(|child| {
            world.get::<bevy::text::TextSpan>(*child).is_some()
                && world
                    .get::<TextBackgroundColor>(*child)
                    .is_some_and(|background| background.0 != bevy::color::Color::NONE)
                && world
                    .get::<bevy::text::TextColor>(*child)
                    .is_some_and(|ink| ink.0 == palette)
        })
    };
    assert!(
        highlight_ink,
        "the matched characters wear the theme's highlight ink, not the role ink"
    );
}

// ---- BEVY-124: the context menu shadow -----------------------------------

#[test]
fn the_context_menu_carries_a_layered_drop_shadow() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let shadow = crate::widgets::menu::menu_shadow(&palette);
    assert_eq!(
        shadow.len(),
        2,
        "the menu shadow has an ambient and a contact layer"
    );
    assert!(
        shadow[0].blur_radius != shadow[1].blur_radius,
        "the two shadow layers are not the same shadow twice"
    );
}
