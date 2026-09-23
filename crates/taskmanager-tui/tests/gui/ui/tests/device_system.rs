//! System-page storage, locale, and diagnostic export render tests.

use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::ScalarObservation;

use super::frame_text;

/// The disk panel carries the typed device-health verdict (§2.3 B-1) and the
/// Removable row for proven removable media (§2.3 B-2); the network panel
/// carries the same health verdict (§2.4 B-1) while its carrier verdict stays
/// independently Connected — the demo NIC keeps its assigned IPv4 address.
#[test]
fn disk_and_network_panels_carry_device_health_and_proven_removability() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let disk = snapshot
            .as_mut()
            .expect("demo snapshot")
            .disks
            .first_mut()
            .expect("demo app should carry one disk");
        disk.device_state = DeviceState::healthy(1_000);
        disk.apply_attachment_capabilities(Some(true), None);
    });
    let disk_text = frame_text(&app, 140, 48);
    assert!(
        disk_text.contains("Status Healthy"),
        "the disk panel must render the typed device health:\n{disk_text}"
    );
    assert!(
        disk_text.contains("Removable Yes"),
        "proven removable media must render its row:\n{disk_text}"
    );

    app.perf_device = crate::PerfDevice::Network;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let network = snapshot
            .as_mut()
            .expect("demo snapshot")
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        network.device_state = DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(1_000),
        };
    });
    let network_text = frame_text(&app, 140, 48);
    assert!(
        network_text.contains("Status Stale data"),
        "the NIC panel must express degraded device health:\n{network_text}"
    );
    assert!(
        network_text.contains("Connected"),
        "the carrier verdict stays independent of device health:\n{network_text}"
    );
}

/// The new health/capability rows wrap inside a narrow frame instead of
/// overflowing it: the compact-width draw must keep the short verdict tokens
/// visible (wrapped lines break at spaces, never mid-token). Height 28 keeps
/// the disk detail itself on screen beside the 12-row directory-usage panel
/// the demo projects under the Disk tab.
#[test]
fn narrow_frames_keep_the_health_rows_wrapped_not_dropped() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Disk;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let disk = snapshot
            .as_mut()
            .expect("demo snapshot")
            .disks
            .first_mut()
            .expect("demo app should carry one disk");
        disk.device_state = DeviceState::healthy(1_000);
        disk.apply_attachment_capabilities(Some(true), None);
    });
    let disk_narrow = frame_text(&app, 54, 28);
    assert!(
        disk_narrow.contains("Healthy") && disk_narrow.contains("Yes"),
        "the narrow disk frame must keep the health and removable rows:\n{disk_narrow}"
    );

    app.perf_device = crate::PerfDevice::Network;
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let network = snapshot
            .as_mut()
            .expect("demo snapshot")
            .networks
            .first_mut()
            .expect("demo app should carry one NIC");
        network.device_state = DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: Some(1_000),
        };
    });
    let network_narrow = frame_text(&app, 54, 28);
    assert!(
        network_narrow.contains("Stale data"),
        "the narrow network frame must keep the health row:\n{network_narrow}"
    );
}

/// System page Storage section (§2.8 B-1, GPUI storage_section parity): one
/// static identity/capacity row per discovered disk carrying name,
/// capacity, available and type, reachable by scroll at reference and
/// compact sizes.
#[test]
fn system_page_renders_the_storage_section_from_snapshot_disks() {
    for (width, height) in [(120, 52), (54, 16)] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
        let mut visited = String::new();
        for offset in 0..80 {
            app.system_scroll = offset;
            visited.push_str(&frame_text(&app, width, height));
            visited.push('\n');
        }
        // Wrapping inflates the row count at narrow widths, so reach the tail
        // explicitly instead of relying on the offset range alone.
        app.system_scroll = usize::MAX;
        visited.push_str(&frame_text(&app, width, height));
        assert!(
            visited.contains("Storage"),
            "{width}×{height} System viewport never reached the storage section"
        );
        // Field quadruple: name, capacity, available, type. At width 120 the
        // whole row paints on one line; at width 54 the row word-wraps and the
        // bounded viewport clips the LAST entry's wrapped continuation (an
        // honest viewport bound, never an overflow), so the narrow pass
        // asserts the visible first chunk instead.
        let facts: &[&str] = if width > 80 {
            &["TiPro9000 2TB · 2000.0 GiB · 1240.0 GiB · NVMe SSD"]
        } else {
            &["TiPro9000 2TB · 2000.0 GiB ·"]
        };
        for fact in facts {
            assert!(
                visited.contains(fact),
                "{width}×{height} System viewport never reached storage fact {fact:?}"
            );
        }
    }
}

/// Unobserved byte figures in the storage row stay honest dashes — never a
/// fabricated total.
#[test]
fn system_storage_row_keeps_unobserved_byte_figures_as_dashes() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut disk = snapshot
            .disks
            .pop()
            .expect("demo app should carry one disk");
        let mut observations = *disk.scalar_observations();
        observations.capacity_bytes = ScalarObservation::default();
        observations.available_bytes = ScalarObservation::default();
        disk.apply_scalar_observations(observations);
        snapshot.disks.push(disk);
    });

    let mut visited = String::new();
    for offset in 0..80 {
        app.system_scroll = offset;
        visited.push_str(&frame_text(&app, 120, 52));
        visited.push('\n');
    }
    assert!(
        visited.contains("TiPro9000 2TB · — · — · NVMe SSD"),
        "unobserved capacity/available must render honest dashes:\n{visited}"
    );
    assert!(
        !visited.contains("2000.0 GiB"),
        "no fabricated byte total may render:\n{visited}"
    );
}

/// Render one frame in an explicit language. Caller holds `LANG_TEST_GUARD`
/// (`frame_text` would re-lock the non-reentrant mutex).
fn frame_text_in_language(app: &crate::TuiApp, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, crate::TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

/// The storage section title resolves through the shared catalog in the
/// active locale: English under En, Chinese under Zh, never hardcoded copy.
#[test]
fn system_storage_section_follows_the_active_locale() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    app.system_scroll = usize::MAX;
    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let en_frame = frame_text_in_language(&app, 120, 52);
    let en_title = taskmanager_application::i18n::t("system.section.storage");

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::Zh);
    let zh_frame = frame_text_in_language(&app, 120, 52);
    let zh_title = taskmanager_application::i18n::t("system.section.storage");
    drop(guard);

    assert_ne!(
        en_title, zh_title,
        "system.section.storage must translate to distinct En/Zh copy"
    );
    assert!(
        en_frame.contains(en_title),
        "the En tail frame must paint {en_title:?}:\n{en_frame}"
    );
    assert!(
        zh_frame.contains(zh_title),
        "the Zh tail frame must paint {zh_title:?}:\n{zh_frame}"
    );
    // The disk identity value itself is locale-independent typed data.
    assert!(
        zh_frame.contains("TiPro9000 2TB"),
        "the Zh tail frame keeps the typed disk identity:\n{zh_frame}"
    );
}

#[test]
fn system_sections_include_smbios_memory_slots_when_provided() {
    use taskmanager_core::core::metrics::{SmbiosMemorySnapshot, SmbiosModuleRow};

    let smbios = SmbiosMemorySnapshot {
        slots_total: 4,
        slots_used: 2,
        modules: vec![SmbiosModuleRow {
            slot: 0,
            size_mb: Some(32768),
            configured_speed_mts: Some(5600),
            manufacturer: Some("Kingston".into()),
            part_number: Some("KF556C40BB-32".into()),
            locator: Some("DIMM_B1".into()),
            memory_type: Some("DDR5".into()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let sections = crate::ui::pages::system_data::system_sections(None, None, None, Some(&smbios));
    drop(guard);

    let slots_sec = sections
        .iter()
        .find(|s| s.title == taskmanager_application::i18n::t("system.memory_slots"))
        .expect("memory slots section must exist");
    assert!(slots_sec.facts.iter().any(|f| f.label == "DIMM_B1"));
    assert!(
        slots_sec
            .facts
            .iter()
            .any(|f| f.value.contains("5600 MT/s"))
    );
}

#[test]
fn diagnostic_failure_feedback_keys_stay_localized_and_keep_paths_private() {
    use crate::ui::pages::system_data::{
        diagnostic_failure_feedback_key, diagnostic_failure_message,
    };
    use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};

    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    // Error kind mappings to localized feedback keys, with raw sensitive paths
    // never interpolated into the user-facing message.
    let error_cases = [
        (
            DiagnosticBundleErrorKind::InvalidSource,
            "diagnostics.failure_invalid_source",
        ),
        (
            DiagnosticBundleErrorKind::InvalidTarget,
            "diagnostics.failure_invalid_target",
        ),
        (
            DiagnosticBundleErrorKind::Encode,
            "diagnostics.failure_encode",
        ),
        (DiagnosticBundleErrorKind::Io, "diagnostics.failure_io"),
        (DiagnosticBundleErrorKind::Busy, "diagnostics.failure_busy"),
        (
            DiagnosticBundleErrorKind::Unavailable,
            "diagnostics.failure_unavailable",
        ),
    ];

    for (kind, key) in error_cases {
        assert_eq!(diagnostic_failure_feedback_key(kind), key);
        let error = DiagnosticBundleError::with_detail(kind, "/root/super/secret/path");
        let msg = diagnostic_failure_message(&error);
        assert!(msg.contains(taskmanager_application::i18n::t(key)));
        assert!(
            !msg.contains("secret"),
            "raw sensitive paths must never leak into feedback"
        );
    }

    drop(guard);
}
