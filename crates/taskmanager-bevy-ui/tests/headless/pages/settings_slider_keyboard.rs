//! test-intent: behavior
//!
//! The settings refresh slider is a keyboard control (GPUI parity):
//!
//! - the mounted slider is a tab stop and wears the product's one focus
//!   ring, so the keyboard can reach it at all;
//! - the keys a focused slider owns — the arrows step, Home/End jump to the
//!   range ends — are answered by the widget, and the shell's routers never
//!   see them (the GPUI `stop_propagation` contract);
//! - the value a key produces reaches the canonical authority (the shell's
//!   telemetry cadence) exactly like a drag does;
//! - the keyboard focus follows the control across the page rebuild its own
//!   change triggers, so a second step still lands on the value the user
//!   sees instead of a frozen one.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputDispatchPlugin, InputFocus};
use bevy::ui_widgets::{SliderPlugin, SliderValue};
use bevy::window::{PrimaryWindow, Window};

use super::tests::{headless_shell_app, mount_settings, refresh_slider_entity};
use crate::app::FrontendTrack;
use crate::focus_visible::FocusRing;
use crate::input::ShellInteractionApplied;

/// The keyboard composition: the official slider widget plus the toolkit's
/// focused-input dispatch, so a raw key press reaches the focused control.
fn keyboard_app() -> App {
    let mut app = headless_shell_app();
    app.add_plugins((SliderPlugin, InputDispatchPlugin));
    // The dispatch plugin routes raw keys to the focused entity, which needs
    // exactly one primary window.
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

/// Press one key and run the frame that carries it.
fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world_mut())
        .expect("the composition owns exactly one primary window");
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
}

/// The shell's accepted telemetry cadence, in milliseconds.
fn cadence_millis(app: &mut App) -> u64 {
    app.world()
        .non_send::<FrontendTrack>()
        .shell
        .telemetry_interval()
        .duration()
        .as_millis() as u64
}

/// Put the keyboard focus on the mounted refresh slider.
fn focus_refresh_slider(app: &mut App) -> Entity {
    let slider = refresh_slider_entity(app.world_mut());
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(slider, FocusCause::Navigated);
    slider
}

#[test]
fn the_refresh_slider_is_a_tab_stop_that_wears_the_shared_focus_ring() {
    let mut app = headless_shell_app();
    mount_settings(&mut app);
    app.update();

    let slider = refresh_slider_entity(app.world_mut());
    assert!(
        app.world().get::<TabIndex>(slider).is_some(),
        "the slider is reachable with the keyboard"
    );
    assert!(
        app.world().get::<FocusRing>(slider).is_some(),
        "the slider wears the product's one focus ring"
    );
}

#[test]
fn a_focused_refresh_slider_answers_its_keys_and_keeps_the_focus() {
    let mut app = keyboard_app();
    mount_settings(&mut app);
    focus_refresh_slider(&mut app);

    let before = cadence_millis(&mut app);

    // The arrow key steps the cadence by the widget's own step.
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    assert_eq!(
        cadence_millis(&mut app),
        before + 100,
        "the arrow key stepped the cadence through the canonical authority"
    );

    // The step composed on the value the user sees, and the focus followed
    // the control across the page rebuild its own change triggered.
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    assert_eq!(
        cadence_millis(&mut app),
        before + 200,
        "the second step landed on the accepted value, not on a frozen one"
    );

    // The vertical axis answers too, like GPUI's slider.
    press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    assert_eq!(
        cadence_millis(&mut app),
        before + 100,
        "the vertical axis steps the same control"
    );

    // Home and End jump to the range ends.
    press(&mut app, KeyCode::End, Key::End);
    assert_eq!(
        cadence_millis(&mut app),
        5_000,
        "End jumps to the range end"
    );
    press(&mut app, KeyCode::Home, Key::Home);
    assert_eq!(
        cadence_millis(&mut app),
        500,
        "Home jumps to the range start"
    );
}

#[test]
fn the_shell_never_sees_a_key_the_focused_slider_owns() {
    let mut app = keyboard_app();
    mount_settings(&mut app);

    // The frontend's published signal that the shell consumed a press.
    let applied = Arc::new(AtomicUsize::new(0));
    app.add_observer({
        let applied = applied.clone();
        move |_: On<ShellInteractionApplied>| {
            applied.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Unfocused, Home is the shell's own selection movement.
    press(&mut app, KeyCode::Home, Key::Home);
    assert_eq!(
        applied.load(Ordering::SeqCst),
        1,
        "an unfocused Home reaches the shell"
    );

    // Focused, the widget owns the key: the shell never sees it.
    focus_refresh_slider(&mut app);
    press(&mut app, KeyCode::Home, Key::Home);
    assert_eq!(
        applied.load(Ordering::SeqCst),
        1,
        "a focused Home never reaches the shell"
    );
    assert_eq!(
        cadence_millis(&mut app),
        500,
        "the widget answered Home with the range minimum"
    );

    // The widget owns its own keys, not the whole keyboard: a key the
    // slider does not own still reaches the shell.
    press(&mut app, KeyCode::F1, Key::F1);
    assert_eq!(
        applied.load(Ordering::SeqCst),
        2,
        "a key the slider does not own still reaches the shell"
    );
}

#[test]
fn the_widget_value_follows_every_change_the_control_emits() {
    let mut app = keyboard_app();
    mount_settings(&mut app);
    let slider = refresh_slider_entity(app.world_mut());

    // A held drag does not rebuild the page, so the widget's own value is
    // the only place the accepted change can live. The write-back is bound
    // for the whole frontend, not only for the page that happened to mount
    // the alerts editor first.
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::ValueChange::<f32> {
            source: slider,
            value: 3.7,
            is_final: false,
        });
    app.update();

    assert_eq!(
        app.world()
            .get::<SliderValue>(slider)
            .expect("the slider keeps its own value")
            .0,
        3.7,
        "the widget's own value follows the change it emitted"
    );
}
