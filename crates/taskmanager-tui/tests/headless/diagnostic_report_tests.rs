use super::*;

#[test]
fn format_diagnostic_summary_includes_sections_and_redacts() {
    let hardware = taskmanager_core::core::hardware::HardwareInfo {
        os_name: Some("Linux".into()),
        os_version: Some("6.1.0".into()),
        kernel_version: Some("6.1.0-custom".into()),
        hostname: Some("host-alpha".into()),
        cpu_brand: Some("Test CPU @ 3.0GHz".into()),
        cpu_cores: Some(8),
        ..Default::default()
    };
    let snapshot = taskmanager_core::core::metrics::SystemSnapshot {
        timestamp_ms: 1_700_000_000_000,
        cpu: taskmanager_core::core::metrics::CpuMetrics::from_observations(Default::default()),
        memory: taskmanager_core::core::metrics::MemoryMetrics::from_observations(
            Default::default(),
            Default::default(),
        ),
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
    };

    let report = format_diagnostic_summary(
        Some(&hardware),
        Some(&snapshot),
        vec!["secret_admin".to_string()],
    );

    assert!(report.contains("TaskForest Terminal Diagnostic Report"));
    assert!(report.contains("Linux"));
    assert!(report.contains("Test CPU @ 3.0GHz"));
    assert!(report.contains("Uptime:"));
    assert!(report.contains("Processes: 120"));
    assert!(report.contains("Threads: 450"));
    assert!(report.contains("Redaction Summary"));
}

#[test]
fn export_diagnostic_report_writes_to_file_and_updates_feedback() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let mut app = crate::demo_app();
    let scratch = crate::ui::test_support::repo_temp_dir().join(format!(
        "diag-export-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    app.export_dir = Some(scratch.clone());

    let result = app.export_diagnostic_report();
    assert!(result.is_ok(), "export must succeed");
    let exported_path = result.unwrap();
    assert!(exported_path.exists(), "exported file must exist on disk");

    let content = std::fs::read_to_string(&exported_path).expect("read exported report");
    assert!(content.contains("TaskForest Terminal Diagnostic Report"));
    assert!(content.contains("Redaction Summary"));

    let feedback = app.feedback_notice().expect("footer notice must be set");
    assert_eq!(feedback.source(), FeedbackSource::Persistence);
    assert_eq!(feedback.severity(), FeedbackSeverity::Success);
    assert!(app.feedback_text().contains("taskforest-diagnostic.txt"));

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_to_custom_path_succeeds() {
    let mut app = crate::demo_app();
    let scratch = crate::ui::test_support::repo_temp_dir().join(format!(
        "diag-custom-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    let target = scratch.join("my-report.txt");

    let result = app.export_diagnostic_report_to(&target);
    assert!(result.is_ok());
    assert!(target.exists());

    let content = std::fs::read_to_string(&target).unwrap();
    assert!(content.contains("TaskForest Terminal Diagnostic Report"));

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn export_diagnostic_report_failure_reports_error_feedback() {
    let mut app = crate::demo_app();
    let invalid_path = PathBuf::from("/nonexistent-invalid-directory-9999/report.txt");
    let result = app.export_diagnostic_report_to(&invalid_path);
    assert!(result.is_err());
    let feedback = app.feedback_notice().expect("failure feedback notice");
    assert_eq!(feedback.severity(), FeedbackSeverity::Error);
}
