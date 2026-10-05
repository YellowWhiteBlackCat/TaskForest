//! test-intent: behavior
//! Actual controls submit coordinator writes and restart restores only saved user views.
use super::*;
use crate::app::RouteChanged;
use crate::saved_views::{SavedViewAction, SavedViewControl, SavedViewsState};
use bevy::ui_widgets::Activate;
use taskmanager_shell::{ProcessStatusFilter, SortCol};
fn click(app: &mut App, runtime: &'static SharedRuntime, action: SavedViewAction) {
    let entity = {
        let world = app.world_mut();
        world
            .query::<(Entity, &SavedViewControl)>()
            .iter(world)
            .find(|(_, control)| control.0 == action)
            .map(|(entity, _)| entity)
            .expect("mounted control")
    };
    app.world_mut().trigger(Activate { entity });
    app.update();
    wait_for_config_sync(runtime);
    for _ in 0..3 {
        app.update();
    }
}
#[test]
fn saved_views_native_controls_persist_and_restart_rehydrates_shared_config() {
    let path = test_path("saved-views");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);
    let mut app = headless_settings_app(runtime);
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    app.world_mut().trigger(RouteChanged);
    {
        let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
        track
            .shell
            .set_process_status_filter(ProcessStatusFilter::Sleeping);
        track.shell.set_sort_column(SortCol::Memory);
    }
    for _ in 0..3 {
        app.update();
    }
    click(&mut app, runtime, SavedViewAction::Open);
    click(&mut app, runtime, SavedViewAction::Save);
    let saved = ConfigStore::new(&path).load_or_default();
    assert_eq!(saved.saved_process_views.len(), 1);
    assert_eq!(saved.saved_process_views[0].filter, "Sleeping");
    drop(app);
    let (_, new_client) = start_coordinator(&path);
    let new_runtime = scripted_runtime_with_config(new_client);
    let mut restarted = headless_settings_app(new_runtime);
    for _ in 0..3 {
        restarted.update();
    }
    let row = restarted
        .world()
        .resource::<SavedViewsState>()
        .rows
        .last()
        .expect("restored")
        .clone();
    assert!(row.is_user_saved());
    assert_eq!(row.filter, ProcessStatusFilter::Sleeping);
    restarted.world_mut().resource_mut::<Route>().page = Page::Processes;
    restarted.world_mut().trigger(RouteChanged);
    for _ in 0..3 {
        restarted.update();
    }
    click(&mut restarted, new_runtime, SavedViewAction::Open);
    click(&mut restarted, new_runtime, SavedViewAction::Remove(row.id));
    assert!(
        ConfigStore::new(&path)
            .load_or_default()
            .saved_process_views
            .is_empty()
    );
    drop(restarted);
    drop(coordinator);
    std::fs::remove_dir_all(path.parent().expect("fixture parent")).expect("cleanup");
}

#[test]
fn saved_views_failed_native_config_save_keeps_canonical_rows_and_reports_failure() {
    let path = test_path("saved-views-failure");
    let (coordinator, client) = start_coordinator(&path);
    let runtime = scripted_runtime_with_config(client);
    let mut app = headless_settings_app(runtime);
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    app.world_mut().trigger(RouteChanged);
    for _ in 0..3 {
        app.update();
    }
    std::fs::create_dir_all(&path).expect("unwritable file destination");
    click(&mut app, runtime, SavedViewAction::Open);
    click(&mut app, runtime, SavedViewAction::Save);
    assert!(
        app.world()
            .resource::<SavedViewsState>()
            .rows
            .iter()
            .all(|row| !row.is_user_saved())
    );
    let track = app.world().non_send::<FrontendTrack>();
    assert!(
        track
            .shell
            .feedback_notice()
            .is_some_and(|feedback| feedback.text().contains("SaveFailed"))
    );
    drop(app);
    drop(coordinator);
    std::fs::remove_dir_all(path.parent().expect("parent")).expect("cleanup");
}
