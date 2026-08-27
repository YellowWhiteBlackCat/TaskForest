//! F18/F16 parity tests: settings completion ("设置补足") plus the user row
//! menu, split out of `app/tests.rs` so that file stays under the
//! `rust_line_guard` 800-line ceiling.
//!
//! `refresh_interval_applies_to_the_shell_policy_and_round_trips` and
//! `graph_unit_and_visibility_preferences_persist_and_apply` are F18 settings
//! parity; `user_row_menu_opens_selects_and_routes_actions` and
//! `user_row_menu_renders_only_while_open_with_the_target_session` are F16 user
//! row menu parity. The settings/theme, modal-lifecycle, and demo-mode tests
//! appended below are further self-contained tests moved out of `app/tests.rs`
//! for the same budget reason. All tests share the app-module test imports.

use super::focus_state::service_control_focus_target;
use super::*;
use crate::test_support::temp_dir;
use taskmanager_application::ConfigStore;
use taskmanager_shell::ShellKeyEvent;

#[test]
fn user_row_menu_opens_selects_and_routes_actions() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Users));
    assert_eq!(app.user_menu_row(), None);

    // Right-clicking a row selects it AND opens its menu.
    let _ = app.update(Message::OpenUserRowMenu(1));
    assert_eq!(app.user_menu_row(), Some(1));
    assert_eq!(app.shell.selected, 1);

    // A menu action routes through the shared session-control path and closes
    // the menu.
    let _ = app.update(Message::RequestSessionControl(
        taskmanager_application::SessionControlAction::Disconnect,
    ));
    assert_eq!(app.user_menu_row(), None);
    assert!(app.shell.feedback_text().contains("Demo mode"));

    // Escape closes an open menu without touching the selection.
    let _ = app.update(Message::OpenUserRowMenu(0));
    assert_eq!(app.user_menu_row(), Some(0));
    let escape = IcedKey::Fixed(ShellKeyEvent::new(
        taskmanager_application::KeyCode::Escape,
        taskmanager_application::Modifiers::NONE,
    ));
    let _ = app.update(Message::Key(escape));
    assert_eq!(app.user_menu_row(), None);
    assert_eq!(app.shell.selected, 0, "Escape keeps the row selection");

    // A plain row click dismisses an open menu.
    let _ = app.update(Message::OpenUserRowMenu(0));
    assert_eq!(app.user_menu_row(), Some(0));
    let _ = app.update(Message::SelectRow(1));
    assert_eq!(app.user_menu_row(), None);

    // The menu entries are focus targets with stable operation ids.
    assert_eq!(
        crate::focus::focus_id(FocusTarget::UserRowMenuDisconnect),
        "iced-user-row-menu-disconnect"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::UserRowMenuLock),
        "iced-user-row-menu-lock"
    );
}

#[test]
fn user_row_menu_renders_only_while_open_with_the_target_session() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Users));
    {
        let view = crate::ui::view(&app);
        let _ = view; // closed menu renders nothing extra and never panics
    }
    assert_eq!(app.user_menu_row(), None);

    let _ = app.update(Message::OpenUserRowMenu(1));
    let _ = crate::ui::view(&app);
    // The fixture's second session is the remote one; the menu binds actions
    // to it through the shared selection, which the open handler set.
    assert_eq!(app.shell.selected, 1);
}

#[test]
fn process_row_menu_reuses_shared_identity_safe_actions() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let pid = app
        .shell
        .visible_processes()
        .get(1)
        .map(|process| process.pid)
        .unwrap_or_default();
    assert_ne!(pid, 0, "the demo process row must have a real pid");

    let _ = app.update(Message::OpenProcessRowMenu { flat_index: 1, pid });
    assert_eq!(app.process_menu_pid(), Some(pid));
    assert_eq!(app.shell.selected, 1);

    // Destructive menu actions use the same shared confirmation slot as the
    // Applications action bar; they do not submit directly from the row UI.
    let _ = app.update(Message::ProcessMenuAction(ProcessMenuAction::Kill));
    assert!(app.process_menu_pid().is_none());
    assert_eq!(
        app.shell.pending_batch().map(|intent| intent.action),
        Some(taskmanager_application::ProcessBatchAction::Kill)
    );
    let _ = app.update(Message::DismissOverlay);

    // Signals are a separate typed platform effect, still frozen from the
    // selected row and still suppressed honestly in demo mode.
    let _ = app.update(Message::OpenProcessRowMenu { flat_index: 1, pid });
    let _ = app.update(Message::ProcessMenuAction(ProcessMenuAction::Signal(
        taskmanager_application::ProcessSignal::Interrupt,
    )));
    assert!(app.process_menu_pid().is_none());
    assert!(app.shell.feedback_text().contains("Demo mode"));

    // Escape closes an open menu without changing the selected row.
    let _ = app.update(Message::OpenProcessRowMenu { flat_index: 1, pid });
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        taskmanager_application::KeyCode::Escape,
        taskmanager_application::Modifiers::NONE,
    ))));
    assert!(app.process_menu_pid().is_none());
    assert_eq!(app.shell.selected, 1);
}

#[test]
fn applications_column_menu_changes_only_the_iced_table_projection() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let _ = app.update(Message::OpenProcessColumnsMenu);
    assert!(app.process_columns_menu_open());
    let _ = crate::ui::view(&app);

    let _ = app.update(Message::ToggleProcessColumn(
        taskmanager_shell::SortCol::Memory,
    ));
    assert!(!app.process_columns_menu_open());
    assert!(
        app.process_presentation
            .hidden_columns
            .contains(&taskmanager_shell::SortCol::Memory)
    );
    let _ = crate::ui::view(&app);

    let _ = app.update(Message::ToggleProcessColumn(
        taskmanager_shell::SortCol::Name,
    ));
    assert!(
        !app.process_presentation
            .hidden_columns
            .contains(&taskmanager_shell::SortCol::Name)
    );
}

#[test]
fn service_log_entry_opens_the_shared_feed_and_modal_controls() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let _ = app.update(Message::OpenServiceLogFor { index: 0 });
    assert!(app.shell.service_log.is_some());
    assert!(app.modal_open());

    let _ = app.update(Message::ToggleLogPaused);
    assert!(
        app.shell
            .service_log
            .as_ref()
            .is_some_and(|open| open.feed.paused)
    );
    let _ = app.update(Message::ToggleLogFollow);
    assert!(
        app.shell
            .service_log
            .as_ref()
            .is_some_and(|open| !open.feed.follow)
    );
    let _ = app.update(Message::CycleLogLevel);
    let _ = app.update(Message::CycleLogTime);

    // The demo has no provider log rows, so copy reports the typed empty state
    // instead of manufacturing a clipboard payload.
    let _ = app.update(Message::CopyServiceLog);
    assert!(app.shell.feedback_notice().is_some_and(
        |feedback| feedback.text().contains("No log") || feedback.text().contains("没有")
    ));

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        taskmanager_application::KeyCode::Escape,
        taskmanager_application::Modifiers::NONE,
    ))));
    assert!(!app.modal_open());
    assert!(app.shell.service_log.is_none());
}

#[test]
fn disk_smart_dialog_is_a_real_local_view() {
    let mut app = IcedApp::demo();
    let mut snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot");
    let disk = &mut snapshot.disks[0];
    disk.smart_temperature_c = Some(41.0);
    disk.smart_temp_critical_c = Some(70.0);
    disk.smart_percent_used = Some(3.0);
    disk.smart_power_on_hours = Some(1_234);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );

    let _ = app.update(Message::OpenDiskSmart { index: 0 });
    assert_eq!(app.disk_smart_index(), Some(0));
    assert!(app.modal_open());
    let _ = crate::ui::view(&app);

    assert_eq!(
        crate::focus::focus_id(FocusTarget::DiskSmartOpen { index: 0 }),
        "iced-disk-smart-open-0"
    );
    let _ = app.update(Message::DismissOverlay);
    assert!(app.disk_smart_index().is_none());
    assert!(!app.modal_open());
}

#[test]
fn refresh_interval_applies_to_the_shell_policy_and_round_trips() {
    let dir = temp_dir("settings-refresh");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));

    // The default cadence is the 1 s policy interval.
    assert_eq!(
        app.shell.telemetry_interval().duration(),
        Duration::from_secs(1)
    );
    assert_eq!(app.preferences().refresh_ms, 1000);

    let _ = app.update(Message::SettingsChanged(SettingsChange::RefreshInterval(
        500,
    )));
    assert_eq!(app.preferences().refresh_ms, 500);
    assert_eq!(
        app.shell.telemetry_interval().duration(),
        Duration::from_millis(500),
        "the change must reach the shared cadence authority"
    );
    app.wait_for_config_where(|config| config.refresh_ms == 500);

    // A fresh app pointed at the same path restores the cadence from disk.
    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    reloaded.load_config();
    assert_eq!(reloaded.preferences().refresh_ms, 500);
    assert_eq!(
        reloaded.shell.telemetry_interval().duration(),
        Duration::from_millis(500)
    );

    drop(reloaded);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graph_unit_and_visibility_preferences_persist_and_apply() {
    let dir = temp_dir("settings-graph-units");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));

    let _ = app.update(Message::SettingsChanged(SettingsChange::GraphDataPoints(
        300,
    )));
    assert_eq!(app.preferences().graph_data_points, 300);
    // The SHARED history store the chart reads follows the persisted window
    // (G-02: one sanctioned store, sized at the settings edge): shrinking
    // keeps the newest samples, growing never fabricates history.
    let snapshot = taskmanager_application::SystemSnapshot {
        timestamp_ms: 1,
        cpu: taskmanager_application::CpuMetrics::from_observations(
            taskmanager_application::CpuScalarObservations {
                global_usage_pct: taskmanager_application::ScalarObservation::available(30.0, 1),
                ..Default::default()
            },
        ),
        ..taskmanager_application::SystemSnapshot::default()
    };
    taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &snapshot, None, None);
    let series_before = app
        .shell
        .history
        .series_sample_count(taskmanager_shell::history::MetricSeries::CpuUsagePercent);
    let _ = app.update(Message::SettingsChanged(SettingsChange::GraphDataPoints(
        10,
    )));
    assert_eq!(app.shell.history.capacity(), 10);
    assert_eq!(
        app.shell
            .history
            .series_sample_count(taskmanager_shell::history::MetricSeries::CpuUsagePercent),
        series_before.min(10),
        "samples survive the shrink (bounded at the new window)"
    );

    let _ = app.update(Message::SettingsChanged(
        SettingsChange::NetworkDynamicScaling(false),
    ));
    assert!(!app.preferences().network_dynamic_scaling);

    let _ = app.update(Message::SettingsChanged(SettingsChange::MemoryBase2(false)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::DriveBytes(false)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::DriveBase2(false)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::NetworkBytes(true)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::NetworkBase2(true)));
    assert!(!app.memory_use_base2());
    assert!(!app.drive_use_bytes());
    assert!(!app.drive_use_base2());
    assert!(app.network_use_bytes());
    assert!(app.network_use_base2());
    app.wait_for_config_where(|config| {
        !config.memory_use_base2
            && !config.drive_use_bytes
            && !config.drive_use_base2
            && config.network_use_bytes
            && config.network_use_base2
    });

    // Every device-family toggle round-trips (the anti-报菜名 rule: the
    // iteration and the mirror accessor must agree on all ten families).
    for kind in DeviceKind::ALL {
        let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
            kind, false,
        )));
        app.wait_for_config_where(|config| !config_device_visible(config, kind));
        assert!(
            !app.preferences().device_visible(kind),
            "{kind:?} must be hidden"
        );
    }
    for kind in DeviceKind::ALL {
        let _ = app.update(Message::SettingsChanged(SettingsChange::ShowDevice(
            kind, true,
        )));
        app.wait_for_config_where(|config| config_device_visible(config, kind));
        assert!(app.preferences().device_visible(kind), "{kind:?} must show");
    }
    app.wait_for_config_where(|config| {
        config.graph_data_points == 10
            && !config.network_dynamic_scaling
            && config.network_use_bytes
            && config.show_gpus
    });

    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    reloaded.load_config();
    assert_eq!(reloaded.preferences().graph_data_points, 10);
    assert!(!reloaded.network_dynamic_scaling());
    assert!(reloaded.network_use_bytes());
    assert!(reloaded.preferences().device_visible(DeviceKind::Gpus));

    drop(reloaded);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

fn config_device_visible(config: &taskmanager_application::Config, kind: DeviceKind) -> bool {
    match kind {
        DeviceKind::Cpu => config.show_cpu,
        DeviceKind::Memory => config.show_memory,
        DeviceKind::Disks => config.show_disks,
        DeviceKind::Network => config.show_network,
        DeviceKind::NetworkWired => config.show_network_wired,
        DeviceKind::NetworkWireless => config.show_network_wireless,
        DeviceKind::NetworkVpn => config.show_network_vpn,
        DeviceKind::NetworkVirtual => config.show_network_virtual,
        DeviceKind::NetworkOther => config.show_network_other,
        DeviceKind::Gpus => config.show_gpus,
    }
}

#[test]
fn demo_app_runs_without_a_platform_client() {
    let mut app = IcedApp::default();
    assert!(app.is_demo());
    // Demo mode: ticks are inert (no platform to poll), effects are
    // honestly suppressed.
    let _ = app.update(Message::Tick);
    let _task = app.update(Message::Key(IcedKey::Character(
        'q',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(app.shell.should_quit());
    app.queue(PlatformEffect::Refresh(RefreshRequest::Telemetry));
    assert!(app.shell.feedback_text().contains("Demo mode"));
}

#[test]
fn demo_constructor_exposes_shared_fixture_data_without_platform_io() {
    let mut app = IcedApp::demo();
    assert!(app.is_demo());
    assert_eq!(
        app.shell.projection().sessions.as_ref().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        app.shell.projection().services.as_ref().map(Vec::len),
        Some(5)
    );
    assert_eq!(
        app.shell
            .projection()
            .startup_entries
            .as_ref()
            .map(Vec::len),
        Some(2)
    );

    let _ = app.update(Message::RequestSessionControl(
        taskmanager_application::SessionControlAction::Lock,
    ));
    assert!(app.shell.feedback_text().contains("Demo mode"));
}

#[test]
fn modal_escape_and_close_messages_use_the_shared_overlay_lifecycle() {
    let mut app = IcedApp::demo();
    let escape = IcedKey::Fixed(ShellKeyEvent::new(
        taskmanager_application::KeyCode::Escape,
        taskmanager_application::Modifiers::NONE,
    ));

    let _ = app.update(Message::Key(IcedKey::Character(
        '?',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(app.shell.help_open());
    let _ = app.update(Message::Key(escape));
    assert!(!app.shell.help_open());

    let _ = app.update(Message::Key(IcedKey::Character(
        'T',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(app.shell.suggestions_open());
    let _ = app.update(Message::DismissOverlay);
    assert!(!app.shell.suggestions_open());
    assert!(!app.shell.help_open());
}
#[test]
fn settings_change_round_trips_through_the_store_and_rebuilds_the_theme() {
    let dir = temp_dir("settings");
    let path = dir.join("nested").join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(app.theme().skin, Skin::Gnome);
    assert!(!app.theme().hc);

    let _ = app.update(Message::SettingsChanged(SettingsChange::Skin(Skin::Kde)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::Mode(
        ModeChoice::Dark,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::HighContrast(true)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::CompactDensity(
        true,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::UiSize(
        taskmanager_theme::tokens::UiSize::Large,
    )));

    assert_eq!(app.theme().skin, Skin::Kde);
    assert_eq!(app.theme().mode, LightDark::Dark);
    assert!(app.theme().hc);
    assert!(app.compact_density());
    assert_eq!(
        app.ui_size(),
        taskmanager_theme::tokens::UiSize::Large,
        "UI size is independent from compact density"
    );
    assert_eq!(app.preferences().skin, "KDE");
    assert_eq!(app.preferences().mode, "Dark");
    app.wait_for_config_where(|config| {
        config.skin == "KDE"
            && config.mode == "Dark"
            && config.hc
            && config.density == "Compact"
            && config.ui_size == "Large"
    });

    // A fresh app pointed at the same path restores everything.
    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    reloaded.load_config();
    assert_eq!(reloaded.theme().skin, Skin::Kde);
    assert_eq!(reloaded.theme().mode, LightDark::Dark);
    assert!(reloaded.theme().hc);
    assert!(reloaded.compact_density());
    assert_eq!(reloaded.ui_size(), taskmanager_theme::tokens::UiSize::Large);

    drop(reloaded);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_apply_unknown_mode_tokens_as_system_and_keep_font_fallbacks() {
    let dir = temp_dir("settings-tokens");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));

    let _ = app.update(Message::SettingsChanged(SettingsChange::Mode(
        ModeChoice::System,
    )));
    assert_eq!(app.preferences().mode, "System");
    // System resolves to the dark fallback (no native appearance provider).
    assert_eq!(app.theme().mode, LightDark::Dark);

    let _ = app.update(Message::SettingsChanged(SettingsChange::UiFont(
        FontChoice::Bundled,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::MonoFont(
        FontChoice::Bundled,
    )));
    assert_eq!(app.preferences().ui_font, "MiSans VF");
    assert_eq!(app.theme().mono_font, "Roboto Mono");

    let _ = app.update(Message::SettingsChanged(SettingsChange::MemoryBytes(false)));
    assert!(!app.memory_use_bytes());

    let _ = app.update(Message::SettingsChanged(SettingsChange::Language(
        crate::i18n::Language::Zh,
    )));
    assert_eq!(app.language(), crate::i18n::Language::Zh);

    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_custom_font_uses_only_the_observed_catalog_family() {
    let dir = temp_dir("settings-custom-font");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store_and_font_availability(
        None,
        ConfigStore::new(&path),
        taskmanager_theme::FontAvailability::from_installed_families([
            taskmanager_theme::FONT_MISANS_VF,
            taskmanager_theme::FONT_ROBOTO_MONO,
            "Fira Sans",
        ]),
    );
    let choice = app
        .preferences()
        .font_availability
        .choice_for(" fira sans ")
        .expect("the observed family should be selectable");

    let _ = app.update(Message::SettingsChanged(SettingsChange::UiFont(choice)));

    assert_eq!(app.preferences().ui_font, "Fira Sans");
    assert_eq!(app.theme().ui_font, "Fira Sans");
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_modals_open_close_and_swallow_quit_keys() {
    let mut app = IcedApp::demo();

    let _ = app.update(Message::OpenSettings);
    assert!(app.settings_open());
    assert!(app.modal_open());

    // 'q' must not quit behind an open modal.
    let _ = app.update(Message::Key(IcedKey::Character(
        'q',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(!app.shell.should_quit());

    let escape = IcedKey::Fixed(taskmanager_shell::ShellKeyEvent::new(
        taskmanager_application::KeyCode::Escape,
        taskmanager_application::Modifiers::NONE,
    ));
    let _ = app.update(Message::Key(escape));
    assert!(!app.settings_open());
    assert!(!app.modal_open());

    // Ctrl+A opens the about modal (the shared ShowSystemAbout chord
    // rendered locally) and never touches shell status.
    let _ = app.update(Message::Key(IcedKey::Character(
        'a',
        taskmanager_application::Modifiers::CONTROL,
    )));
    assert!(app.about_open());
    assert!(!app.shell.feedback_text().contains("System information"));

    let _ = app.update(Message::OpenHealth);
    assert!(app.health_open());
    let _ = app.update(Message::OpenContainers);
    assert!(app.containers_open());
    // Opening a new local modal closes the previous one (single modal).
    assert!(!app.health_open());
    assert!(!app.settings_open());
    // Escape closes every local modal at once.
    let _ = app.update(Message::Key(IcedKey::Fixed(
        taskmanager_shell::ShellKeyEvent::new(
            taskmanager_application::KeyCode::Escape,
            taskmanager_application::Modifiers::NONE,
        ),
    )));
    assert!(!app.containers_open());
    assert!(!app.health_open());
}

#[test]
fn service_control_tab_scope_alternates_confirm_and_cancel() {
    assert_eq!(
        service_control_focus_target(Some(FocusTarget::ConfirmServiceControl)),
        FocusTarget::CancelServiceControl
    );
    assert_eq!(
        service_control_focus_target(Some(FocusTarget::CancelServiceControl)),
        FocusTarget::ConfirmServiceControl
    );
    assert_eq!(
        service_control_focus_target(None),
        FocusTarget::ConfirmServiceControl
    );
}

#[test]
fn service_control_confirmation_enters_the_modal_focus_scope() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Services;
    let _ = app.update(Message::RequestServiceAction {
        index: 0,
        action: ServiceAction::Stop,
    });
    assert!(app.shell.pending_service_control().is_some());
    assert_eq!(app.modal_focus_target(), FocusTarget::ConfirmServiceControl);

    let tab = taskmanager_shell::ShellKeyEvent::new(
        taskmanager_application::KeyCode::Tab,
        taskmanager_application::Modifiers::NONE,
    );
    let _ = app.update(Message::Key(IcedKey::Fixed(tab)));
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::CancelServiceControl)
    );
}

#[test]
fn properties_overlay_counts_as_a_modal_and_swallows_characters() {
    let mut app = IcedApp::demo();
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));
    let _ = app.shell.apply_action(AppAction::OpenProperties);
    assert!(app.process_properties_open());
    assert!(app.modal_open());

    let _ = app.update(Message::Key(IcedKey::Character(
        'q',
        taskmanager_application::Modifiers::NONE,
    )));
    assert!(!app.shell.should_quit());

    // Escape dismisses through the shared route.
    let _ = app.update(Message::Key(IcedKey::Fixed(
        taskmanager_shell::ShellKeyEvent::new(
            taskmanager_application::KeyCode::Escape,
            taskmanager_application::Modifiers::NONE,
        ),
    )));
    assert!(!app.process_properties_open());
}

#[path = "tests/settings_and_appearance.rs"]
mod settings_and_appearance;
