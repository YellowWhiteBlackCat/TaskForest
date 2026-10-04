//! test-intent: behavior

use super::*;
use crate::app::{Page, Route};
use crate::text_selection::flush_clipboard;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::change_detection::Mut;
use bevy::ecs::entity::Entity;
use taskmanager_shell::fixture;

#[test]
fn normal_system_about_control_renders_cached_facts_and_exports_the_reviewed_values() {
    let mut app = scripted_frontend_app();
    app.world_mut().resource_mut::<Route>().page = Page::System;
    app.update();
    app.world_mut().non_send_mut::<FrontendTrack>().shell = fixture::demo_app();
    let control = app
        .world_mut()
        .query::<(Entity, &AboutControl)>()
        .iter(app.world())
        .find(|(_, control)| matches!(control.0, AboutCommand::Open))
        .map(|(entity, _)| entity)
        .expect("normal System About control");
    app.world_mut().trigger(Activate { entity: control });
    app.update();
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::About)
    ));
    let expected = about_text(app.world().non_send::<FrontendTrack>());
    assert!(expected.contains("TaskForestB"));
    assert!(expected.contains("Linux"), "cached OS fact: {expected}");
    let body = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .find(|text| text.0 == expected);
    assert!(
        body.is_some(),
        "actual modal body must render the reviewed facts"
    );
    app.world_mut().trigger(AboutCommand::Copy);
    app.world_mut().flush();
    let mut written = Vec::new();
    app.world_mut()
        .resource_scope(|world, mut port: Mut<ClipboardPort>| {
            flush_clipboard(
                &mut port,
                &mut world.non_send_mut::<FrontendTrack>().shell,
                |value| {
                    written.push(value.to_owned());
                    Ok(())
                },
            );
        });
    assert_eq!(written, [expected]);
    app.world_mut().trigger(AboutCommand::Diagnostics);
    app.update();
    assert!(
        app.world()
            .resource::<WindowSurfaceState>()
            .diagnostic()
            .is_some()
    );
    app.world_mut().trigger(AboutCommand::Close);
    app.update();
    assert!(
        app.world()
            .resource::<WindowSurfaceState>()
            .diagnostic()
            .is_some(),
        "a stale About close cannot dismiss diagnostics"
    );
}
