//! test-intent: behavior

use super::*;
use crate::app::{FrontendTrack, Page, Route};
use crate::window::tests::scripted_frontend_app;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_shell::{command_help, page_help};

fn press(app: &mut App, key_code: KeyCode, text: Option<&str>) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: text.map_or(Key::Unidentified(NativeKey::Unidentified), |value| {
            Key::Character(value.into())
        }),
        state: ButtonState::Pressed,
        text: text.map(Into::into),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();
}

fn settle(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn overlay_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<HelpOverlay>>()
        .iter(world)
        .count()
}

fn mounted_texts(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect()
}

fn close_button(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<HelpCloseButton>>()
        .single(world)
        .expect("one help close button")
}

#[test]
fn f1_opens_shared_help_and_question_mark_toggles_it_closed() {
    let mut app = scripted_frontend_app();
    app.update();
    assert_eq!(overlay_count(&mut app), 0);

    press(&mut app, KeyCode::F1, None);
    settle(&mut app, 4);
    assert!(
        app.world().non_send::<FrontendTrack>().shell.help_open(),
        "bare F1 enters the shell-owned help mode"
    );
    assert_eq!(overlay_count(&mut app), 1, "one visible help dialog mounts");
    let texts = mounted_texts(&mut app);
    assert!(texts.iter().any(|text| text == t("settings.keyboard")));
    assert!(texts.iter().any(|text| text == t("settings.keys_pages")));
    assert!(texts.iter().any(|text| text == t("common.all")));
    for page in page_help() {
        assert!(
            texts.iter().any(|text| text == page.shortcut),
            "the page shortcut {} must be visible",
            page.shortcut
        );
    }
    for help in command_help() {
        assert!(
            texts.iter().any(|text| text == help.shortcut),
            "the command shortcut {} must be visible",
            help.shortcut
        );
    }
    assert!(
        texts.iter().any(|text| text == "F1 / ?"),
        "the frontend-local opener must advertise both reachable chords"
    );

    press(&mut app, KeyCode::F1, None);
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "F1 toggles the already open help surface closed"
    );
    assert_eq!(overlay_count(&mut app), 0, "the dialog retires with F1");

    press(&mut app, KeyCode::F1, None);
    assert!(
        app.world().non_send::<FrontendTrack>().shell.help_open(),
        "F1 can open the help again"
    );
    press(&mut app, KeyCode::Slash, Some("?"));
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "the shared local-key router closes help on '?'"
    );
    assert_eq!(
        overlay_count(&mut app),
        0,
        "the dialog retires with the mode"
    );
}

#[test]
fn escape_and_footer_button_close_the_help_surface() {
    let mut app = scripted_frontend_app();
    app.update();
    press(&mut app, KeyCode::F1, None);
    assert_eq!(overlay_count(&mut app), 1);

    press(&mut app, KeyCode::Escape, None);
    assert!(!app.world().non_send::<FrontendTrack>().shell.help_open());
    assert_eq!(overlay_count(&mut app), 0);

    press(&mut app, KeyCode::F1, None);
    let button = close_button(&mut app);
    app.world_mut().trigger(Activate { entity: button });
    app.update();
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "the visible Close action leaves through the same shell state"
    );
    assert_eq!(overlay_count(&mut app), 0);
}

#[test]
fn page_navigation_closes_help_and_still_moves_the_route() {
    let mut app = scripted_frontend_app();
    app.update();
    press(&mut app, KeyCode::F1, None);
    assert!(app.world().non_send::<FrontendTrack>().shell.help_open());
    assert_eq!(overlay_count(&mut app), 1);

    press(&mut app, KeyCode::AltLeft, None);
    press(&mut app, KeyCode::Digit1, None);
    app.update();
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "the shared page transition retires help like the GPUI reference"
    );
    assert_eq!(overlay_count(&mut app), 0);
    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Performance,
        "the learned page chord still moves the route"
    );
}

#[test]
fn question_mark_types_into_search_instead_of_opening_help() {
    let mut app = scripted_frontend_app();
    app.update();
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .open_search();
    press(&mut app, KeyCode::Slash, Some("?"));
    let shell = &app.world().non_send::<FrontendTrack>().shell;
    assert!(
        !shell.help_open(),
        "the search field owns '?' instead of opening the help dialog"
    );
    assert!(shell.query.contains('?'), "the literal character is typed");
    assert_eq!(overlay_count(&mut app), 0);
}

#[test]
fn f1_stays_inactive_while_a_primary_modal_owns_the_keyboard() {
    let mut app = scripted_frontend_app();
    app.update();
    app.world_mut()
        .trigger(crate::window_surface::WindowSurfaceCommand::About);
    app.update();
    assert!(matches!(
        app.world()
            .resource::<crate::window_surface::WindowSurfaceState>()
            .active(),
        Some(crate::window_surface::WindowSurface::About)
    ));

    press(&mut app, KeyCode::F1, None);
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "the primary surface keeps precedence over the help chord"
    );
    assert_eq!(overlay_count(&mut app), 0);

    let mut app = scripted_frontend_app();
    app.update();
    app.world_mut().non_send_mut::<FrontendTrack>().shell =
        crate::selection_fixture_support::shell_with_selection();
    app.update();
    press(&mut app, KeyCode::Delete, None);
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .confirmation_kind()
            .is_some(),
        "the end-task gate owns the keyboard"
    );
    press(&mut app, KeyCode::F1, None);
    assert!(
        !app.world().non_send::<FrontendTrack>().shell.help_open(),
        "F1 cannot open help above an armed confirmation"
    );
    assert_eq!(overlay_count(&mut app), 0);
}
