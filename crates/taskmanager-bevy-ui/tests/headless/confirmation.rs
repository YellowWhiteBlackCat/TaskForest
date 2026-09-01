//! test-intent: behavior
//!
//! Headless behavior tests for the confirmation surface
//! (`src/confirmation.rs`): the REAL window composition on `MinimalPlugins`
//! arms the shell gate through the real input seam, and the assertions cover
//! the whole vertical slice — one modal mounted under the shell root with the
//! frozen target echoed in its body, the confirm button re-emitting the
//! frozen end-task effect, dismissal producing nothing, and the feedback line
//! reflecting the submission outcome.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::input_focus::InputFocusPlugin;
use bevy::scene::ScenePlugin;
use bevy::ui::widget::Text;
use taskmanager_application::{AppAction, AppPage, ConfirmationKind};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessItem,
    ProcessScalarObservations,
};

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_shell::presentation::process_batch_action_label;
use taskmanager_theme::Theme;

use super::{ArmedConfirmation, ConfirmationOverlay, PendingConfirmationView};
use crate::app::FrontendTrack;
use crate::input::PendingEffects;
use crate::window::{FeedbackLine, FrontendWindowPlugin};

// ---- fixtures -----------------------------------------------------------

fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

fn frozen_identity(pid: u32, name: &str) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, name, 1, 1)
        .expect("a fixture identity with an authoritative token")
}

/// The production window composition (drain included) with the demo no-I/O
/// runtime handle — the same plugin seam the real launcher uses, minus the
/// winit window.
fn window_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        AssetPlugin::default(),
        ScenePlugin,
        InputPlugin,
        InputFocusPlugin,
    ));
    app.init_resource::<Assets<bevy::text::Font>>();
    app.add_plugins(FrontendWindowPlugin {
        runtime: crate::runtime::demo_platform_runtime(),
        palette: crate::palette::ui_palette(&Theme::dark()),
    });
    app
}

/// Seed one token-bearing process into the live track and route the shell to
/// the Applications page so the shared Delete chord is in scope.
fn seed_processes(app: &mut App) {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| {
        *processes = Some(vec![token_process(4242, "cargo")]);
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    let mut track = app
        .world_mut()
        .get_non_send_mut::<FrontendTrack>()
        .expect("the window plugin installed the track");
    track.shell = shell;
}

/// Write one key press into the world's message queue (same injection seam
/// the input-seam tests use).
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

fn overlay_despawned(app: &mut App) -> bool {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<ConfirmationOverlay>>()
        .iter(world)
        .next()
        .is_none()
}

fn button_entity(app: &mut App, marker: &str) -> Entity {
    let world = app.world_mut();
    if marker == "confirm" {
        world
            .query_filtered::<Entity, With<super::ConfirmChoice>>()
            .iter(world)
            .next()
            .expect("the confirm button exists")
    } else {
        world
            .query_filtered::<Entity, With<super::DismissChoice>>()
            .iter(world)
            .next()
            .expect("the dismiss button exists")
    }
}

fn activate(app: &mut App, entity: Entity) {
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity });
}

// ---- tests ---------------------------------------------------------------

#[test]
fn the_armed_view_echoes_the_frozen_target() {
    let pending =
        taskmanager_application::PendingConfirmation::EndTask(frozen_identity(4242, "cargo"));
    let view =
        PendingConfirmationView::from_pending(&pending).expect("EndTask renders a dialog view");
    assert!(
        view.body.contains("cargo") && view.body.contains("4242"),
        "the user must read the name and pid they are about to end: {}",
        view.body
    );

    // The frozen-set key is an identity, not a serialization detail: the
    // same target set armed in a different order must produce the SAME key
    // (an arm/refresh race cannot forge a second identity), and a different
    // set must produce a DIFFERENT key (a stale dialog cannot be mistaken
    // for the armed one).
    let freeze = |pids: [u32; 2]| {
        let intent = ProcessBatchIntent {
            action: ProcessBatchAction::Kill,
            scope: Default::default(),
            targets: pids
                .iter()
                .map(|&pid| frozen_identity(pid, "worker"))
                .collect(),
        };
        PendingConfirmationView::from_pending(
            &taskmanager_application::PendingConfirmation::ProcessBatch(intent),
        )
        .expect("a frozen batch renders a dialog view")
        .target_key
    };
    assert_eq!(
        freeze([900, 300]),
        freeze([300, 900]),
        "arm order cannot change the frozen-set identity"
    );
    assert_ne!(
        freeze([900, 300]),
        freeze([900, 301]),
        "a different target set is a different identity"
    );
}

#[test]
fn batch_confirmation_names_the_requested_action() {
    for action in [
        ProcessBatchAction::End,
        ProcessBatchAction::EndProcessTree,
        ProcessBatchAction::Kill,
        ProcessBatchAction::Suspend,
        ProcessBatchAction::Resume,
        ProcessBatchAction::SetPriority(taskmanager_core::core::process::PriorityTier::High),
    ] {
        let intent = ProcessBatchIntent {
            action,
            scope: Default::default(),
            targets: vec![frozen_identity(4242, "worker")],
        };
        let pending = taskmanager_application::PendingConfirmation::ProcessBatch(intent);
        let view = PendingConfirmationView::from_pending(&pending)
            .expect("every process batch action has a confirmation view");
        let expected = process_batch_action_label(action);
        assert!(
            view.body.contains(&expected),
            "confirmation body must name {expected:?}: {}",
            view.body
        );
        if action != ProcessBatchAction::Kill {
            assert!(
                !view
                    .body
                    .contains(taskmanager_application::i18n::t("proc.kill")),
                "non-kill action {expected:?} must not use kill copy: {}",
                view.body
            );
        }
    }
}

#[test]
fn delete_arms_the_modal_and_confirm_completes_the_submission_loop() {
    let mut app = window_app();
    seed_processes(&mut app);
    app.update();
    app.update();

    press(&mut app, KeyCode::Delete, None);
    app.update();
    assert!(!overlay_despawned(&mut app), "the modal mounts");
    {
        let world = app.world_mut();
        let armed = world
            .query_filtered::<&ArmedConfirmation, With<ConfirmationOverlay>>()
            .iter(world)
            .next()
            .expect("the overlay carries the armed view")
            .0
            .clone()
            .expect("the armed view is present");
        assert_eq!(
            armed.kind,
            ConfirmationKind::EndTask,
            "Delete arms the shared end-task gate"
        );
    }

    let confirm = button_entity(&mut app, "confirm");
    activate(&mut app, confirm);
    // The queued trigger dispatches at the next sync point, and the drain
    // submits the re-emitted effect through the shared queue_effect seam in
    // the same frame — so the honest observables are the closed gate, the
    // despawned modal, and the reported outcome on the feedback line.
    app.update();
    app.update();
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "confirm closes the gate"
    );
    assert!(overlay_despawned(&mut app), "a confirmed modal despawns");
    let world = app.world_mut();
    let feedback = world
        .query_filtered::<&Text, With<FeedbackLine>>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(feedback.len(), 1);
    assert!(
        !feedback[0].is_empty(),
        "the submitted effect is reported on the feedback line"
    );
}

#[test]
fn dismiss_never_submits_and_confirm_reports_through_the_feedback_line() {
    let mut app = window_app();
    seed_processes(&mut app);
    app.update();
    app.update();

    // Dismiss through the cancel button: the modal closes, nothing submits.
    press(&mut app, KeyCode::Delete, None);
    app.update();
    let dismiss = button_entity(&mut app, "dismiss");
    activate(&mut app, dismiss);
    app.update();
    assert!(overlay_despawned(&mut app), "a dismissed modal despawns");
    assert!(
        app.world().resource::<PendingEffects>().0.is_empty(),
        "dismissal never submits"
    );

    // The full loop: re-arm, confirm, let the drain submit through the
    // shared queue_effect seam, and observe the header feedback line leave
    // its cold-start blank — the honest outcome of the submission.
    press(&mut app, KeyCode::Delete, None);
    app.update();
    assert!(!overlay_despawned(&mut app), "the gate re-arms");
    let confirm = button_entity(&mut app, "confirm");
    activate(&mut app, confirm);
    app.update();
    let world = app.world_mut();
    let feedback = world
        .query_filtered::<&Text, With<FeedbackLine>>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(feedback.len(), 1, "exactly one feedback line");
    assert!(
        !feedback[0].is_empty(),
        "the drain publishes the submission outcome to the feedback line"
    );
}
