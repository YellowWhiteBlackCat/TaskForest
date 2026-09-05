use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use taskmanager_theme::Theme;
use taskmanager_ui_contract::{
    CapabilityStatus, CapabilitySupport, ComponentCapability, FrontendShape, capability_findings,
    capability_report,
};

use crate::palette::ui_palette;
use crate::widgets::controls::{
    MIN_SCROLLBAR_THUMB_PX, ScrollbarOrientation, SliderState, TooltipSpec,
    compute_scrollbar_geometry, scrollbar_scene, slider_scene, tooltip_scene,
};
use crate::widgets::menu::{DropdownMenuState, MenuItem, MenuSpec, dropdown_menu_scene};

fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

#[test]
fn capability_declaration_is_complete_and_has_no_reference_claim() {
    let declaration = crate::capabilities::capability_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);
    assert!(capability_findings(&declaration).is_empty());

    let report = capability_report(&declaration);
    assert_eq!(report.len(), ComponentCapability::ALL.len());
    assert!(report.iter().all(|(_, status)| matches!(
        status,
        CapabilityStatus::Declared(taskmanager_ui_contract::CapabilitySupport::Ported)
            | CapabilityStatus::Declared(taskmanager_ui_contract::CapabilitySupport::Native { .. })
            | CapabilityStatus::Declared(
                taskmanager_ui_contract::CapabilitySupport::Divergent { .. }
            )
            | CapabilityStatus::Declared(
                taskmanager_ui_contract::CapabilitySupport::Unsupported { .. }
            )
    )));
}

#[test]
fn bevy_ported_component_capabilities_surface_matches_contract() {
    let declaration = crate::capabilities::capability_declaration();

    let find_support = |cap: ComponentCapability| {
        declaration
            .entries
            .iter()
            .find(|e| e.capability == cap)
            .map(|e| e.support)
            .expect("capability must be declared")
    };

    assert_eq!(
        find_support(ComponentCapability::Tooltip),
        CapabilitySupport::Ported
    );
    assert_eq!(
        find_support(ComponentCapability::Slider),
        CapabilitySupport::Ported
    );
    assert_eq!(
        find_support(ComponentCapability::DropdownMenu),
        CapabilitySupport::Ported
    );
    assert_eq!(
        find_support(ComponentCapability::Scrollbar),
        CapabilitySupport::Ported
    );
}

#[test]
fn bevy_ported_component_scenes_assemble_and_spawn_in_world() {
    let palette = ui_palette(&Theme::dark());
    let mut app = headless_scene_app();
    let world = app.world_mut();

    // 1. Tooltip scene
    let tooltip_spec = TooltipSpec::new("Search").with_key_hint("Ctrl+F");
    let tip_root = world
        .spawn_scene(tooltip_scene(&tooltip_spec, &palette))
        .expect("tooltip scene resolves")
        .id();
    assert!(world.despawn(tip_root));

    // 2. Slider scene
    let slider_state = SliderState::new(0.0, 100.0, 10.0, 50.0);
    let slider_root = world
        .spawn_scene(slider_scene(&slider_state, &palette))
        .expect("slider scene resolves")
        .id();
    assert!(world.despawn(slider_root));

    // 3. DropdownMenu scene (both open and closed)
    let menu_spec = MenuSpec {
        title: "Actions".to_owned(),
        items: vec![MenuItem {
            label: "Run".to_owned(),
            enabled: true,
        }],
    };
    let mut dropdown_state = DropdownMenuState::default();
    let dd_closed = world
        .spawn_scene(dropdown_menu_scene(
            "Action".to_owned(),
            &menu_spec,
            &dropdown_state,
            &palette,
        ))
        .expect("closed dropdown scene resolves")
        .id();
    assert!(world.despawn(dd_closed));

    dropdown_state.open();
    let dd_open = world
        .spawn_scene(dropdown_menu_scene(
            "Action".to_owned(),
            &menu_spec,
            &dropdown_state,
            &palette,
        ))
        .expect("open dropdown scene resolves")
        .id();
    assert!(world.despawn(dd_open));

    // 4. Scrollbar scene (both vertical and horizontal)
    let scrollbar_geo =
        compute_scrollbar_geometry(100.0, 300.0, 50.0, 200.0, MIN_SCROLLBAR_THUMB_PX);
    let sb_vert = world
        .spawn_scene(scrollbar_scene(
            &scrollbar_geo,
            ScrollbarOrientation::Vertical,
            &palette,
        ))
        .expect("vertical scrollbar scene resolves")
        .id();
    assert!(world.despawn(sb_vert));

    let sb_horiz = world
        .spawn_scene(scrollbar_scene(
            &scrollbar_geo,
            ScrollbarOrientation::Horizontal,
            &palette,
        ))
        .expect("horizontal scrollbar scene resolves")
        .id();
    assert!(world.despawn(sb_horiz));
}
