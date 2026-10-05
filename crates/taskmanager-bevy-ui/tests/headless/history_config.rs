//! test-intent: behavior
//! Native history review commands and the canonical preference lifecycle.

use super::*;

#[test]
fn history_settings_enable_real_review_commands_and_release_the_writer_on_disable() {
    use crate::app::RouteChanged;
    use crate::pages::history::control::{
        HistoryCommand, PerformanceHistoryProjectionResource, PerformancePresentation,
    };
    use crate::pages::history::{HistoryProjectionResource, HistoryRuntime, drain_history_system};
    use crate::widgets::history_controls::HistoryControl;
    use bevy::app::PreUpdate;
    use bevy::ui_widgets::Activate;
    use taskmanager_app_host::NativeAppHost;
    use taskmanager_core::core::history::{
        ApplicationHistoryIdentity, HistoryMetric, HistorySeriesKey, HistoryWindow,
    };
    use taskmanager_core::core::time::LocalTimeRulesObservation;

    let path = test_path("history-review");
    let root = path.parent().expect("isolated root");
    let history_dir = root.join("history");
    std::fs::create_dir_all(&history_dir).expect("private history fixture");
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
    )
    .expect("stamp");
    let jsonl = format!(
        "{{\"r\":1,\"c\":{},\"m\":1,\"v\":91}}\n{{\"r\":2,\"c\":{},\"m\":2,\"v\":21}}\n{{\"r\":3,\"c\":{},\"m\":3,\"v\":37}}\n",
        now - 4 * 3_600_000,
        now - 60_000,
        now - 30_000
    );
    std::fs::write(history_dir.join("cpu-usage-pct__-__-.jsonl"), &jsonl).expect("system series");
    std::fs::write(
        history_dir.join(format!(
            "{}.jsonl",
            HistorySeriesKey::for_application(
                HistoryMetric::ApplicationCpuUsagePct,
                ApplicationHistoryIdentity::verified_launcher("io.example.History")
                    .expect("application identity")
            )
            .file_stem()
        )),
        &jsonl,
    )
    .expect("application series");
    let host = NativeAppHost::from_paths(
        path.clone(),
        history_dir,
        LocalTimeRulesObservation::unsupported(0),
    );
    let mut config = host.config_client().expect("coordinator");
    assert!(matches!(
        config.wait_for_initial(Duration::from_secs(2)),
        ConfigBootstrap::Published(_)
    ));
    let runtime = scripted_runtime_with_config(config);
    let mut app = headless_settings_app(runtime);
    let mut history = HistoryRuntime::default();
    history.install_connector(host.history_frontend_connector());
    app.insert_non_send(history);
    app.add_systems(PreUpdate, drain_history_system);

    fn await_rows(app: &mut App) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            app.update();
            if !app
                .world()
                .resource::<PerformanceHistoryProjectionResource>()
                .0
                .rows
                .is_empty()
                && !app
                    .world()
                    .resource::<PerformanceHistoryProjectionResource>()
                    .0
                    .refreshing
            {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "real worker must publish the requested rows"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn press(app: &mut App, command: HistoryCommand) {
        let entity = app
            .world_mut()
            .query::<(Entity, &HistoryControl)>()
            .iter(app.world())
            .find(|(_, control)| control.0 == Some(command))
            .map(|(entity, _)| entity)
            .expect("normal history control mounted");
        app.world_mut().trigger(Activate { entity });
        app.update();
    }

    assert!(!app.world().non_send::<HistoryRuntime>().available());
    let enabled = choice_entity(app.world_mut(), &SettingsField::HistoryPersistence(true));
    activate(&mut app, enabled);
    wait_for_config_sync(runtime);
    await_rows(&mut app);
    assert!(
        ConfigStore::new(&path)
            .load_or_default()
            .history_persistence
    );
    assert_eq!(
        app.world()
            .resource::<PerformanceHistoryProjectionResource>()
            .0
            .rows[0]
            .peak_value,
        Some(37.0)
    );
    assert_eq!(
        app.world()
            .resource::<HistoryProjectionResource>()
            .0
            .rows
            .len(),
        1
    );

    app.world_mut().resource_mut::<Route>().page = Page::Performance;
    app.world_mut().trigger(RouteChanged);
    app.update();
    press(&mut app, HistoryCommand::OpenPerformance);
    assert_eq!(
        *app.world().resource::<PerformancePresentation>(),
        PerformancePresentation::Replay
    );
    let source = app
        .world()
        .resource::<PerformanceHistoryProjectionResource>()
        .0
        .source_request;
    press(
        &mut app,
        HistoryCommand::Window(HistoryWindow::TwentyFourHours),
    );
    await_rows(&mut app);
    let wider = &app
        .world()
        .resource::<PerformanceHistoryProjectionResource>()
        .0;
    assert_eq!(wider.rows_window, Some(HistoryWindow::TwentyFourHours));
    assert_ne!(wider.source_request, source);
    assert_eq!(wider.rows[0].peak_value, Some(91.0));
    let source = wider.source_request;
    press(&mut app, HistoryCommand::Refresh);
    await_rows(&mut app);
    assert_ne!(
        app.world()
            .resource::<PerformanceHistoryProjectionResource>()
            .0
            .source_request,
        source
    );
    press(&mut app, HistoryCommand::ClosePerformance);
    assert_eq!(
        *app.world().resource::<PerformancePresentation>(),
        PerformancePresentation::Live
    );
    assert_eq!(
        app.world()
            .resource::<HistoryProjectionResource>()
            .0
            .rows
            .len(),
        1,
        "leaving Performance retains the application reader"
    );

    app.world_mut().resource_mut::<Route>().page = Page::Settings;
    app.world_mut().trigger(RouteChanged);
    app.update();
    let disabled = choice_entity(app.world_mut(), &SettingsField::HistoryPersistence(false));
    activate(&mut app, disabled);
    wait_for_config_sync(runtime);
    app.update();
    assert!(!app.world().non_send::<HistoryRuntime>().available());
    assert!(
        app.world()
            .resource::<PerformanceHistoryProjectionResource>()
            .0
            .rows
            .is_empty()
    );
    assert!(
        !ConfigStore::new(&path)
            .load_or_default()
            .history_persistence
    );
    let enabled = choice_entity(app.world_mut(), &SettingsField::HistoryPersistence(true));
    activate(&mut app, enabled);
    wait_for_config_sync(runtime);
    await_rows(&mut app);
    assert!(
        app.world().non_send::<HistoryRuntime>().available(),
        "the same connector can reacquire its released writer"
    );
    drop(app);
    drop(host);
    let _ = std::fs::remove_dir_all(root);
}
