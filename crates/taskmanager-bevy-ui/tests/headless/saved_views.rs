//! test-intent: behavior
//! Mounted native controls use the shared saved-view model and owned clipboard completion.
use super::*;
use crate::demo_fixture::demo_shell;
use crate::text_selection::flush_clipboard;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::entity::Entity;
use taskmanager_shell::{ProcessStatusFilter, SortCol};

fn click(app: &mut App, action: SavedViewAction) {
    let entity = {
        let world = app.world_mut();
        world
            .query::<(Entity, &SavedViewControl)>()
            .iter(world)
            .find(|(_, control)| control.0 == action)
            .map(|(entity, _)| entity)
            .expect("native control")
    };
    app.world_mut().trigger(Activate { entity });
    for _ in 0..3 {
        app.update();
    }
}
#[test]
fn saved_views_native_controls_transfer_apply_and_reject_invalid_import_atomically() {
    let mut app = scripted_frontend_app();
    app.update();
    app.insert_resource(DemoMode);
    {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        track.shell = demo_shell();
        track
            .shell
            .set_process_status_filter(ProcessStatusFilter::Sleeping);
        track.shell.set_sort_column(SortCol::Memory);
    }
    click(&mut app, SavedViewAction::Open);
    click(&mut app, SavedViewAction::Save);
    assert_eq!(
        app.world()
            .resource::<SidebarState>()
            .0
            .saved_process_views
            .len(),
        1
    );
    let id = app
        .world()
        .resource::<SavedViewsState>()
        .rows
        .last()
        .expect("saved")
        .id;
    // Use the real output queue with an injected owned sink, without touching a host clipboard.
    app.world_mut()
        .trigger(SavedViewCommand(SavedViewAction::Export));
    let mut copied = String::new();
    app.world_mut().resource_scope(
        |world: &mut bevy::ecs::world::World,
         mut output: bevy::ecs::change_detection::Mut<ClipboardPort>| {
            let mut track = world.non_send_mut::<FrontendTrack>();
            flush_clipboard(&mut output, &mut track.shell, |text| {
                copied = text.to_owned();
                Ok(())
            });
        },
    );
    assert!(copied.contains("Sleeping"));
    app.world_mut().resource_mut::<PendingImport>().0 = Some(ClipboardRead::Ready(Ok(copied)));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world()
            .resource::<SidebarState>()
            .0
            .saved_process_views
            .len(),
        2
    );
    let before = app.world().resource::<SidebarState>().0.clone();
    app.world_mut().resource_mut::<PendingImport>().0 =
        Some(ClipboardRead::Ready(Ok("broken".into())));
    app.update();
    assert_eq!(app.world().resource::<SidebarState>().0, before);
    assert_eq!(
        app.world().resource::<SavedViewsState>().feedback,
        Some(SavedViewTransferFeedback::ImportInvalid)
    );
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .set_process_status_filter(ProcessStatusFilter::All);
    click(&mut app, SavedViewAction::Apply(id));
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .process_status_filter,
        ProcessStatusFilter::Sleeping
    );
    assert_eq!(app.world().resource::<Route>().page, Page::Processes);
    assert!(app.world().resource::<WindowSurfaceState>().0.is_none());
    click(&mut app, SavedViewAction::Open);
    click(&mut app, SavedViewAction::Remove(id));
    assert_eq!(
        app.world()
            .resource::<SidebarState>()
            .0
            .saved_process_views
            .len(),
        1
    );
}
