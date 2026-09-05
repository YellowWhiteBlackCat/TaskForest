//! test-intent: behavior
//!
//! Unit and scene assembly tests for the ported DropdownMenu:
//! - DropdownMenuState: open/close toggle, keyboard advance (confirm to open, cancel to dismiss, item confirmation).
//! - dropdown_menu_scene: trigger button render when closed, anchored popup assembly when open.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_theme::Theme;

use super::{DropdownMenuState, MenuInput, MenuItem, MenuOutcome, MenuSpec, dropdown_menu_scene};
use crate::palette::ui_palette;

fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

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

fn test_items() -> Vec<MenuItem> {
    vec![
        MenuItem {
            label: "Option A".to_owned(),
            enabled: true,
        },
        MenuItem {
            label: "Option B (Disabled)".to_owned(),
            enabled: false,
        },
        MenuItem {
            label: "Option C".to_owned(),
            enabled: true,
        },
    ]
}

#[test]
fn dropdown_menu_state_transitions_follow_interactions() {
    let mut state = DropdownMenuState::default();
    assert!(!state.is_open(), "dropdown is initially closed");

    state.toggle();
    assert!(state.is_open(), "toggle opens dropdown");

    state.toggle();
    assert!(!state.is_open(), "toggle closes dropdown");

    let items = test_items();

    // While closed, navigation keys do not open
    assert_eq!(state.advance(&items, MenuInput::Down), None);
    assert!(!state.is_open());

    // Confirm while closed opens the dropdown
    assert_eq!(state.advance(&items, MenuInput::Confirm), None);
    assert!(state.is_open(), "confirm opens closed dropdown");

    // While open, Up/Down navigates without closing
    assert_eq!(state.advance(&items, MenuInput::Down), None);
    assert!(state.is_open());
    assert_eq!(state.menu_state.selection, 1);

    // Confirm on disabled item does not close
    assert_eq!(state.advance(&items, MenuInput::Confirm), None);
    assert!(state.is_open(), "disabled item keeps dropdown open");

    // Navigate to next enabled item and confirm
    state.advance(&items, MenuInput::Down);
    assert_eq!(state.menu_state.selection, 2);
    assert_eq!(
        state.advance(&items, MenuInput::Confirm),
        Some(MenuOutcome::Confirmed(2))
    );
    assert!(!state.is_open(), "confirmed action closes dropdown");

    // Re-open and test cancel
    state.open();
    assert!(state.is_open());
    assert_eq!(
        state.advance(&items, MenuInput::Cancel),
        Some(MenuOutcome::Canceled)
    );
    assert!(!state.is_open(), "cancel closes dropdown");
}

#[test]
fn dropdown_menu_scene_spawns_trigger_and_conditionally_mounts_popup() {
    let palette = ui_palette(&Theme::dark());
    let spec = MenuSpec {
        title: "Actions".to_owned(),
        items: test_items(),
    };
    let mut state = DropdownMenuState::default();
    let mut app = headless_scene_app();

    // Closed: only trigger label and chevron
    let texts_closed = text_census(
        &mut app,
        dropdown_menu_scene("Select Action".to_owned(), &spec, &state, &palette),
    );
    assert_eq!(
        texts_closed.len(),
        1,
        "only trigger label is a text node when closed"
    );
    assert_eq!(texts_closed[0], "Select Action");

    // Open: trigger label, popup title, and 3 menu items
    state.open();
    let texts_open = text_census(
        &mut app,
        dropdown_menu_scene("Select Action".to_owned(), &spec, &state, &palette),
    );
    assert_eq!(texts_open.len(), 5, "trigger text + popup title + 3 items");
    assert_eq!(texts_open[0], "Select Action");
    assert_eq!(texts_open[1], "Actions");
    assert!(texts_open.contains(&"Option A".to_owned()));
    assert!(texts_open.contains(&"Option B (Disabled)".to_owned()));
    assert!(texts_open.contains(&"Option C".to_owned()));
}
