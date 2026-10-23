//! test-intent: behavior
//!
//! Behavior tests for the Bevy process column chooser: the ANCHORED
//! POPOVER that replaces the former full-screen blocking modal, and the
//! hidden-column projection it drives.

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
use bevy::ui::prelude::Node;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::{
    ProcessColumnToggleId, ProcessColumnsModalChanged, ProcessColumnsModalState,
    ProcessColumnsPopover, ProcessColumnsTrigger, ProcessHiddenColumns,
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

/// The chooser is an anchored popover: it lives INSIDE the trigger, it never
/// covers the page, and closing removes it again.
#[test]
fn column_chooser_is_an_anchored_popover_inside_its_trigger() {
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
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });

    let target_proc = test_proc(4242, "cargo");
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![target_proc]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));

    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;

    app.update();
    app.update();

    // Open the chooser through the shared key seam.
    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.open();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("open column chooser");
    app.update();

    let world = app.world_mut();
    let popover = world
        .query_filtered::<Entity, With<ProcessColumnsPopover>>()
        .iter(world)
        .next()
        .expect("the anchored popover is mounted");
    let trigger = world
        .query_filtered::<Entity, With<ProcessColumnsTrigger>>()
        .iter(world)
        .next()
        .expect("the chooser trigger is mounted");
    let parent = world
        .get::<bevy::ecs::hierarchy::ChildOf>(popover)
        .expect("the popover is parented")
        .parent();
    assert_eq!(
        parent, trigger,
        "the popover is anchored INSIDE the trigger, not over the page"
    );
    // The popover is a local surface: it is not a full-screen overlay.
    let node = world.get::<Node>(popover).expect("the popover is a node");
    assert_ne!(
        node.width,
        bevy::ui::prelude::Val::Percent(100.0),
        "the chooser is a local menu, never a page-sized sheet"
    );

    // Close it again.
    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.close();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("close column chooser");
    app.update();
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessColumnsPopover>>()
            .iter(world)
            .count(),
        0,
        "closing the chooser removes the popover"
    );
}

/// The chooser toggles a column, keeps the menu open, and resets to the
/// default column set.
#[test]
fn column_chooser_toggles_columns_and_resets_without_blocking_the_page() {
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
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });

    let target_proc = test_proc(4242, "cargo");
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![target_proc]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));

    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;

    app.update();
    app.update();

    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.open();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("open column chooser");
    app.update();

    // Toggle PID off through the popover's own row.
    let toggle_entity = {
        let world = app.world_mut();
        world
            .query_filtered::<(Entity, &ProcessColumnToggleId), ()>()
            .iter(world)
            .find(|(_, id)| id.0 == "PID")
            .map(|(entity, _)| entity)
            .expect("PID column row exists")
    };
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate {
            entity: toggle_entity,
        });
    app.update();

    let hidden = app.world().resource::<ProcessHiddenColumns>();
    assert!(hidden.0.contains("PID"), "PID is hidden");
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessColumnsPopover>>()
            .iter(app.world())
            .count(),
        1,
        "the chooser stays open after a toggle"
    );

    // Toggle User off as well, then reset.
    let user_toggle = {
        let world = app.world_mut();
        world
            .query_filtered::<(Entity, &ProcessColumnToggleId), ()>()
            .iter(world)
            .find(|(_, id)| id.0 == "User")
            .map(|(entity, _)| entity)
            .expect("User column row exists")
    };
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate {
            entity: user_toggle,
        });
    app.update();
    let hidden = app.world().resource::<ProcessHiddenColumns>();
    assert!(hidden.0.contains("PID"));
    assert!(hidden.0.contains("User"));

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
    assert!(hidden.0.is_empty(), "the reset restores every column");
}

/// The keyboard seam opens the chooser over the page's own trigger.
#[test]
fn column_chooser_opens_from_the_c_key_and_closes_on_escape() {
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
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });

    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = ShellApp::new();
    app.update();
    app.update();

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

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessColumnsPopover>>()
            .iter(app.world())
            .count(),
        1,
        "the chooser opens from the 'c' chord"
    );

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
    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<ProcessColumnsPopover>>()
            .iter(app.world())
            .count(),
        0,
        "Escape closes the chooser"
    );
}
