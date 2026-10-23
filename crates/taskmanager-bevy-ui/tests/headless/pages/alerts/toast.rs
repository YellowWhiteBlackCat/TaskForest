//! test-intent: behavior
//!
//! A preset export that reached the system clipboard confirms itself with a
//! toast: the toast appears only after the native write succeeded, and it
//! carries the copied label. A failed write must not raise it.

use super::tests::{headless_shell_app, mount_alerts};
use super::*;
use crate::text_selection::{ClipboardPort, flush_clipboard};
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use taskmanager_application::i18n::t;

/// The export control, activated without advancing a frame: the headless
/// composition has no native clipboard, so the frame's own flush would
/// consume the queued document before the test can observe it.
fn request_export(app: &mut App) {
    let entity = app
        .world_mut()
        .query::<(Entity, &editor::RuleButton)>()
        .iter(app.world())
        .find(|(_, control)| control.0 == editor::RuleControl::Export)
        .map(|(entity, _)| entity)
        .expect("the export control is mounted");
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity });
}

fn toast_message(app: &mut App) -> String {
    app.world().resource::<toast::ExportToast>().message.clone()
}

fn toast_badges(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<toast::ExportToastBadge>>()
        .iter(app.world())
        .count()
}

#[test]
fn a_successful_export_raises_the_confirmation_toast() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);

    // The export action queues the real document for the native clipboard.
    request_export(&mut app);
    assert!(
        app.world().resource::<toast::ExportToast>().pending,
        "the export asked for a confirmation"
    );
    assert!(
        toast_message(&mut app).is_empty(),
        "the toast waits for the native write: it must not claim a copy that did not happen"
    );

    // The native write succeeds.
    let mut written = Vec::new();
    app.world_mut().resource_scope(
        |world, mut output: bevy::ecs::change_detection::Mut<ClipboardPort>| {
            let mut track = world.non_send_mut::<FrontendTrack>();
            flush_clipboard(&mut output, &mut track.shell, |text| {
                written.push(text.to_owned());
                Ok(())
            });
        },
    );
    assert_eq!(
        written.len(),
        1,
        "the export produced one clipboard document"
    );
    app.update();

    let message = toast_message(&mut app);
    assert!(!message.is_empty(), "the successful copy raised the toast");
    assert!(
        message.contains(t("alerts.manage")),
        "the toast carries the copied label: {message}"
    );
    assert_eq!(
        toast_badges(&mut app),
        1,
        "the confirmation badge is mounted over the content region"
    );
}

#[test]
fn a_failed_export_raises_no_toast() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    request_export(&mut app);

    // The native write fails.
    app.world_mut().resource_scope(
        |world, mut output: bevy::ecs::change_detection::Mut<ClipboardPort>| {
            let mut track = world.non_send_mut::<FrontendTrack>();
            flush_clipboard(&mut output, &mut track.shell, |_text| {
                Err("the compositor refused the paste".to_owned())
            });
        },
    );
    app.update();

    assert!(
        toast_message(&mut app).is_empty(),
        "a refused copy raises no confirmation"
    );
    assert_eq!(
        toast_badges(&mut app),
        0,
        "no confirmation badge is mounted for a failed copy"
    );
}
