use super::apply_capture_target;
use crate::app::{IcedApp, LocalSurfaceKind, Message};

#[test]
fn startup_failure_scene_requires_failed_units_and_their_boot_chain() {
    use std::time::Instant;
    use taskmanager_application::AppPage;
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
    let mut app = IcedApp::demo();
    apply_capture_target(&mut app, "startup-failure-evidence");
    assert_eq!(app.shell.page(), AppPage::Startup);
    let evidence = app
        .shell
        .projection()
        .startup_boot_evidence
        .as_ref()
        .expect("boot evidence");
    assert_eq!(evidence.failed_units.len(), 3);
    assert_eq!(evidence.critical_chain.len(), 2);
    assert!(
        evidence
            .failed_units
            .iter()
            .all(|unit| unit.active_state == "failed")
    );
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::StartupBootEvidence(None),
    );
    for _ in 0..3 {
        let _ = app.update(Message::Frame(Instant::now()));
    }
    assert!(
        !app.capture.emitted,
        "an empty boot panel cannot certify failure evidence"
    );
}

#[test]
fn alert_scenes_open_the_normal_rule_page_and_require_their_actual_content() {
    use std::time::Instant;
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

    let mut app = IcedApp::demo();
    apply_capture_target(&mut app, "active-alert");
    assert!(app.alerts_page_open());
    assert!(app.local_surface_kind().is_none());
    let lines = crate::app::alerts::active_alert_lines(&app);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].text.contains("94.0%"));

    seed_projection_fact(&mut app.shell, ProjectionSeedFact::ActiveAlerts(Vec::new()));
    for _ in 0..3 {
        let _ = app.update(Message::Frame(Instant::now()));
    }
    assert!(
        !app.capture.emitted,
        "an empty alert page cannot certify an active alert"
    );

    apply_capture_target(&mut app, "active-alert");
    let _ = app.update(Message::Frame(Instant::now()));
    assert!(!app.capture.emitted);
    let _ = app.update(Message::Frame(Instant::now()));
    assert!(app.capture.emitted);

    let mut rules = IcedApp::demo();
    apply_capture_target(&mut rules, "alert-rules-manager");
    assert!(rules.alerts_page_open());
    assert_eq!(crate::app::alerts::rule_rows(&rules).len(), 5);
}

#[test]
fn battery_scenes_ingest_both_real_chart_windows_before_the_presented_marker() {
    use std::time::Instant;
    for scenario in ["battery-fan-performance", "battery-live-performance"] {
        let mut app = IcedApp::demo();
        apply_capture_target(&mut app, scenario);
        let power = app
            .shell
            .projection()
            .power_supplies
            .as_ref()
            .expect("power facts");
        let battery = power.batteries.first().expect("battery");
        let charge = app.cached_battery_series(&battery.id);
        let watts = app.cached_battery_power_series(&battery.id);
        assert_eq!(charge.len(), 8);
        assert_eq!(charge.first().copied(), Some(85.0));
        assert_eq!(charge.last().copied(), Some(78.0));
        assert_eq!(watts.len(), 8);
        assert_eq!(watts.first().copied(), Some(12.0));
        assert!((watts.last().copied().expect("latest power") - 14.8).abs() < 0.01);
        let _ = app.update(Message::Frame(Instant::now()));
        assert!(!app.capture.emitted);
        let _ = app.update(Message::Frame(Instant::now()));
        assert!(app.capture.emitted);
    }
}

#[test]
fn smart_failure_scenes_open_the_normal_detail_and_do_not_accept_another_status() {
    use std::time::Instant;
    use taskmanager_core::core::device_state::DeviceStatus;
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
    use taskmanager_shell::presentation::effective_smart_status;

    for (scenario, status) in [
        ("smart-missing-tool", DeviceStatus::MissingTool),
        ("smart-permission", DeviceStatus::PermissionDenied),
    ] {
        let mut app = IcedApp::demo();
        apply_capture_target(&mut app, scenario);
        assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::DiskSmart));
        let disk = app
            .shell
            .projection()
            .snapshot
            .as_ref()
            .expect("snapshot")
            .disks
            .first()
            .expect("disk");
        assert_eq!(effective_smart_status(disk), status);
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::Snapshot(Box::new(
                IcedApp::demo().shell.projection().snapshot.clone(),
            )),
        );
        for _ in 0..3 {
            let _ = app.update(Message::Frame(Instant::now()));
        }
        assert!(
            !app.capture.emitted,
            "another SMART state cannot certify {scenario}"
        );
    }
}
