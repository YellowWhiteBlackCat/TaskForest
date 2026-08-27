//! Settings-form navigation/apply/cancel and the snapshot-export (`x`) key
//! (success path and the honest error when no snapshot is loaded).

use super::super::*;
use taskmanager_application::AppAction;
use taskmanager_application::ConfigStore;

fn wait_for_config(app: &mut TuiApp, predicate: impl Fn(&taskmanager_application::Config) -> bool) {
    for _ in 0..64 {
        let drain = app
            .config_client
            .as_mut()
            .expect("injected config client")
            .wait_for_drain(std::time::Duration::from_secs(2));
        match drain {
            taskmanager_application::ConfigDrain::Empty => {}
            taskmanager_application::ConfigDrain::Publications(publications) => {
                for publication in publications {
                    app.apply_config_publication(&publication);
                }
            }
            taskmanager_application::ConfigDrain::ResyncRequired { latest, .. } => {
                app.apply_config_publication(&latest);
            }
        }
        if predicate(&app.config_draft) {
            return;
        }
    }
    panic!("configuration predicate was not published");
}

#[test]
fn pristine_first_launch_applies_defaults_without_a_recovery_notice() {
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-config-pristine-{}",
        std::process::id()
    ));
    let mut app = crate::demo_app();
    app.shell.clear_feedback_notice();
    crate::ui::test_support::install_config_store(&mut app, dir.join("config.json"));

    assert_eq!(app.config_draft, taskmanager_application::Config::default());
    assert!(app.shell.feedback_notice().is_none());

    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn settings_form_navigates_and_ent_applies_without_platform_effect() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('p'),
            KeyModifiers::NONE,
        ),
    );
    // Tab moves to the mode field; Right steps Dark -> EyeForest -> System
    // -> Light (wrapping), so three steps land on Light.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    for _ in 0..3 {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Right,
                KeyModifiers::NONE,
            ),
        );
    }
    assert_eq!(app.settings_form.mode, 0, "mode steps to Light");

    // Enter applies and closes; no platform effect is produced.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none());
    assert!(!app.settings_open());
}

#[test]
fn dirty_settings_form_enters_conflict_on_disjoint_external_revision() {
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-config-conflict-disjoint-{}",
        std::process::id()
    ));
    let path = dir.join("config.json");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.begin_settings_edit();
    app.settings_form.skin = 1;

    let store = ConfigStore::new(&path);
    let mut external = store.load_or_default();
    external.show_gpus = false;
    store.save(&external).unwrap();
    app.config_client.as_ref().unwrap().try_refresh().unwrap();
    wait_for_config(&mut app, |config| !config.show_gpus);

    assert!(matches!(
        app.settings_draft,
        crate::preferences::SettingsDraftLifecycle::Conflict { .. }
    ));
    assert_eq!(app.settings_form.skin, 1, "dirty form remains visible");
    assert!(
        !app.prefs.show[9],
        "external fact reaches runtime projection"
    );
    assert!(!app.apply_settings_form(), "conflicted draft cannot submit");
    app.cancel_settings();
    assert_eq!(app.settings_form.skin, 0, "cancel reloads latest canonical");
    assert!(!app.settings_form.show[9]);

    app.begin_settings_edit();
    app.settings_form.skin = 1;
    assert!(app.apply_settings_form());
    wait_for_config(&mut app, |config| config.skin == "KDE" && !config.show_gpus);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dirty_settings_form_does_not_overwrite_same_field_external_change() {
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-config-conflict-same-{}",
        std::process::id()
    ));
    let path = dir.join("config.json");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.begin_settings_edit();
    app.settings_form.skin = 1;

    let store = ConfigStore::new(&path);
    let mut external = store.load_or_default();
    external.skin = "Windows".into();
    store.save(&external).unwrap();
    app.config_client.as_ref().unwrap().try_refresh().unwrap();
    wait_for_config(&mut app, |config| config.skin == "Windows");

    assert!(!app.apply_settings_form());
    assert_eq!(ConfigStore::new(&path).load().unwrap().skin, "Windows");
    app.cancel_settings();
    assert_eq!(app.settings_form.skin_token(), "Windows");
    assert!(matches!(
        app.settings_draft,
        crate::preferences::SettingsDraftLifecycle::Clean { .. }
    ));
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_form_reverse_keys_walk_back_fields_and_values() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('p'),
            KeyModifiers::NONE,
        ),
    );
    // Tab to the mode field (1), then Right steps its value forward and Left
    // steps it back. The default mode is 1 (Dark), so Right → 2 and Left → 1.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Tab, KeyModifiers::NONE),
    );
    assert_eq!(app.settings_form.field, 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Right,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.settings_form.mode, 2, "Right must step the mode value");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Left, KeyModifiers::NONE),
    );
    assert_eq!(
        app.settings_form.mode, 1,
        "Left must step the mode value back"
    );

    // BackTab (Shift+Tab) moves to the PREVIOUS field (saturating, not
    // wrapping): from field 1 it returns to field 0.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::BackTab,
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(
        app.settings_form.field, 0,
        "BackTab must return to the first field"
    );
    // Up moves to the previous field too (the same reverse walk).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Up, KeyModifiers::NONE),
    );
    assert_eq!(
        app.settings_form.field, 0,
        "Up at the first field saturates in place"
    );
}

#[test]
fn settings_esc_cancels_without_applying_changes() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('p'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Right,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.settings_form.skin, 1, "GNOME -> KDE");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(!app.settings_open());
    assert_eq!(app.settings_form.skin, 0, "cancel reverts the form");
}

#[test]
fn export_key_records_feedback_without_a_platform_effect() {
    taskmanager_test_support::pin_english();
    let mut app = crate::demo_app();
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('x'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none());
    let feedback = app.feedback_notice().expect("export feedback");
    assert_eq!(
        feedback.source(),
        taskmanager_shell::FeedbackSource::Persistence
    );
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Error
    );
    assert_eq!(feedback.text(), "Snapshot export is unavailable");
}

#[test]
fn export_without_a_snapshot_reports_an_honest_error() {
    let mut app = TuiApp::new();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('x'),
            KeyModifiers::NONE,
        ),
    );
    let feedback = app.feedback_notice().expect("export feedback");
    assert_eq!(
        feedback.source(),
        taskmanager_shell::FeedbackSource::Persistence
    );
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Warning
    );
}

#[test]
fn refresh_interval_field_applies_to_the_shared_policy_and_persists() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-refresh-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");
    let dir = path.parent().expect("temp dir parent").join(format!(
        "taskmanager-tui-refresh-dir-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.cancel_settings(); // seed the form from the (default) config

    // Default cadence is 1 s.
    assert_eq!(
        app.shell.telemetry_interval().duration(),
        Duration::from_secs(1)
    );
    assert_eq!(app.settings_form.refresh_ms(), 1_000);

    // Step the refresh field (7) to 0.5 s and save.
    app.settings_form.field = 7;
    app.settings_form.step_value(-1); // 1000 -> 500
    assert_eq!(app.settings_form.refresh_ms(), 500);
    assert!(app.apply_settings_form(), "save must succeed");
    wait_for_config(&mut app, |config| config.refresh_ms == 500);
    assert_eq!(
        app.shell.telemetry_interval().duration(),
        Duration::from_millis(500),
        "the saved interval must reach the shared cadence authority"
    );

    // A fresh app pointed at the same config restores the interval through
    // the composition-edge restore path.
    let mut reloaded = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut reloaded, path.clone());
    assert_eq!(reloaded.settings_form.refresh_ms(), 500);
    assert_eq!(
        reloaded.shell.telemetry_interval().duration(),
        Duration::from_millis(500)
    );
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

#[test]
fn refresh_field_wraps_across_the_four_choices() {
    let mut form = crate::ui::settings::SettingsForm {
        field: 7,
        ..Default::default()
    };
    assert_eq!(form.refresh_ms(), 1_000);
    form.step_value(-1);
    assert_eq!(form.refresh_ms(), 500);
    form.step_value(-1);
    assert_eq!(form.refresh_ms(), 5_000, "wraps to the last choice");
    form.step_value(1);
    assert_eq!(form.refresh_ms(), 500);
    assert_eq!(crate::ui::settings::REFRESH_MS.len(), 4);
}

#[test]
fn device_visibility_preferences_filter_the_digit_selector() {
    let mut app = crate::demo_app();
    // Seed the demo with a battery and a fan so the full seven-resource rail
    // renders (the demo fixture carries neither).
    let mut battery = taskmanager_application::BatteryInfo::new(
        "battery:demo:BAT0",
        taskmanager_application::DeviceState::healthy(1),
    );
    battery.status = "Discharging".into();
    battery.apply_scalar_observations(taskmanager_application::BatteryScalarObservations {
        capacity_pct: taskmanager_application::ScalarObservation::available(80, 1),
        ..Default::default()
    });
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(
            taskmanager_application::PowerSupplySnapshot {
                state: taskmanager_application::DeviceState::healthy(1),
                timestamp_ms: 1,
                batteries: vec![battery],
                ..Default::default()
            },
        )),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(
            taskmanager_application::SensorCenterSnapshot {
                state: taskmanager_application::DeviceState::healthy(1),
                timestamp_ms: 1,
                readings: vec![
                    taskmanager_application::SensorReading::from_measurement_observation(
                        "hwmon:demo:cpu".into(),
                        "fan1".into(),
                        "CPU Fan".into(),
                        taskmanager_application::SensorMeasurementObservation::available(
                            taskmanager_application::SensorDescriptor::fan_speed(
                                taskmanager_application::SensorScale::IDENTITY,
                            ),
                            taskmanager_application::SensorMagnitude::Unsigned(1200),
                            1,
                        )
                        .expect("valid fan fixture"),
                    ),
                ],
                ..Default::default()
            },
        )),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));

    // Default: all seven resources are reachable.
    let all = app.visible_perf_devices();
    assert_eq!(all.len(), 7);
    assert_eq!(
        app.select_perf_device_digit('1'),
        Some(crate::PerfDevice::Cpu)
    );
    assert_eq!(
        app.select_perf_device_digit('5'),
        Some(crate::PerfDevice::Gpu)
    );

    // Hide GPUs: the rail renumbers and digit 5 now lands on Battery.
    app.prefs.show[9] = false;
    let filtered = app.visible_perf_devices();
    assert!(!filtered.contains(&crate::PerfDevice::Gpu));
    assert_eq!(
        app.select_perf_device_digit('5'),
        Some(crate::PerfDevice::Battery)
    );

    // Hiding the whole network family removes the NIC too.
    app.prefs.show[3] = false;
    assert!(
        !app.visible_perf_devices()
            .contains(&crate::PerfDevice::Network)
    );
}

#[test]
fn device_visibility_and_unit_preferences_persist_and_restore() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-prefs-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.cancel_settings();

    // Flip a device family off, switch memory units to bits, and save.
    app.settings_form.show[9] = false; // GPUs hidden
    app.settings_form.units[0] = false; // memory in bits
    app.settings_form.units[1] = true; // base-2
    app.settings_form.gray_zero = true;
    assert!(app.apply_settings_form());
    wait_for_config(&mut app, |config| {
        !config.show_gpus && !config.memory_use_bytes && config.gray_zero_values
    });
    assert!(!app.prefs.show[9]);
    assert!(!app.prefs.units[0]);
    assert!(app.prefs.gray_zero);

    // Restore from disk through the composition edge.
    let mut reloaded = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut reloaded, path.clone());
    assert!(!reloaded.prefs.show[9], "GPU visibility restored");
    assert!(!reloaded.prefs.units[0], "memory-bits restored");
    assert!(reloaded.prefs.gray_zero, "gray-zero restored");
    assert!(
        !reloaded
            .visible_perf_devices()
            .contains(&crate::PerfDevice::Gpu),
        "the restored preference filters the rail"
    );
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

#[test]
fn startup_page_preference_opens_the_configured_page_at_launch() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-startup-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");

    // Write a startup-page token, then load it through the edge.
    let store = ConfigStore::new(&path);
    let mut config = store.load_or_default();
    config.startup_page = "apps".to_string();
    store.save(&config).expect("fixture save");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    assert_eq!(
        app.page(),
        AppPage::Applications,
        "the startup-page token must open the configured page"
    );

    // A page switch records the remember-last token.
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    let stored = ConfigStore::new(&path).load_or_default();
    assert_eq!(stored.last_page, "performance");
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

#[test]
fn graph_data_points_preference_scales_the_shared_history_store() {
    use taskmanager_shell::history::MetricSeries;
    // The applied preference drives the SHARED rolling store at the
    // composition edge (G-02): every headline/trend series the TUI renders
    // reads the shell `MetricHistory`, so the persisted graph window must
    // reach ITS capacity — and the window keeps the newest samples only.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-graph-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");
    let store = ConfigStore::new(&path);
    let mut config = store.load_or_default();
    config.graph_data_points = 300;
    store.save(&config).expect("fixture save");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    assert_eq!(app.prefs.graph_points, 300);
    assert_eq!(
        app.history.capacity(),
        300,
        "the persisted window must reach the shared store's capacity"
    );
    // The store honors the capacity: 305 snapshots into a 300-capacity
    // window keep exactly the newest 300.
    let snapshot = app
        .projection()
        .snapshot
        .clone()
        .expect("demo carries a snapshot");
    for tick in 0..305u64 {
        let mut point = snapshot.clone();
        point.timestamp_ms = 1_785_292_800_000 + tick;
        let mut observations = point.cpu.scalar_observations().clone();
        observations.global_usage_pct =
            taskmanager_application::ScalarObservation::available(tick as f32, point.timestamp_ms);
        point.cpu.apply_scalar_observations(observations);
        taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &point, None, None);
    }
    let cpu = app.history.series(MetricSeries::CpuUsagePercent);
    assert_eq!(cpu.len(), 300, "the shared window is bounded");
    assert_eq!(cpu.last(), Some(&304.0), "the window keeps the newest tail");
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

#[test]
fn graph_points_field_persists_and_restores() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-graphfield-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.cancel_settings();
    app.settings_form.field = 25;
    app.settings_form.step_value(2); // 60 -> 300
    assert_eq!(app.settings_form.graph_points(), 300);
    assert!(app.apply_settings_form());
    wait_for_config(&mut app, |config| config.graph_data_points == 300);
    assert_eq!(app.prefs.graph_points, 300);
    let mut reloaded = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut reloaded, path.clone());
    assert_eq!(reloaded.prefs.graph_points, 300);
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

#[test]
fn notification_fields_persist_and_reach_the_shared_alert_center() {
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-notify-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.cancel_settings();
    // Enable notifications (field 26) and set 22:00-07:00 quiet hours.
    app.settings_form.field = 26;
    app.settings_form.step_value(1);
    assert!(app.settings_form.notify_enabled);
    app.settings_form.field = 27;
    app.settings_form.step_value(22); // 0 -> 22
    app.settings_form.field = 28;
    app.settings_form.step_value(7); // 0 -> 7
    assert_eq!(app.settings_form.quiet_start, 22);
    assert_eq!(app.settings_form.quiet_end, 7);
    assert!(app.apply_settings_form(), "save must succeed");
    wait_for_config(&mut app, |config| {
        config.notify_enabled && config.notify_quiet_hours == Some((22 * 60, 7 * 60))
    });
    let policy = app.shell.projection().alert_center.policy();
    assert!(policy.enabled, "opt-in must reach the shared alert center");
    let hours = policy.quiet_hours.expect("quiet hours persisted");
    assert_eq!(hours.start_minutes, 22 * 60);
    assert_eq!(hours.end_minutes, 7 * 60);

    // A fresh app pointed at the same config restores the same form state.
    let mut reloaded = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut reloaded, path.clone());
    assert!(reloaded.settings_form.notify_enabled);
    assert_eq!(reloaded.settings_form.quiet_start, 22);
    assert_eq!(reloaded.settings_form.quiet_end, 7);
    assert!(std::fs::remove_dir_all(dir).is_ok());
}

/// G-22: the Language selector persists through `Config::language` and
/// re-applies at the composition edge. Holds the language test guard across
/// the whole cycle (the choice flips the process-global i18n bundle, which
/// would otherwise leak into concurrently-rendering English assertions) and
/// restores English on the way out.
#[test]
fn language_choice_round_trips_through_the_settings_path() {
    use taskmanager_application::i18n::{Language, current_language, set_language};
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-lang-{}-{unique}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("config.json");

    // Step the language field (6) to 中文 and save: the write-through must
    // persist the token AND apply the bundle immediately.
    let mut app = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut app, path.clone());
    app.cancel_settings();
    app.settings_form.field = 6;
    app.settings_form.step_value(1);
    assert_eq!(app.settings_form.language_token(), "zh");
    assert!(app.apply_settings_form(), "save must succeed");
    wait_for_config(&mut app, |config| config.language.as_deref() == Some("zh"));
    assert_eq!(
        current_language(),
        Language::Zh,
        "saving must apply the choice to the i18n bundle immediately"
    );

    // The persisted token round-trips through the config store.
    let stored = ConfigStore::new(&path).load_or_default();
    assert_eq!(stored.language.as_deref(), Some("zh"));

    // A fresh app re-applies the persisted choice at the composition edge.
    let mut reloaded = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut reloaded, path.clone());
    assert_eq!(reloaded.settings_form.language, 1, "the form restores zh");
    assert_eq!(
        current_language(),
        Language::Zh,
        "load_config must re-apply the persisted language"
    );

    // Cancel re-seeds from the persisted config too (a dismissed edit never
    // leaks).
    reloaded.settings_form.language = 0;
    reloaded.cancel_settings();
    assert_eq!(reloaded.settings_form.language, 1);

    // No recorded preference keeps the host-detected locale (the Config
    // contract): a default config never forces a language.
    let bare = dir.join("bare.json");
    let mut fresh = crate::demo_app();
    crate::ui::test_support::set_config_store_client(&mut fresh, bare.clone());
    set_language(Language::En);
    fresh.load_config();
    assert_eq!(fresh.settings_form.language, 0);
    assert_eq!(
        current_language(),
        Language::En,
        "an unrecorded preference must not force a language"
    );

    set_language(Language::En);
    assert!(std::fs::remove_dir_all(dir).is_ok());
}
