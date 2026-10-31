//! test-intent: behavior
//!
//! Headless behavior tests for the action menu's ANCHORED POPOVER surface
//! (BEVY-188): the menu is a CHILD of its trigger — never a page-sized sheet
//! over a full-screen catcher — Escape closes it, a press OUTSIDE reaches
//! the background and closes the menu in the same frame, and a picked verb
//! still commits through the shared typed path.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, ResMut, RunSystemOnce};
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::InputFocusPlugin;
use bevy::math::Vec2;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, PointerPress};
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::scene::{ScenePlugin, WorldSceneExt, bsn};
use bevy::text::Font;
use bevy::ui::prelude::{Node, Val};
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessBatchAction, ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage, PendingConfirmation};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use crate::app::FrontendTrack;
use crate::menu_modal::{MenuModalChanged, MenuModalPopover, MenuModalTrigger};
use crate::pages::processes::menu::{ProcessMenuCtx, ProcessMenuModal, open_for_selected};
use crate::palette::ui_palette;
use crate::widgets::menu::MenuItemIndex;
use crate::window::{AppShellRoot, FrontendWindowPlugin};

// ---- fixtures -----------------------------------------------------------

/// The background surface behind the menu: a plain node that records the
/// presses it receives, so a test can prove the outside press was NOT
/// swallowed by a catcher.
#[derive(Component, Clone, Copy, Default)]
struct Background;

/// Every press that reached the background surface.
#[derive(Resource, Default)]
struct BackgroundPresses(Vec<Entity>);

/// Observer: the background records the press it receives.
fn on_background_pressed(
    press: On<PointerPress>,
    backgrounds: Query<(), With<Background>>,
    mut presses: ResMut<BackgroundPresses>,
) {
    if backgrounds.contains(press.entity) {
        presses.0.push(press.entity);
    }
}

/// The headless composition the frontend ships: the window plugin brings the
/// palette, the shell track and the window shell root.
fn headless_app() -> App {
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
    app.init_resource::<BackgroundPresses>();
    app.add_observer(on_background_pressed);
    app
}

/// A process whose provider-native start token is available, so the menu can
/// freeze a live identity (`ProcessLiveKey`).
fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

/// The headless app with one selected process row in the shell.
fn shelved_app() -> App {
    let mut app = headless_app();
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |shelved| {
        *shelved = Some(vec![token_process(100, "alpha")]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(shell.select_row(0));
    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;
    app.update();
    app.update();
    app
}

/// The control the menu floats against: a real node carrying the trigger
/// marker, mounted under the window shell.
fn mount_trigger(app: &mut App) -> Entity {
    let world = app.world_mut();
    let root = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .next()
        .expect("the window shell root is mounted");
    let trigger = world
        .spawn_scene(bsn! {
            Node { width: Val::Auto, height: Val::Auto }
            MenuModalTrigger
        })
        .expect("the trigger scene mounts")
        .id();
    let mut commands = world.commands();
    crate::scene_attach::attach_child(&mut commands, root, trigger);
    // The page publishes its anchor into the menu's own state; the test does
    // the same, through the same path a page uses.
    world
        .resource_mut::<ProcessMenuModal>()
        .set_trigger(trigger);
    trigger
}

/// The background surface behind the menu, mounted under the window shell.
fn mount_background(app: &mut App) -> Entity {
    let world = app.world_mut();
    let root = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .iter(world)
        .next()
        .expect("the window shell root is mounted");
    let background = world
        .spawn_scene(bsn! {
            Node { width: Val::Auto, height: Val::Auto }
            Background
        })
        .expect("the background scene mounts")
        .id();
    let mut commands = world.commands();
    crate::scene_attach::attach_child(&mut commands, root, background);
    background
}

/// Open the action menu for the shell's selected row.
fn open_menu(app: &mut App) {
    app.world_mut()
        .run_system_once(
            |mut modal: ResMut<ProcessMenuModal>,
             track: NonSend<FrontendTrack>,
             mut commands: Commands| {
                assert!(open_for_selected(&mut modal, &track.shell));
                commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
                    true,
                    std::marker::PhantomData,
                ));
            },
        )
        .expect("open menu");
    app.update();
}

/// The one mounted popover.
fn mounted_popover(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<MenuModalPopover>>()
        .iter(world)
        .next()
        .expect("the anchored popover is mounted")
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

/// The picking hit of a press over the window.
fn hit_data() -> HitData {
    HitData {
        camera: Entity::PLACEHOLDER,
        depth: 0.0,
        position: None,
        normal: None,
        extra: None,
    }
}

// ---- the anchored popover ----------------------------------------------

/// The inventory page publishes the control the menu floats against: the
/// trigger IS the page's inventory block, so the popover is that block's
/// child and the table behind it stays live.
#[test]
fn the_inventory_page_publishes_the_control_the_menu_floats_against() {
    let mut app = headless_app();
    app.world_mut().insert_resource(crate::app::Route {
        page: crate::app::Page::Services,
    });
    app.update();
    app.update();

    let world = app.world_mut();
    let trigger = world
        .query_filtered::<Entity, With<MenuModalTrigger>>()
        .iter(world)
        .next()
        .expect("the page published the control the menu floats against");
    let surface = world
        .query_filtered::<Entity, With<crate::pages::services::ServicesPageRoot>>()
        .iter(world)
        .next()
        .expect("the services page surface is mounted");
    assert_eq!(
        trigger, surface,
        "the menu floats against the page surface, not a page-sized sheet"
    );
    let block = world
        .query_filtered::<Entity, With<crate::pages::services::ServicesBody>>()
        .iter(world)
        .next()
        .expect("the services inventory block is mounted");
    assert_ne!(
        trigger, block,
        "the trigger is never the inventory block the paint replaces"
    );
}

/// The open menu survives the page's own rebuild: the popover hangs off a
/// surface the paint never replaces.
#[test]
fn the_open_menu_survives_the_pages_own_rebuild() {
    let mut app = shelved_app();
    open_menu(&mut app);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        1,
        "the menu is open"
    );

    // The page rebuilds its own rows.
    app.world_mut()
        .commands()
        .trigger(crate::input::ShellInteractionApplied);
    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        1,
        "the menu survives the page's own rebuild"
    );
}

/// The menu is an ANCHORED POPOVER: it is a CHILD of its trigger, not a
/// page-sized sheet over a full-screen catcher.
#[test]
fn the_action_menu_is_an_anchored_popover_inside_its_trigger() {
    let mut app = shelved_app();
    let trigger = mount_trigger(&mut app);
    open_menu(&mut app);

    let popover = mounted_popover(&mut app);
    let world = app.world_mut();
    let parent = world
        .get::<ChildOf>(popover)
        .expect("the popover is parented")
        .parent();
    assert_eq!(
        parent, trigger,
        "the menu is anchored INSIDE its trigger, not over the page"
    );
    let node = world.get::<Node>(popover).expect("the popover is a node");
    assert_ne!(
        node.width,
        Val::Percent(100.0),
        "the menu is a local popover, never a page-sized sheet"
    );
    assert_ne!(
        node.height,
        Val::Percent(100.0),
        "the menu never covers the inventory behind it"
    );
}

/// Escape closes the open menu: the popover is retired with the session.
#[test]
fn escape_closes_the_anchored_popover() {
    let mut app = shelved_app();
    mount_trigger(&mut app);
    open_menu(&mut app);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        1,
        "the menu is open"
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
        .run_system_once(
            move |mut writer: bevy::ecs::message::MessageWriter<KeyboardInput>| {
                if let Some(event) = esc_event.take() {
                    writer.write(event);
                }
            },
        )
        .expect("escape key injection");
    app.update();
    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        0,
        "Escape closes the menu and retires the popover"
    );
    assert!(
        app.world_mut()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "a cancelled menu arms nothing"
    );
}

/// A press OUTSIDE the menu reaches the background — and the menu closes in
/// the same frame. There is no catcher between the two.
#[test]
fn an_outside_press_reaches_the_background_and_closes_the_menu() {
    let mut app = shelved_app();
    mount_trigger(&mut app);
    let background = mount_background(&mut app);
    open_menu(&mut app);

    app.world_mut().commands().trigger(PointerPress {
        entity: background,
        pointer: pointer_at(20.0, 30.0),
        button: PointerButton::Primary,
        hit: hit_data(),
        count: 1,
    });
    app.update();
    app.update();

    let presses = app.world().resource::<BackgroundPresses>();
    assert!(
        presses.0.contains(&background),
        "the outside press reaches the background target, as the reference surface does"
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        0,
        "the menu closes when the press lands outside it"
    );
    assert!(
        app.world_mut()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "a dismissed menu arms nothing"
    );
}

/// The picked verb still commits through the shared typed path: the menu is
/// a popover, not a different command surface.
#[test]
fn a_picked_verb_still_commits_through_the_shared_typed_path() {
    let mut app = shelved_app();
    mount_trigger(&mut app);
    open_menu(&mut app);

    // Pick 4 is Kill: the destructive verb arms the shared batch gate.
    let kill_item = {
        let world = app.world_mut();
        world
            .query_filtered::<(Entity, &MenuItemIndex), ()>()
            .iter(world)
            .find(|(_, item)| item.0 == 4)
            .map(|(entity, _)| entity)
            .expect("the kill menu item exists")
    };
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: kill_item });
    app.update();
    app.update();

    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(world)
            .count(),
        0,
        "a committed menu retires its popover"
    );
    let track = world.non_send::<FrontendTrack>();
    let Some(PendingConfirmation::ProcessBatch(intent)) = track.shell.pending_confirmation() else {
        panic!("the picked verb arms the shared confirmation gate");
    };
    assert_eq!(intent.action, ProcessBatchAction::Kill);
}

/// The menu refuses to swallow the keys it does not use: an open menu owns
/// the keyboard chord-for-chord, exactly like the former modal did.
#[test]
fn an_open_menu_owns_the_keyboard_without_committing_anything() {
    let mut app = shelved_app();
    mount_trigger(&mut app);
    open_menu(&mut app);

    let arrow_event = KeyboardInput {
        key_code: KeyCode::ArrowDown,
        logical_key: Key::ArrowDown,
        state: bevy::input::ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    let mut arrow_event = Some(arrow_event);
    app.world_mut()
        .run_system_once(
            move |mut writer: bevy::ecs::message::MessageWriter<KeyboardInput>| {
                if let Some(event) = arrow_event.take() {
                    writer.write(event);
                }
            },
        )
        .expect("arrow key injection");
    app.update();

    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        1,
        "navigating the menu keeps it open"
    );
    assert!(
        app.world_mut()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "moving the cursor arms nothing"
    );
}

/// A dismissed menu is retired at the END of the frame: the surface is
/// hidden first, so no observer of this frame ever meets a vanished child.
#[test]
fn a_dismissed_menu_is_hidden_before_it_is_despawned() {
    let mut app = shelved_app();
    mount_trigger(&mut app);
    open_menu(&mut app);
    let popover = mounted_popover(&mut app);

    app.world_mut()
        .run_system_once(
            |mut modal: ResMut<ProcessMenuModal>,
             mut track: NonSendMut<FrontendTrack>,
             mut commands: Commands| {
                let mut effects = Vec::new();
                assert!(
                    modal.drive(&mut track.shell, KeyCode::Escape, &mut effects),
                    "an open menu consumes the key"
                );
                commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
                    false,
                    std::marker::PhantomData,
                ));
            },
        )
        .expect("dismiss menu");
    app.world_mut().flush();

    // The surface is hidden at once, so no observer of this frame meets a
    // vanishing child.
    let visibility = app
        .world()
        .get::<bevy::prelude::Visibility>(popover)
        .expect("the retired popover still exists this frame");
    assert_eq!(
        *visibility,
        bevy::prelude::Visibility::Hidden,
        "the retired surface is hidden before it leaves"
    );

    // And it is despawned at the end of the frame.
    app.update();
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<MenuModalPopover>>()
            .iter(app.world())
            .count(),
        0,
        "the retired surface leaves with the frame"
    );
}
