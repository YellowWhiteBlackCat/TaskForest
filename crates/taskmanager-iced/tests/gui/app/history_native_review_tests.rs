//! test-intent: behavior
//! Real query completions and the normal Iced settings/message lifecycle.

use super::*;
use crate::app::SettingsChange;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use taskmanager_app_host::NativeAppHost;
use taskmanager_core::core::history::ApplicationHistoryIdentity;
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_platform_conformance::smoke_budget::{DRAIN_DEADLINE, DRAIN_POLL};

fn await_rows(app: &mut IcedApp) {
    let deadline = Instant::now() + DRAIN_DEADLINE;
    loop {
        app.drain_config_publications();
        app.drain_history_replay_completions();
        if !app.history_replay_state().rows().is_empty() && !app.history_replay_state().is_loading()
        {
            return;
        }
        assert!(Instant::now() < deadline, "the real query must complete");
        std::thread::sleep(DRAIN_POLL);
    }
}

#[test]
fn normal_history_settings_and_commands_use_real_queries_and_disable_discards_cached_curves() {
    let root = crate::test_support::repo_temp_dir().join("iced-history-review");
    let history = root.join("history");
    std::fs::create_dir_all(&history).expect("history directory");
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
    )
    .expect("stamp");
    let records = format!(
        "{{\"r\":1,\"c\":{},\"m\":1,\"v\":87}}\n{{\"r\":2,\"c\":{},\"m\":2,\"v\":11}}\n{{\"r\":3,\"c\":{},\"m\":3,\"v\":31}}\n",
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
                ApplicationHistoryIdentity::verified_launcher("io.example.Iced")
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
    let mut app = IcedApp::new_with_runtime_clients(
        None,
        Some(host.config_client().expect("config client")),
        None,
    );
    app.load_config();
    app.install_history_frontend_connector(host.history_frontend_connector());
    app.toggle_history_replay();
    assert!(
        !app.history_replay_state().is_open(),
        "disabled history cannot invent a review"
    );
    app.apply_settings_change(SettingsChange::ContinuousHistory(true));
    await_rows(&mut app);
    assert!(app.history_replay_entry_available());
    app.toggle_history_replay();
    assert!(app.history_replay_state().is_open());
    assert_eq!(app.history_replay_state().rows()[0].peak_value, Some(31.0));
    app.select_history_replay_window(HistoryWindow::TwentyFourHours);
    await_rows(&mut app);
    assert_eq!(
        app.history_replay_state().rows_window(),
        Some(HistoryWindow::TwentyFourHours)
    );
    assert_eq!(app.history_replay_state().rows()[0].peak_value, Some(87.0));
    let original = Rc::as_ptr(&app.history_replay_state().rows()[0].samples);
    app.refresh_history_replay();
    assert_eq!(
        Rc::as_ptr(&app.history_replay_state().rows()[0].samples),
        original,
        "refresh retains the last good curve until its correlated completion"
    );
    await_rows(&mut app);
    app.toggle_history_replay();
    assert!(!app.history_replay_state().is_open());
    assert_eq!(app.application_history_projection().rows.len(), 1);
    app.apply_settings_change(SettingsChange::ContinuousHistory(false));
    assert!(!app.history_replay_entry_available());
    assert!(!app.history_replay_state().is_open());
    assert!(
        app.history_replay_state().rows().is_empty(),
        "disabled persistence discards its renderer cache"
    );
    drop(app);
    drop(host);
    let _ = std::fs::remove_dir_all(root);
}
