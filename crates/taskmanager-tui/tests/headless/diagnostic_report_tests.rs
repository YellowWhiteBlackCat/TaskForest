use super::*;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::metrics::{CpuMetrics, MemoryMetrics};
use taskmanager_core::core::process::{ProcessMetadataObservations, ProcessOwner};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_test_support::ProcessItemFixtureBuilder;

fn hardware_fixture() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("Linux".into()),
        os_version: Some("6.1.0".into()),
        kernel_version: Some("6.1.0-custom".into()),
        hostname: Some("host-alpha".into()),
        cpu_brand: Some("Test CPU @ 3.0GHz".into()),
        cpu_cores: Some(8),
        ..Default::default()
    }
}

fn snapshot_fixture() -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms: 1_700_000_000_000,
        cpu: CpuMetrics::from_observations(Default::default()),
        memory: MemoryMetrics::from_observations(Default::default(), Default::default()),
        disks: Vec::new(),
        networks: Vec::new(),
        gpu: Vec::new(),
        telemetry_sources: Vec::new(),
        provider_states: Vec::new(),
        device_lifecycles: Default::default(),
        uptime_secs: 3600,
        processes: 120,
        threads: Some(450),
        pressure: None,
        load_average: None,
    }
}

fn process_with_user(pid: u32, name: &str, user: &str) -> ProcessItem {
    ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .current_cpu_percentage(12.5)
        .current_memory_bytes(100 * 1024 * 1024)
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque(user),
            None,
            42,
        ))
        .build()
}

fn scratch_dir(label: &str) -> PathBuf {
    let scratch = crate::ui::test_support::repo_temp_dir().join(format!(
        "{label}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    scratch
}

#[test]
fn render_diagnostic_report_includes_sections_and_redacts() {
    let hardware = hardware_fixture();
    let snapshot = snapshot_fixture();
    let processes = vec![process_with_user(4242, "sample", "secret_admin")];

    let report = render_diagnostic_report(
        Some(&hardware),
        Some(&snapshot),
        None,
        None,
        &processes,
        ["secret_admin".to_string()],
    )
    .expect("shared engine renders the report")
    .expect("observed facts produce a report");

    assert!(report.contains("# TaskForest Diagnostic Report"));
    assert!(report.contains("Linux"));
    assert!(report.contains("Test CPU @ 3.0GHz"));
    assert!(report.contains("Uptime"));
    assert!(report.contains("Total Processes: 1"));
    assert!(
        !report.contains("secret_admin"),
        "the observed username must never survive redaction:\n{report}"
    );
    assert!(
        report.contains("<redacted-user>"),
        "redaction must leave its audited placeholder:\n{report}"
    );
}

#[test]
fn render_diagnostic_report_without_facts_is_honest_none() {
    let report =
        render_diagnostic_report(None, None, None, None, &[], std::iter::empty::<String>())
            .expect("empty fact set is not an error");
    assert!(
        report.is_none(),
        "no observed fact must answer with None, never an empty success report"
    );
}

#[test]
fn export_diagnostic_report_writes_to_file_and_updates_feedback() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);

    let mut app = crate::demo_app();
    let scratch = scratch_dir("diag-export");
    app.export_dir = Some(scratch.clone());

    let result = app.export_diagnostic_report();
    assert!(result.is_ok(), "export must succeed");
    let exported_path = result.unwrap();
    assert!(exported_path.exists(), "exported file must exist on disk");

    let content = std::fs::read_to_string(&exported_path).expect("read exported report");
    assert!(content.contains("# TaskForest Diagnostic Report"));

    let feedback = app.feedback_notice().expect("footer notice must be set");
    assert_eq!(feedback.source(), FeedbackSource::Persistence);
    assert_eq!(feedback.severity(), FeedbackSeverity::Success);
    assert!(app.feedback_text().contains("taskforest-diagnostic.txt"));

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_to_custom_path_succeeds() {
    let mut app = crate::demo_app();
    let scratch = scratch_dir("diag-custom");
    let target = scratch.join("my-report.txt");

    let result = app.export_diagnostic_report_to(&target);
    assert!(result.is_ok());
    assert!(target.exists());

    let content = std::fs::read_to_string(&target).unwrap();
    assert!(content.contains("# TaskForest Diagnostic Report"));

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_redacts_process_usernames_on_disk() {
    let mut app = crate::TuiApp::new();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Processes(Some(vec![process_with_user(
            4242,
            "sample",
            "secret_admin",
        )])),
    );
    let scratch = scratch_dir("diag-redaction");
    let target = scratch.join("report.txt");

    let result = app.export_diagnostic_report_to(&target);
    assert!(
        result.is_ok(),
        "export must succeed with observed processes"
    );

    let content = std::fs::read_to_string(&target).expect("read exported report");
    assert!(
        !content.contains("secret_admin"),
        "the on-disk report must not leak the observed username:\n{content}"
    );
    assert!(content.contains("<redacted-user>"));

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_without_data_warns_and_writes_nothing() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);

    let mut app = crate::TuiApp::new();
    let scratch = scratch_dir("diag-empty");
    let target = scratch.join("empty-report.txt");

    let result = app.export_diagnostic_report_to(&target);
    assert!(
        matches!(result, Err(DiagnosticExportError::NoData)),
        "an empty fact set must be a typed no-data failure"
    );
    assert!(
        !target.exists(),
        "no data must never write a success-looking report"
    );

    let feedback = app.feedback_notice().expect("warning notice must be set");
    assert_eq!(feedback.source(), FeedbackSource::Persistence);
    assert_eq!(feedback.severity(), FeedbackSeverity::Warning);

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_failure_reports_error_feedback() {
    let mut app = crate::demo_app();
    let invalid_path = PathBuf::from("/nonexistent-invalid-directory-9999/report.txt");
    let result = app.export_diagnostic_report_to(&invalid_path);
    assert!(result.is_err());
    assert!(matches!(result, Err(DiagnosticExportError::Io(_))));
    let feedback = app.feedback_notice().expect("failure feedback notice");
    assert_eq!(feedback.severity(), FeedbackSeverity::Error);
}
