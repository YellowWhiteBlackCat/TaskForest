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
use bevy::input_focus::{FocusCause, InputFocus, InputFocusPlugin};
use bevy::scene::ScenePlugin;
use bevy::text::Font;
use bevy::ui::prelude::Node;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;
use taskmanager_ui_contract::{COLUMN_WIDTH_KEYBOARD_STEP, COLUMN_WIDTH_MAX, COLUMN_WIDTH_MIN};

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
    assert!(
        world
            .get::<bevy::ui_widgets::popover::Popover>(popover)
            .is_some(),
        "the chooser mounts the official bevy_ui_widgets Popover"
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

/// The chooser traps focus in its own modal tab group, closes when focus
/// leaves the popover, and returns focus to the trigger.
#[test]
fn column_chooser_closes_on_focus_loss_and_restores_trigger_focus() {
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

    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.open();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("open column chooser");
    app.update();
    app.update();
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
    let focused = world
        .resource::<InputFocus>()
        .get()
        .expect("the chooser takes focus when it mounts");
    let mut current = focused;
    let mut inside_popover = false;
    for _ in 0..16 {
        if current == popover {
            inside_popover = true;
            break;
        }
        let Some(parent) = world.get::<bevy::ecs::hierarchy::ChildOf>(current) else {
            break;
        };
        current = parent.parent();
    }
    assert!(
        inside_popover && world.get::<ProcessColumnToggleId>(focused).is_some(),
        "the first focusable chooser row receives focus when the popover mounts: {focused:?}"
    );
    assert!(
        world
            .get::<bevy::input_focus::tab_navigation::TabGroup>(popover)
            .is_some(),
        "the popover is its own modal tab group"
    );

    world
        .run_system_once(move |mut focus: ResMut<InputFocus>| {
            focus.set(trigger, FocusCause::Navigated);
        })
        .expect("move focus out of the chooser");
    app.update();
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world.resource::<ProcessColumnsModalState>().session,
        None,
        "focus loss closes the chooser"
    );
    assert_eq!(
        world
            .query_filtered::<Entity, With<ProcessColumnsPopover>>()
            .iter(world)
            .count(),
        0,
        "the closed popover is removed"
    );
    assert_eq!(
        world.resource::<InputFocus>().get(),
        Some(trigger),
        "focus returns to the chooser trigger"
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

/// Keyboard width stepping: `-`/`=` on the focused column row moves the one
/// width override authority and the live cells together, with the popover
/// staying open.
#[test]
fn column_chooser_keyboard_steps_the_focused_column_width() {
    use crate::widgets::table::resize::{ColumnWidthNode, ColumnWidthOverrides};

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
    app.world_mut()
        .run_system_once(
            |mut state: ResMut<ProcessColumnsModalState>, mut commands: Commands| {
                state.open();
                commands.trigger(ProcessColumnsModalChanged);
            },
        )
        .expect("open column chooser");
    app.update();
    app.update();

    let world = app.world_mut();
    let focused = world
        .resource::<InputFocus>()
        .get()
        .expect("the chooser takes focus when it mounts");
    let column = world
        .get::<ProcessColumnToggleId>(focused)
        .expect("focus lands on a column row")
        .0;
    let start = world
        .query::<(&ColumnWidthNode, &Node)>()
        .iter(world)
        .find(|(marker, _)| marker.0 == column)
        .map(|(_, node)| match node.width {
            bevy::ui::prelude::Val::Px(width) => width,
            _ => panic!("the column cell carries a pixel width"),
        })
        .expect("the column has a live cell");

    let mut expected = start;
    for (code, direction) in [
        (KeyCode::Minus, -1.0_f32),
        (KeyCode::Equal, 1.0),
        (KeyCode::Equal, 1.0),
        (KeyCode::Minus, -1.0),
    ] {
        let event = KeyboardInput {
            key_code: code,
            logical_key: Key::Character("x".into()),
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        app.world_mut()
            .commands()
            .trigger(bevy::input_focus::FocusedInput::new(
                focused,
                event,
                Entity::PLACEHOLDER,
            ));
        app.update();
        expected = (expected + direction * COLUMN_WIDTH_KEYBOARD_STEP)
            .clamp(COLUMN_WIDTH_MIN, COLUMN_WIDTH_MAX);

        let world = app.world_mut();
        let width = world
            .resource::<ColumnWidthOverrides>()
            .widths
            .get(column)
            .copied()
            .expect("the keyboard step committed a width");
        assert_eq!(
            width, expected,
            "the shared keyboard step is applied to the focused column"
        );
        let live = world
            .query::<(&ColumnWidthNode, &Node)>()
            .iter(world)
            .find(|(marker, _)| marker.0 == column)
            .map(|(_, node)| match node.width {
                bevy::ui::prelude::Val::Px(width) => width,
                _ => panic!("the column cell carries a pixel width"),
            })
            .expect("the column has a live cell");
        assert_eq!(live, width, "the painted cell follows the override");
        assert!(
            world
                .query_filtered::<Entity, With<ProcessColumnsPopover>>()
                .iter(world)
                .count()
                == 1,
            "the chooser stays open across a width step"
        );
    }
    assert_eq!(expected, start, "the stepper is reversible");
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
