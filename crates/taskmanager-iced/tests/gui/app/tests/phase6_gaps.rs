//! Phase 6 gap regression tests covering:
//! 1. Working Directory extraction
//! 2. Wi-Fi signal & connection formatting
//! 3. System spec Markdown export
//! 4. Visual table navigation with Home and End keys
//! 5. Shell level Home / End / PageUp / PageDown handling

use taskmanager_application::{
    AppPage, HardwareInfo, KeyCode, Modifiers, NetworkAdapterType, NetworkMetrics, SystemSnapshot,
};
use taskmanager_shell::{ShellApp, ShellKeyEvent};

use crate::app::{IcedApp, IcedKey, Message};
use crate::ui::overlays::process_details::{filtered_environment_rows, working_directory_value};
use crate::ui::perf_devices::network::{network_summary_lines, network_title};
use taskmanager_application::ProcessEnvironmentEntry;

#[test]
fn test_environment_filter_matches_keys_case_insensitively() {
    let entries = vec![
        ProcessEnvironmentEntry {
            key: "PATH".into(),
            value: "/usr/bin".into(),
        },
        ProcessEnvironmentEntry {
            key: "HOME".into(),
            value: "/home/<user>".into(),
        },
        ProcessEnvironmentEntry {
            key: "XDG_SESSION_ID".into(),
            value: "2".into(),
        },
    ];
    assert_eq!(filtered_environment_rows(&entries, "").len(), 3);
    let path = filtered_environment_rows(&entries, "path");
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].key, "PATH");
    assert!(
        filtered_environment_rows(&entries, "xdg")
            .iter()
            .all(|e| e.key == "XDG_SESSION_ID")
    );
    assert!(filtered_environment_rows(&entries, "nomatch").is_empty());
}

#[test]
fn working_directory_is_collecting_until_the_typed_insight_arrives() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let shell = ShellApp::default();
    let target = taskmanager_application::FrozenProcessIdentity::from_authoritative_parts(
        1, "init", 100, 1_000,
    )
    .expect("fixture identity");
    assert_eq!(working_directory_value(&shell, &target), "collecting…");
}

#[test]
fn test_network_wifi_signal_formatting() {
    let mut nic = NetworkMetrics::default();
    nic.ipv4_addr = Some("192.168.1.100".into());
    let wireless_observations = taskmanager_application::NetworkWirelessObservations {
        signal_dbm: taskmanager_application::OptionalObservation::present(-60, 0),
        ssid: taskmanager_application::OptionalObservation::present("HomeWiFi_5G".into(), 0),
        ..Default::default()
    };
    nic.apply_observations(
        NetworkAdapterType::WiFi,
        taskmanager_application::NetworkScalarObservations::default(),
        wireless_observations,
    );

    let rows = network_summary_lines(&nic, false, false);
    // GPUI parity: the SSID lives in the page TITLE, not the stats rail; the
    // signal row carries the dBm + quality fact.
    assert_eq!(
        network_title(&nic),
        "Wi-Fi: HomeWiFi_5G ()",
        "an associated wireless link surfaces its SSID as the page heading"
    );
    assert!(
        !rows.iter().any(|row| row.label() == "SSID"),
        "the SSID stats row is retired (it is the page title now)"
    );

    let sig_row = rows
        .iter()
        .find(|row| row.label() == taskmanager_application::i18n::t("common.signal"));
    assert!(sig_row.is_some());
    assert!(
        sig_row
            .unwrap()
            .value()
            .unwrap_or_default()
            .contains("-60 dBm")
    );
}

#[test]
fn test_wifi_quality_mapping_and_hardware_card_rows() {
    use crate::ui::perf_devices::network::{network_hardware_rows, wifi_signal_quality_percent};
    assert_eq!(wifi_signal_quality_percent(-90), 0.0);
    assert_eq!(wifi_signal_quality_percent(-60), 50.0);
    assert_eq!(wifi_signal_quality_percent(-30), 100.0);
    assert_eq!(wifi_signal_quality_percent(-120), 0.0, "clamped below");
    assert_eq!(wifi_signal_quality_percent(0), 100.0, "clamped above");

    let empty_nic = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .adapter_type(NetworkAdapterType::WiFi)
        .build();
    assert!(network_hardware_rows(&empty_nic).is_empty());
    let nic = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .adapter_type(NetworkAdapterType::WiFi)
        .adapter(Some("Intel Wi-Fi 6E AX211".into()))
        .driver(Some("iwlwifi".into()))
        .build();
    let rows = network_hardware_rows(&nic);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        (
            taskmanager_application::i18n::t("common.adapter").to_string(),
            "Intel Wi-Fi 6E AX211".to_string()
        )
    );
    assert_eq!(
        rows[1],
        (
            taskmanager_application::i18n::t("common.driver").to_string(),
            "iwlwifi".to_string()
        )
    );
}

#[test]
fn test_system_spec_export_formatting() {
    let hw = HardwareInfo {
        os_name: Some("Linux".to_string()),
        kernel_version: Some("6.10.0".to_string()),
        cpu_brand: Some("AMD Ryzen 9 7950X".to_string()),
        ..Default::default()
    };

    let snap = SystemSnapshot {
        uptime_secs: 3600,
        processes: 120,
        ..Default::default()
    };

    let export = crate::ui::system_table::format_system_spec_export(Some(&hw), Some(&snap), None);
    assert!(export.contains("# System Specifications"));
    assert!(export.contains("AMD Ryzen 9 7950X"));
    assert!(export.contains("6.10.0"));
    assert!(export.contains("120"));
}

#[test]
fn test_system_hardware_rows_include_panorama_facts() {
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    let hw = HardwareInfo {
        package_count: Some(1489),
        architecture: Some("x86_64".into()),
        motherboard_vendor: Some("ASUSTeK COMPUTER INC.".into()),
        motherboard_model: Some("ROG STRIX B760-I".into()),
        firmware_release_date: Some("08/01/2026".into()),
        secure_boot: Some(true),
        ..Default::default()
    };
    let rows = crate::ui::system_table::hardware_info_rows(&hw);
    let value = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .map(|row| row.value.as_str())
    };
    assert_eq!(value("Architecture"), Some("x86_64"));
    assert_eq!(value("Motherboard vendor"), Some("ASUSTeK COMPUTER INC."));
    assert_eq!(value("Motherboard model"), Some("ROG STRIX B760-I"));
    assert_eq!(value("Firmware release date"), Some("08/01/2026"));
    assert_eq!(value("Secure Boot"), Some("Enabled"));
    assert_eq!(value("Installed packages"), Some("1489"));
}

#[test]
fn test_shell_home_and_end_keys() {
    let mut shell = taskmanager_shell::demo_app();
    let _ = shell.apply_action(taskmanager_application::AppAction::SelectPage(
        AppPage::Applications,
    ));
    let count = shell.visible_process_count();
    assert!(count > 1);

    // Initial position
    shell.selected = 2;

    // Home
    let _ = shell.handle_local_key(ShellKeyEvent::new(KeyCode::Home, Modifiers::NONE));
    assert_eq!(shell.selected, 0);

    // End
    let _ = shell.handle_local_key(ShellKeyEvent::new(KeyCode::End, Modifiers::NONE));
    assert_eq!(shell.selected, count - 1);

    // PageUp
    let _ = shell.handle_local_key(ShellKeyEvent::new(KeyCode::PageUp, Modifiers::NONE));
    assert_eq!(shell.selected, (count - 1).saturating_sub(10));

    // PageDown
    let _ = shell.handle_local_key(ShellKeyEvent::new(KeyCode::PageDown, Modifiers::NONE));
    assert_eq!(shell.selected, count - 1);
}

#[test]
fn test_visual_navigation_home_and_end() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let mut procs = Vec::new();
    for pid in 1..=10 {
        let p = taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(format!("proc_{pid}"))
            .build();
        procs.push(p);
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(procs)),
    );
    app.process_presentation.visual_cursor = 5;
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Home,
        Modifiers::NONE,
    ))));
    assert_eq!(app.process_presentation.visual_cursor, 0);

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::End,
        Modifiers::NONE,
    ))));
    // The category header is the first visual row, followed by the ten tree
    // nodes, so End lands on visual row 10 rather than flat index 9.
    assert_eq!(app.process_presentation.visual_cursor, 10);
}

#[test]
fn ctrl_c_copies_the_selected_row_summary() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    app.shell.selected = 0;
    let summary = app.shell.selected_row_summary().expect("demo row summary");
    assert!(summary.contains('\t'));
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::C,
        Modifiers::CONTROL,
    ))));
    assert!(
        app.shell.feedback_text().contains("Selected Row"),
        "status feedback: {}",
        app.shell.feedback_text()
    );
}

#[test]
fn ctrl_c_is_inert_without_a_selectable_row() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(Vec::new())),
    );
    let before = app.shell.feedback_text().to_owned();
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::C,
        Modifiers::CONTROL,
    ))));
    assert_eq!(
        app.shell.feedback_text(),
        before,
        "no row → no clipboard feedback"
    );
}
