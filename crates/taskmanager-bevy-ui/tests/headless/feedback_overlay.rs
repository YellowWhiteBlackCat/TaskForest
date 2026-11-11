use super::*;
use crate::app::{FrontendTrack, Page, Route};
use crate::window::FeedbackActivityLine;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::input_focus::InputFocus;
use bevy::picking::Pickable;
use bevy::ui::widget::Text;
use std::sync::{Arc, Mutex};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, FeedbackTone};

fn mounted_notice(app: &mut App) -> Option<String> {
    let world = app.world_mut();
    world
        .query_filtered::<&Text, With<FeedbackLine>>()
        .iter(world)
        .map(|text| text.0.clone())
        .next()
}

fn overlay_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<FeedbackToast>>()
        .iter(world)
        .count()
}

fn overlay_entity(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<FeedbackToast>>()
        .single(world)
        .expect("one feedback overlay is mounted")
}

fn set_route(app: &mut App, page: Page) {
    app.world_mut().resource_mut::<Route>().page = page;
    app.world_mut().trigger(crate::app::RouteChanged);
    app.update();
}

#[test]
fn performance_notice_mounts_a_single_non_modal_overlay() {
    let mut app = crate::window::tests::scripted_frontend_app();
    set_route(&mut app, Page::Performance);
    let focus_before = app.world().resource::<InputFocus>().get();

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Success,
            FeedbackLifecycle::UntilReplaced,
            "Saved current window",
        );
    app.update();

    assert_eq!(
        overlay_count(&mut app),
        1,
        "the floating shell notice mounts over Performance"
    );
    assert_eq!(
        mounted_notice(&mut app).as_deref(),
        Some("Saved current window"),
        "the mounted overlay carries the shell notice text"
    );
    let overlay = overlay_entity(&mut app);
    assert_eq!(
        app.world().get::<Pickable>(overlay),
        Some(&Pickable::IGNORE),
        "the floating overlay leaves pointer input available to the page"
    );
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        focus_before,
        "mounting feedback does not steal keyboard focus"
    );
    assert!(
        app.world()
            .resource::<crate::input::PendingEffects>()
            .0
            .is_empty(),
        "displaying feedback submits no platform effect"
    );
}

#[test]
fn non_performance_routes_keep_notice_off_the_overlay() {
    let mut app = crate::window::tests::scripted_frontend_app();
    set_route(&mut app, Page::Processes);

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Info,
            FeedbackLifecycle::UntilReplaced,
            "Not a Performance notice",
        );
    app.update();

    assert_eq!(
        overlay_count(&mut app),
        0,
        "routes with a persistent footer must not mount a second feedback surface"
    );
}

#[test]
fn overlay_closes_when_the_shell_notice_is_replaced_by_activity() {
    let mut app = crate::window::tests::scripted_frontend_app();
    set_route(&mut app, Page::Performance);

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Info,
            FeedbackLifecycle::UntilReplaced,
            "First notice",
        );
    app.update();
    assert_eq!(overlay_count(&mut app), 1);

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .clear_feedback_notice();
    app.update();

    assert_eq!(
        overlay_count(&mut app),
        0,
        "expired or replaced notices must retire the overlay"
    );
}

#[test]
fn same_notice_text_with_a_new_tone_repaints_the_overlay() {
    let mut app = crate::window::tests::scripted_frontend_app();
    set_route(&mut app, Page::Performance);

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Success,
            FeedbackLifecycle::UntilReplaced,
            "Operation complete",
        );
    app.update();
    let first = overlay_entity(&mut app);
    assert_eq!(
        app.world()
            .get::<FeedbackToast>(first)
            .and_then(|toast| toast.tone),
        Some(FeedbackTone::Positive)
    );

    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            "Operation complete",
        );
    app.update();

    assert_eq!(overlay_count(&mut app), 1);
    let updated = overlay_entity(&mut app);
    assert_eq!(
        app.world()
            .get::<FeedbackToast>(updated)
            .and_then(|toast| toast.tone),
        Some(FeedbackTone::Danger),
        "a changed visual tone is not hidden by identical feedback copy"
    );
}

#[test]
fn footer_activity_update_publishes_once_across_stable_frames() {
    let mut app = crate::window::tests::scripted_frontend_app();
    app.update();

    let published = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&published);
    app.add_observer(
        move |activity: bevy::ecs::observer::On<crate::drain::FeedbackActivityChanged>| {
            observed
                .lock()
                .expect("activity observer lock")
                .push(activity.event().0.clone());
        },
    );
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .set_feedback_activity("Refreshing hardware inventory");

    app.update();
    app.update();

    assert_eq!(
        published.lock().expect("activity observer lock").as_slice(),
        &["Refreshing hardware inventory".to_owned()],
        "the activity cache publishes its change once across stable frames"
    );
    let world = app.world_mut();
    let footer_text = world
        .query_filtered::<&Text, With<FeedbackActivityLine>>()
        .single(world)
        .expect("the footer activity line")
        .0
        .clone();
    assert_eq!(footer_text, "Refreshing hardware inventory");
}
