use super::*;
use taskmanager_shell::demo_app;

#[test]
fn health_cpu_line_relabels_bogomips_instead_of_faking_mhz() {
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let mut bogomips_only = snapshot.clone();
    bogomips_only.cpu.frequency_source = taskmanager_application::CpuFrequencySource::BogoMips;

    let rows = health_rows(&bogomips_only);
    // A BogoMIPS-only host must read the BogoMIPS readout, never "MHz".
    assert!(rows[0].value.contains("BogoMIPS"));
    assert!(!rows[0].value.contains("MHz"));

    // The untouched native fixture stays an MHz clock.
    let native = health_rows(snapshot);
    assert!(native[0].value.contains("MHz"));
}

#[test]
fn health_rows_cover_every_domain_with_fixture_values() {
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let rows = health_rows(snapshot);

    assert_eq!(rows.len(), 7);
    assert!(rows[0].label == "CPU" && rows[0].healthy);
    assert!(rows[0].value.contains("37.4%"));
    assert!(rows[1].label == "Memory" && rows[1].healthy);
    assert!(rows[1].value.contains("GiB"));
    assert!(rows[4].label == "Networks" && rows[4].healthy);
    assert!(rows[4].value.contains("wlan0"));
    assert!(rows[5].label == "GPU" && rows[5].healthy);
    assert!(rows[5].value.contains("Intel Graphics (xe)"));
    assert!(rows[6].label == "System" && rows[6].healthy);
    assert!(rows[6].value.contains("347 processes"));
}

#[test]
fn health_rows_stay_honest_when_domains_are_absent() {
    let snapshot = SystemSnapshot::default();
    let rows = health_rows(&snapshot);
    assert_eq!(rows.len(), 7);
    for row in &rows {
        assert!(!row.healthy, "{} must not claim health", row.label);
    }
    assert!(rows[0].value.contains("—"));
    assert!(rows[1].value == "—");
}

#[test]
fn health_modal_renders_with_and_without_telemetry() {
    let app = crate::IcedApp::demo();
    let _view = render(&app);
    drop(_view);

    let mut app = crate::IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    let _view = render(&app);
}
