//! test-intent: behavior
use super::*;
use crate::demo_fixture::{demo_shell, seed_capture_confirmation_scenario};
use crate::text_selection::flush_clipboard;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::entity::Entity;
use bevy::ui_widgets::Activate;

fn click(app: &mut App, action: EventAction) {
    let entity = {
        let world = app.world_mut();
        world
            .query::<(Entity, &EventControl)>()
            .iter(world)
            .find(|(_, control)| control.0 == action)
            .map(|(entity, _)| entity)
            .expect("mounted native event control")
    };
    app.world_mut().trigger(Activate { entity });
    for _ in 0..3 {
        app.update();
    }
}
#[test]
fn native_event_controls_filter_export_clear_and_close_owned_history() {
    let mut app = scripted_frontend_app();
    app.update();
    let mut shell = demo_shell();
    seed_capture_confirmation_scenario(&mut shell, "event-center");
    let cleared_line = event_history_line(
        shell
            .projection()
            .alert_center
            .event_history()
            .iter()
            .find(|event| event.kind == AlertEventKind::Cleared)
            .expect("cleared"),
    );
    let activated_line = event_history_line(
        shell
            .projection()
            .alert_center
            .event_history()
            .iter()
            .find(|event| event.kind == AlertEventKind::Activated)
            .expect("activated"),
    );
    let expected =
        export_alert_events_json(shell.projection().alert_center.event_history()).expect("JSON");
    assert!(
        shell
            .projection()
            .alert_center
            .event_history()
            .iter()
            .any(|event| event.kind == AlertEventKind::Activated)
    );
    assert!(
        shell
            .projection()
            .alert_center
            .event_history()
            .iter()
            .any(|event| event.kind == AlertEventKind::Cleared)
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    app.world_mut().trigger(EventCommand(EventAction::Open));
    for _ in 0..3 {
        app.update();
    }
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::EventCenter)
    ));
    click(&mut app, EventAction::Filter(Some(AlertEventKind::Cleared)));
    assert_eq!(
        app.world().resource::<EventCenterState>().0,
        Some(AlertEventKind::Cleared)
    );
    let texts: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(texts.contains(&cleared_line));
    assert!(!texts.contains(&activated_line));
    // Activate without advancing PostUpdate: inspect the real output queue using an owned sink.
    let export = app
        .world_mut()
        .query::<(Entity, &EventControl)>()
        .iter(app.world())
        .find(|(_, control)| control.0 == EventAction::Export)
        .map(|(entity, _)| entity)
        .expect("export");
    app.world_mut().trigger(Activate { entity: export });
    app.world_mut().flush();
    let mut copied = String::new();
    app.world_mut().resource_scope(
        |world, mut output: bevy::ecs::change_detection::Mut<ClipboardPort>| {
            let mut track = world.non_send_mut::<FrontendTrack>();
            flush_clipboard(&mut output, &mut track.shell, |text| {
                copied = text.into();
                Ok(())
            });
        },
    );
    assert_eq!(
        copied, expected,
        "export retains the authoritative complete history"
    );
    click(&mut app, EventAction::Clear);
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .projection()
            .alert_center
            .event_history()
            .is_empty()
    );
    click(&mut app, EventAction::Close);
    assert!(app.world().resource::<WindowSurfaceState>().0.is_none());
}
