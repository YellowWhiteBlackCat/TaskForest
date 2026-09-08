use super::*;
use taskmanager_shell::demo_app;

#[test]
fn health_cpu_line_relabels_bogomips_instead_of_faking_mhz() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let mut bogomips_only = snapshot.clone();
    bogomips_only.cpu.frequency_source =
        taskmanager_core::core::metrics::CpuFrequencySource::BogoMips;

    let rows = health_rows(&bogomips_only);
    // A BogoMIPS-only host must read the BogoMIPS readout, never "MHz".
    let cpu = rows.iter().find(|row| row.label == "CPU").expect("CPU row");
    assert!(cpu.value.contains("BogoMIPS"));
    assert!(!cpu.value.contains("MHz"));

    // The untouched native fixture stays an MHz clock.
    let native = health_rows(snapshot);
    let native_cpu = native
        .iter()
        .find(|row| row.label == "CPU")
        .expect("native CPU row");
    assert!(native_cpu.value.contains("MHz"));
}

#[test]
fn health_rows_cover_every_domain_with_fixture_values() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let shell = demo_app();
    let snapshot = shell.projection().snapshot.as_ref().expect("demo snapshot");
    let rows = health_rows(snapshot);

    assert_eq!(rows.len(), 8);
    let row = |label: &str| rows.iter().find(|row| row.label == label).expect(label);
    assert!(row("CPU").healthy);
    assert!(row("CPU").value.contains("37.4%"));
    assert!(row("Memory").healthy);
    assert!(row("Memory").value.contains("GiB"));
    assert!(row("Networks").healthy);
    assert!(row("Networks").value.contains("wlan0"));
    assert!(row("GPU").healthy);
    assert!(row("GPU").value.contains("Intel Graphics (xe)"));
    assert!(row("System").healthy);
    assert!(row("System").value.contains("347 processes"));
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
