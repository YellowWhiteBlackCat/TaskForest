//! test-intent: behavior
//!
//! The active-alert banner is a roller blind: a firing alert opens from
//! collapsed through a damped spring, and an alert the page has already
//! shown stays settled across the page's periodic remount (the page
//! remounts on every folded telemetry batch, so an unsettled banner would
//! replay its reveal every second).

use super::tests::{headless_shell_app, mount_alerts};
use super::*;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::time::TimeUpdateStrategy;
use bevy::ui::prelude::{Node, Val};
use std::time::Duration;
use taskmanager_shell::fixture::alerts::seed_shell_active_alert;

/// Drive the app on a deterministic 60fps clock so the spring advances by a
/// known step per frame instead of by the host's wall clock.
fn fixed_clock(app: &mut App) {
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )));
}

fn banner_entities(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<banner::BannerSpring>>()
        .iter(app.world())
        .collect()
}

fn spring_of(app: &mut App, entity: Entity) -> banner::BannerSpring {
    app.world_mut()
        .get::<banner::BannerSpring>(entity)
        .copied()
        .expect("the banner frame owns its spring")
}

#[test]
fn a_firing_alert_opens_its_banner_through_the_spring() {
    let mut app = headless_shell_app();
    fixed_clock(&mut app);
    let mut shell = crate::demo_fixture::demo_shell();
    assert!(
        seed_shell_active_alert(&mut shell),
        "the fixture arms one active alert"
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    mount_alerts(&mut app);

    let banners = banner_entities(&mut app);
    assert_eq!(banners.len(), 1, "the active alert renders one banner");
    let banner = banners[0];
    let spring = spring_of(&mut app, banner);
    assert!(spring.target > 0.0, "the banner has a slot to open into");
    assert_eq!(
        spring.value, 0.0,
        "a newly fired alert starts collapsed — no hard displacement"
    );

    let mut opened = 0.0;
    let mut height = Val::Auto;
    for _ in 0..30 {
        app.update();
        let spring = spring_of(&mut app, banner);
        opened = spring.value;
        height = app
            .world_mut()
            .get::<Node>(banner)
            .expect("the banner frame is a node")
            .height;
    }
    assert!(
        opened > 0.0,
        "the spring eased the banner open instead of snapping it"
    );
    assert!(
        opened <= spring.target * 1.5,
        "the banner stays inside its slot: {opened} vs {}",
        spring.target
    );
    assert!(
        matches!(height, Val::Rem(_)),
        "the banner height rides the rem axis: {height:?}"
    );
}

#[test]
fn a_known_alert_keeps_its_banner_settled_across_a_remount() {
    let mut app = headless_shell_app();
    fixed_clock(&mut app);
    let mut shell = crate::demo_fixture::demo_shell();
    assert!(
        seed_shell_active_alert(&mut shell),
        "the fixture arms one active alert"
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    mount_alerts(&mut app);
    let first = banner_entities(&mut app);
    assert_eq!(first.len(), 1, "the active alert renders one banner");

    // The page remounts on every folded batch; the alert is already known,
    // so its banner must not replay the reveal.
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    let remounted = banner_entities(&mut app);
    assert_eq!(
        remounted.len(),
        1,
        "the remounted page still renders the banner"
    );
    assert_ne!(
        remounted[0], first[0],
        "the remount replaced the banner frame"
    );
    let spring = spring_of(&mut app, remounted[0]);
    assert_eq!(
        spring.value, spring.target,
        "an alert the page already showed keeps its banner open"
    );
}
