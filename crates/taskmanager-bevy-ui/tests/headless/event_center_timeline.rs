//! test-intent: behavior
//!
//! The event center renders the audit history on a vertical timeline: every
//! event owns a node ring and the events are joined by a connector, so the
//! history reads as a timeline with a sense of time passing instead of a
//! flat list of lines.

use super::*;
use crate::demo_fixture::{demo_shell, seed_capture_confirmation_scenario};
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;

fn count<T: bevy::ecs::component::Component>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<T>>()
        .iter(app.world())
        .count()
}

#[test]
fn the_event_history_renders_a_vertical_timeline() {
    let mut app = scripted_frontend_app();
    app.update();
    let mut shell = demo_shell();
    seed_capture_confirmation_scenario(&mut shell, "event-center");
    let events = shell.projection().alert_center.event_history().len();
    assert!(events > 0, "the fixture seeds audit events");
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    app.world_mut().trigger(EventCommand(EventAction::Open));
    for _ in 0..3 {
        app.update();
    }

    assert_eq!(
        count::<TimelineNode>(&mut app),
        events,
        "every audit event owns a timeline node ring"
    );
    assert_eq!(
        count::<TimelineConnector>(&mut app),
        events - 1,
        "the events are joined by one connector each"
    );
}
