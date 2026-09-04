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
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
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
    // The details panel cannot read the keyboard: it learns that the shell
    // selection changed through the typed refresh signal. The shell remains
    // the identity authority and the cursor assertion below proves the landed
    // process.
    #[derive(Resource, Default)]
    struct SelectionLog(usize);

    fn record_selection(
        _change: bevy::ecs::observer::On<crate::pages::processes::ProcessSelectionChanged>,
        mut log: ResMut<SelectionLog>,
    ) {
        log.0 += 1;
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
    assert_eq!(
        app.world().resource::<SelectionLog>().0,
        1,
        "the details seam receives one typed selection refresh signal"
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

#[test]
fn the_process_action_chord_opens_the_applications_menu_and_commits_through_the_batch_track() {
    use crate::menu_modal::MenuModal;
    use crate::pages::processes::menu::ProcessMenuCtx;

    let mut app = input_app(shell_with_selection());
    app.update();
    app.update();

    // `a` is the TUI's frontend-local action-menu chord on Applications.
    press(&mut app, KeyCode::KeyA, Some("a"));
    app.update();
    assert!(
        app.world()
            .resource::<MenuModal<ProcessMenuCtx>>()
            .session
            .is_some(),
        "the Applications action menu opens on the frontend-local chord"
    );

    // Down Down Enter picks Suspend (index 2): one marked row, a
    // non-destructive verb — the batch track submits straight into the
    // pending-effect bridge, so the drain submits it the same frame tail.
    for key in [KeyCode::ArrowDown, KeyCode::ArrowDown, KeyCode::Enter] {
        app.world_mut()
            .resource_mut::<bevy::input::ButtonInput<KeyCode>>()
            .clear();
        press(&mut app, key, None);
        app.update();
    }
    assert!(
        app.world()
            .resource::<MenuModal<ProcessMenuCtx>>()
            .session
            .is_none(),
        "a committed menu closes"
    );
    let effects = app.world().resource::<PendingEffects>().0.clone();
    let Some(taskmanager_application::PlatformEffect::ExecuteBatch(intent)) = effects.first()
    else {
        panic!("the menu's verb crosses the effect bridge, got {effects:?}");
    };
    assert_eq!(
        intent.action,
        taskmanager_core::core::process::ProcessBatchAction::Suspend
    );
}

#[test]
fn escape_clears_active_feedback_notice_when_no_modal_is_open() {
    let mut shell = shell_with_selection();
    shell.report_notice(
        taskmanager_shell::FeedbackSource::Interaction,
        taskmanager_shell::FeedbackSeverity::Info,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        "Screenshot saved to /tmp/screenshot.png",
    );
    assert!(shell.feedback_notice().is_some());

    let mut app = input_app(shell);
    app.update();
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_some()
    );

    // Press Escape to dismiss the notice.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "pressing Esc must clear active feedback notice when no modal is open"
    );
    assert_ne!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_text(),
        "Screenshot saved to /tmp/screenshot.png"
    );
}

#[test]
fn escape_dismisses_armed_gate_first_before_clearing_feedback_notice() {
    let mut shell = shell_with_selection();
    shell.report_notice(
        taskmanager_shell::FeedbackSource::Interaction,
        taskmanager_shell::FeedbackSeverity::Info,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        "Active notice across modal",
    );

    let mut app = input_app(shell);
    app.update();
    app.update();

    // Arm confirmation gate via Delete.
    press(&mut app, KeyCode::Delete, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .confirmation_kind()
            .is_some(),
        "gate must be armed"
    );
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_some(),
        "feedback notice must be present"
    );

    // First Esc dismisses the confirmation gate.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .confirmation_kind()
            .is_none(),
        "gate must be dismissed by first Esc"
    );
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_some(),
        "notice must NOT be cleared when Esc was consumed to dismiss the gate"
    );

    // Second Esc clears the feedback notice.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "second Esc must clear the feedback notice"
    );
}

#[test]
fn escape_cancels_action_menu_first_before_clearing_feedback_notice() {
    use crate::menu_modal::MenuModal;
    use crate::pages::processes::menu::ProcessMenuCtx;

    let mut shell = shell_with_selection();
    shell.report_notice(
        taskmanager_shell::FeedbackSource::Interaction,
        taskmanager_shell::FeedbackSeverity::Info,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        "Active notice across menu modal",
    );

    let mut app = input_app(shell);
    app.update();
    app.update();

    // Open process menu via 'a'.
    press(&mut app, KeyCode::KeyA, Some("a"));
    app.update();

    assert!(
        app.world()
            .resource::<MenuModal<ProcessMenuCtx>>()
            .session
            .is_some(),
        "process menu modal must be open"
    );
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_some()
    );

    // First Esc cancels the menu modal.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        !app.world()
            .resource::<MenuModal<ProcessMenuCtx>>()
            .session
            .is_some(),
        "menu modal must be closed by first Esc"
    );
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_some(),
        "notice must NOT be cleared when Esc was consumed to cancel the menu"
    );

    // Second Esc clears the feedback notice.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "second Esc must clear the feedback notice"
    );
}

#[test]
fn escape_clears_feedback_notice_and_fires_feedback_changed() {
    use std::sync::{Arc, Mutex};
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let mut shell = shell_with_selection();
    shell.report_notice(
        taskmanager_shell::FeedbackSource::Interaction,
        taskmanager_shell::FeedbackSeverity::Info,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        "Active notice to clear",
    );

    let mut app = input_app(shell);
    app.add_observer(
        move |event: bevy::ecs::observer::On<crate::drain::FeedbackChanged>| {
            received_clone.lock().unwrap().push(event.event().0.clone());
        },
    );

    app.update();
    app.update();

    // Press Escape.
    press(&mut app, KeyCode::Escape, None);
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "notice must be cleared"
    );

    let events = received.lock().unwrap().clone();
    assert!(
        events.iter().any(|text| text != "Active notice to clear"),
        "FeedbackChanged with empty feedback must have fired: {events:?}"
    );
}
