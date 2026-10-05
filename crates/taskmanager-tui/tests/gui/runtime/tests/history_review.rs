//! test-intent: behavior
//! Real history worker, settings apply and terminal command entry points.

use super::super::*;
use crate::demo_app;
use crate::ui::test_support::repo_temp_dir;
use ratatui::crossterm::event::KeyCode;
use std::time::{Duration, Instant};
use taskmanager_app_host::NativeAppHost;
use taskmanager_application::AppAction;
use taskmanager_core::core::history::{
    ApplicationHistoryIdentity, HistoryMetric, HistorySeriesKey, HistoryWindow,
};
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_theme::Theme;

fn press(app: &mut TuiApp, value: char) {
    let _ = handle_key(app, KeyEvent::new(KeyCode::Char(value), KeyModifiers::NONE));
}
fn wait_for_rows(app: &mut TuiApp) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let _ = app.drain_config_publications();
        let _ = app.drain_history_replay_completions();
        let model = app.performance_history_projection();
        if !model.rows.is_empty() && !model.refreshing {
            return;
        }
        assert!(Instant::now() < deadline, "the real query must complete");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn settings_history_apply_exposes_real_r_f_window_commands_and_disable_clears_review() {
    let root = repo_temp_dir().join("history-review");
    let history = root.join("history");
    std::fs::create_dir_all(&history).expect("history input directory");
    let now = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
    )
    .expect("stamp");
    let records = format!(
        "{{\"r\":1,\"c\":{},\"m\":1,\"v\":83}}\n{{\"r\":2,\"c\":{},\"m\":2,\"v\":19}}\n{{\"r\":3,\"c\":{},\"m\":3,\"v\":29}}\n",
        now - 4 * 3_600_000,
        now - 60_000,
        now - 30_000
    );
    std::fs::write(history.join("cpu-usage-pct__-__-.jsonl"), &records).expect("system input");
    std::fs::write(
        history.join(format!(
            "{}.jsonl",
            HistorySeriesKey::for_application(
                HistoryMetric::ApplicationCpuUsagePct,
                ApplicationHistoryIdentity::verified_launcher("io.example.Terminal")
                    .expect("application identity")
            )
            .file_stem()
        )),
        &records,
    )
    .expect("application input");
    let host = NativeAppHost::from_paths(
        root.join("config.json"),
        history,
        LocalTimeRulesObservation::unsupported(0),
    );
    let mut app = demo_app();
    app.config_client = Some(host.config_client().expect("configuration client"));
    app.load_config();
    app.install_history_frontend_connector(host.history_frontend_connector());
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    press(&mut app, 'r');
    assert!(
        !app.history_replay_open(),
        "a disabled reader has no fake review entry"
    );
    app.toggle_settings();
    app.settings_form.field = 29;
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.history_persistence_enabled(),
        "the normal form apply changes the runtime preference"
    );
    wait_for_rows(&mut app);
    press(&mut app, 'r');
    assert!(app.history_replay_open());
    assert_eq!(
        app.performance_history_projection().rows[0].peak_value,
        Some(29.0)
    );
    let old = app.performance_history_projection().source_request;
    press(&mut app, '2');
    wait_for_rows(&mut app);
    let wide = app.performance_history_projection();
    assert_eq!(wide.rows_window, Some(HistoryWindow::TwentyFourHours));
    assert_eq!(wide.rows[0].peak_value, Some(83.0));
    assert_ne!(wide.source_request, old);
    press(&mut app, 'r');
    assert!(!app.history_replay_open());
    assert_eq!(
        app.application_history_projection().rows.len(),
        1,
        "closing the performance presentation retains the shared reader"
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::AppHistory));
    let old = app.application_history_projection().source_request;
    press(&mut app, 'f');
    wait_for_rows(&mut app);
    assert_ne!(
        app.application_history_projection().source_request,
        old,
        "refresh works on the normal History route too"
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(54, 16))
        .expect("native-size terminal");
    terminal
        .draw(|frame| crate::render(frame, &app, crate::TuiTheme::from_theme(&Theme::dark())))
        .expect("paint the actual app frame");
    let painted = terminal.backend().to_string();
    assert!(
        painted.contains("83.0%") && painted.contains("Refresh"),
        "the complete root frame must present queried metrics and pinned actions: {painted}"
    );
    app.toggle_settings();
    app.settings_form.field = 29;
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!app.history_persistence_enabled());
    assert!(!app.history_replay_available());
    assert!(!app.history_replay_open());
    assert!(app.performance_history_projection().rows.is_empty());
    drop(app);
    drop(host);
    let _ = std::fs::remove_dir_all(root);
}
