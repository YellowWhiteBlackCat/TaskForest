// test-intent: behavior
//! The process-control emphasis: the "End task" action wears the destructive
//! accent exactly when the selected process is in the platform's typed
//! "not responding" state.

use super::*;
use iced::widget::button;
use taskforest_core::core::process::{ProcessItem, ProcessLiveKey};
use taskmanager_shell::fixture::{ProjectionSeedFact, demo_app, seed_projection_fact};
use taskmanager_test_support::ProcessItemFixtureBuilder;
use taskmanager_theme::Theme;

/// A process in the platform's typed "not responding" state (the core's
/// uninterruptible wait) and a healthy one.
fn stuck_process() -> ProcessItem {
    ProcessItemFixtureBuilder::new()
        .pid(4242)
        .name("stuck-worker".to_owned())
        .status("D".to_owned())
        .build()
}

fn healthy_process() -> ProcessItem {
    ProcessItemFixtureBuilder::new()
        .pid(4343)
        .name("healthy-worker".to_owned())
        .status("R".to_owned())
        .build()
}

/// The typed state, not a synthetic flag: the core's process state decides.
#[test]
fn the_not_responding_state_comes_from_the_typed_process_state() {
    assert!(
        emphasis::is_not_responding(&stuck_process()),
        "the uninterruptible wait is the product's 'not responding' state"
    );
    assert!(
        !emphasis::is_not_responding(&healthy_process()),
        "a running process is not in the 'not responding' state"
    );
    let sleeping = ProcessItemFixtureBuilder::new()
        .pid(4444)
        .name("sleeper".to_owned())
        .status("S".to_owned())
        .build();
    assert!(
        !emphasis::is_not_responding(&sleeping),
        "ordinary sleeping is distinct from the uninterruptible wait"
    );
}

/// The action bar's emphasis follows the SELECTION: selecting a stuck process
/// arms the destructive accent, selecting a healthy one does not.
#[test]
fn the_selection_drives_the_destructive_emphasis() {
    let stuck = stuck_process();
    let mut shell = demo_app();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Processes(Some(vec![stuck.clone()])),
    );
    assert!(
        !emphasis::selection_is_not_responding(&shell),
        "an empty selection has no emphasis"
    );

    let identity = ProcessLiveKey::from_process(&stuck).expect("the fixture has a live identity");
    shell.add_selected_identity(identity);
    assert!(
        emphasis::selection_is_not_responding(&shell),
        "the selected stuck process arms the destructive emphasis"
    );

    let healthy = healthy_process();
    let mut shell = demo_app();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Processes(Some(vec![healthy.clone()])),
    );
    let healthy_identity =
        ProcessLiveKey::from_process(&healthy).expect("the fixture has a live identity");
    shell.add_selected_identity(healthy_identity);
    assert!(
        !emphasis::selection_is_not_responding(&shell),
        "a healthy selection keeps the ordinary accent"
    );
}

/// The emphasis reaches the pixels: the destructive variant of the shared
/// button style paints the theme's danger token, the ordinary one does not.
#[test]
fn the_destructive_emphasis_paints_the_theme_danger_token() {
    let theme = Theme::dark();
    let danger = crate::theme_binding::color(theme.palette().danger);
    let accent = crate::theme_binding::color(theme.palette().accent);

    let destructive = crate::theme::button_style_status(&theme, true, button::Status::Active);
    let Some(iced::Background::Color(destructive_fill)) = destructive.background else {
        panic!("the destructive button paints a solid fill");
    };
    assert_eq!(
        destructive_fill, danger,
        "the emphasized button must wear the theme's destructive red"
    );
    assert_ne!(
        destructive_fill, accent,
        "the destructive fill must not be the ordinary accent"
    );

    let ordinary = crate::theme::button_style_status(&theme, false, button::Status::Active);
    let Some(iced::Background::Gradient(_)) = ordinary.background else {
        panic!("the ordinary button paints the accent gradient");
    };
}
