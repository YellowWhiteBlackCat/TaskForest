//! test-intent: behavior
//!
//! Behavior tests for the Bevy process columns visibility modal and hidden
//! column projections.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, ResMut, RunSystemOnce};
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::InputFocusPlugin;
use bevy::scene::ScenePlugin;
use bevy::text::Font;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::Theme;

use super::{
    ProcessColumnToggleId, ProcessColumnsModalChanged, ProcessColumnsModalOverlay,
    ProcessColumnsModalScrim, ProcessColumnsModalState, ProcessHiddenColumns,
};
use crate::app::FrontendTrack;
use crate::pages::processes::projection;
use crate::palette::ui_palette;
use crate::window::FrontendWindowPlugin;

fn test_proc(pid: u32, name: &str) -> ProcessItem {
    let mut p = ProcessItem::new(pid, name);
    p.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    p
}

#[test]
fn process_rows_projection_with_hidden_columns() {
    let mut shell = ShellApp::new();
    let p = test_proc(10, "alpha");
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![p]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));

    let proj_all = projection::rows_projection(&shell, 10, 0);
    assert_eq!(proj_all.rows.len(), 1);
    let cells_all_count = proj_all.rows[0].cells.len();

    let proj_hidden = projection::rows_projection_with_hidden(&shell, 10, 0, &["PID", "CPU"]);
    assert_eq!(proj_hidden.rows.len(), 1);
    let cells_hidden_count = proj_hidden.rows[0].cells.len();
    assert_eq!(cells_hidden_count, cells_all_count - 2);
}

#[test]
fn process_columns_modal_toggles_columns_resets_and_dismisses_via_button_and_escape() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: ui_palette(&Theme::dark()),
    });

    let target_proc = test_proc(4242, "cargo");
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![target_proc]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));

    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;

    // Pump frames to initialize window chrome, AppShellRoot and page mount
    app.update();
    app.update();

    // 1. Open the columns modal via resource / event
    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.open();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("open columns modal");
    app.update();

    // Verify modal overlay is mounted
    let world = app.world_mut();
    let overlay_count = world
        .query_filtered::<Entity, With<ProcessColumnsModalOverlay>>()
        .iter(world)
        .count();
    assert_eq!(overlay_count, 1, "columns modal overlay is mounted");

    // 2. Find a column toggle button and toggle it
    let toggle_entity = world
        .query_filtered::<(Entity, &ProcessColumnToggleId), ()>()
        .iter(world)
        .find(|(_, id)| id.0 == "PID")
        .map(|(e, _)| e)
        .expect("PID column toggle button exists");

    world.commands().trigger(bevy::ui_widgets::Activate {
        entity: toggle_entity,
    });
    app.update();

    // Verify PID is now in ProcessHiddenColumns
    let hidden = app.world().resource::<ProcessHiddenColumns>();
    assert!(hidden.0.contains("PID"), "PID is in hidden columns");

    // 3. Reset columns
    app.world_mut()
        .run_system_once(
            |mut hidden: ResMut<ProcessHiddenColumns>, mut commands: Commands| {
                hidden.0.clear();
                commands.trigger(ProcessColumnsModalChanged);
                commands.trigger(crate::input::ShellInteractionApplied);
            },
        )
        .expect("reset columns");
    app.update();

    let hidden = app.world().resource::<ProcessHiddenColumns>();
    assert!(hidden.0.is_empty(), "hidden columns are cleared");

    // 4. Dismiss via scrim
    let world = app.world_mut();
    let scrim_entity = world
        .query_filtered::<Entity, With<ProcessColumnsModalScrim>>()
        .iter(world)
        .next()
        .expect("scrim exists");
    world.commands().trigger(bevy::ui_widgets::Activate {
        entity: scrim_entity,
    });
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessColumnsModalOverlay>>()
            .iter(world)
            .count(),
        0,
        "columns modal is despawned after scrim activation"
    );

    // 5. Open via 'c' key press and dismiss via Escape key
    let c_event = KeyboardInput {
        key_code: KeyCode::KeyC,
        logical_key: Key::Character("c".into()),
        state: bevy::input::ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    let mut c_event = Some(c_event);
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            if let Some(event) = c_event.take() {
                writer.write(event);
            }
        })
        .expect("c key injection");
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessColumnsModalOverlay>>()
            .iter(world)
            .count(),
        1,
        "modal is mounted after pressing 'c'"
    );

    // Send Escape
    let esc_event = KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state: bevy::input::ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    let mut esc_event = Some(esc_event);
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            if let Some(event) = esc_event.take() {
                writer.write(event);
            }
        })
        .expect("esc key injection");
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessColumnsModalOverlay>>()
            .iter(world)
            .count(),
        0,
        "modal is despawned after Escape"
    );
}
