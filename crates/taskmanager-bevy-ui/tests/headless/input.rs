//! test-intent: behavior
//!
//! Headless behavior tests for the real-input seam (`src/input.rs`).
//!
//! Every test drives the REAL `InputPlugin` composition on `MinimalPlugins`:
//! Bevy `KeyboardInput` messages are written into the world, `InputPlugin`
//! folds them into `ButtonInput<KeyCode>` on the next update, and the
//! keyboard adapter forwards them through the shell's own routers. The
//! assertions are on shell state, the route protocol, the effect bridge, and
//! the one-shot quit forward — never on source text.

use bevy::MinimalPlugins;
use bevy::app::{App, AppExit, Startup, Update};
use bevy::asset::AssetPlugin;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageReader;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::RunSystemOnce;
use bevy::ecs::system::{Commands, ResMut};
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input_focus::InputFocusPlugin;
use bevy::scene::ScenePlugin;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_theme::Theme;

use super::{PendingEffects, QuitForwarded};
use crate::app::{AppShellPlugin, ContentSlot, FrontendTrack, Page, Route};
use crate::pages::history::HistoryProjectionResource;
use crate::window::WindowPalette;

// ---- fixtures -----------------------------------------------------------

/// A process whose provider-native start token is available, so the shared
/// gate can freeze an authoritative identity (`FrozenProcessIdentity`).
fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

fn shell_with_selection() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| {
        *processes = Some(vec![
            token_process(100, "alpha"),
            token_process(200, "beta"),
        ])
    });
    let _ = shell.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Applications,
    ));
    shell
}

/// The real `AppShellPlugin` composition (route, input adapter, mount chain)
/// without any window. The content slot exists from `Startup` so the chained
/// mount system finds it, exactly as the window shell guarantees.
fn input_app(shell: ShellApp) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.add_plugins(AppShellPlugin);
    app.insert_resource(WindowPalette {
        inner: crate::palette::ui_palette(&Theme::dark()),
    });
    app.init_resource::<HistoryProjectionResource>();
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
    });
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn(ContentSlot);
    });
    app
}

/// Write one key press into the world's message queue. `logical_key` only
/// matters for text-bearing keys; textless chords carry an unidentified
/// logical key exactly like a real keyboard backend would.
fn press(app: &mut App, key: KeyCode, text: Option<&str>) {
    let event = KeyboardInput {
        key_code: key,
        logical_key: text.map_or(Key::Unidentified(NativeKey::Unidentified), |t| {
            Key::Character(t.into())
        }),
        state: bevy::input::ButtonState::Pressed,
        text: text.map(Into::into),
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    let mut event = Some(event);
    app.world_mut()
        .run_system_once(move |mut writer: MessageWriter<KeyboardInput>| {
            if let Some(event) = event.take() {
                writer.write(event);
            }
        })
        .expect("the injection system runs");
}

#[test]
fn delete_arms_the_shared_gate_and_y_confirms_to_the_end_task_effect() {
    let mut app = input_app(shell_with_selection());
    app.update();
    app.update();

    press(&mut app, KeyCode::Delete, None);
    app.update();
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(
        matches!(
            shell.pending_confirmation(),
            Some(taskmanager_application::PendingConfirmation::EndTask(_))
        ),
        "Delete must arm the shared end-task gate, got {:?}",
        shell.confirmation_kind()
    );
    assert!(
        app.world().resource::<PendingEffects>().0.is_empty(),
        "arming emits no platform effect"
    );

    press(&mut app, KeyCode::KeyY, Some("y"));
    app.update();
    let effects = &app.world().resource::<PendingEffects>().0;
    assert!(
        matches!(
            effects.as_slice(),
            [taskmanager_application::PlatformEffect::EndTask(_)]
        ),
        "the gate confirm must re-emit the frozen end-task effect, got {effects:?}"
    );
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(
        shell.pending_confirmation().is_none(),
        "a confirmed gate is closed"
    );
}

#[test]
fn arrows_move_the_shell_cursor_and_the_details_seam_follows() {
    // The details panel cannot read the keyboard: it learns the landed row
    // only through the published selection identity. The recorder proves the
    // arrow press actually tells it which process is on the cursor.
    #[derive(Resource, Default)]
    struct SelectionLog(Vec<Option<crate::pages::processes::ProcessRowIdentity>>);

    fn record_selection(
        change: bevy::ecs::observer::On<crate::pages::processes::ProcessSelectionChanged>,
        mut log: ResMut<SelectionLog>,
    ) {
        log.0.push(change.event().0.clone());
    }

    let mut app = input_app(shell_with_selection());
    app.init_resource::<SelectionLog>();
    app.add_observer(record_selection);
    app.update();
    app.update();

    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert_eq!(shell.selected, 1, "arrow down moves the shell cursor");
    let landed = app
        .world()
        .resource::<SelectionLog>()
        .0
        .last()
        .cloned()
        .flatten()
        .expect("the move publishes the selection identity");
    assert_eq!(
        (landed.pid, landed.name.as_str()),
        (200, "beta"),
        "the details seam learns the process the cursor landed on"
    );
}

#[test]
fn search_typing_folds_into_the_shell_query_and_p_cannot_steal_the_route() {
    let mut app = input_app(shell_with_selection());
    app.update();
    app.update();

    press(&mut app, KeyCode::ControlLeft, None);
    press(&mut app, KeyCode::KeyF, None);
    app.update();
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(shell.search_active(), "Ctrl+F opens the shell search");
    // Release the chord: `reset_all` lifts the held ControlLeft (`clear`
    // only resets the just_* sets).
    app.world_mut()
        .resource_mut::<bevy::input::ButtonInput<KeyCode>>()
        .reset_all();

    press(&mut app, KeyCode::KeyP, Some("p"));
    app.update();
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert_eq!(shell.query, "p", "typing folds into the query");
    assert!(
        shell.search_active(),
        "the search surface still owns the keyboard"
    );
    let route = app.world().resource::<Route>();
    assert_eq!(
        route.page,
        Page::Processes,
        "the settings chord must not steal a key from the search owner"
    );
}

#[test]
fn navigation_chords_are_swallowed_while_a_gate_is_armed() {
    let mut app = input_app(shell_with_selection());
    app.update();
    app.update();

    press(&mut app, KeyCode::Delete, None);
    app.update();
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_some()
    );

    // Alt+2 is a plain navigation chord while free, but an armed gate owns
    // the keyboard: the route must not move and the gate must stay armed.
    app.world_mut()
        .resource_mut::<bevy::input::ButtonInput<KeyCode>>()
        .clear();
    press(&mut app, KeyCode::AltLeft, None);
    press(&mut app, KeyCode::Digit2, None);
    app.update();
    let route = app.world().resource::<Route>();
    assert_eq!(route.page, Page::Processes, "no navigation while armed");
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(shell.pending_confirmation().is_some(), "gate stays armed");
}

#[test]
fn escape_dismisses_the_gate_without_an_effect() {
    let mut app = input_app(shell_with_selection());
    app.update();
    app.update();

    press(&mut app, KeyCode::Delete, None);
    app.update();
    press(&mut app, KeyCode::Escape, None);
    app.update();
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(shell.pending_confirmation().is_none(), "Escape dismisses");
    assert!(
        app.world().resource::<PendingEffects>().0.is_empty(),
        "dismissal never submits"
    );
}

#[test]
fn quit_reason_forwards_app_exit_exactly_once() {
    /// Counts `AppExit` messages the runner would observe.
    #[derive(Resource, Default)]
    struct ExitCount(usize);

    fn count_exits(mut reader: MessageReader<AppExit>, mut count: ResMut<ExitCount>) {
        for _ in reader.read() {
            count.0 += 1;
        }
    }

    let mut shell = shell_with_selection();
    shell.request_quit(taskmanager_shell::QuitReason::Keyboard);
    let mut app = input_app(shell);
    app.init_resource::<ExitCount>();
    // No ordering against the adapter: the buffered message is read within
    // this frame or the next, and each message is counted exactly once.
    app.add_systems(Update, count_exits);
    app.update();
    app.update();
    app.update();

    let count = app.world().resource::<ExitCount>();
    assert_eq!(count.0, 1, "the shell quit decision forwards exactly once");
    assert!(app.world().resource::<QuitForwarded>().0);
}
