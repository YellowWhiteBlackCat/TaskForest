use super::*;

#[test]
fn test_process_to_tsv() {
    let proc = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(1234)
        .name("rustc".to_string())
        .current_cpu_percentage(45.2)
        .current_memory_bytes(1024 * 1024 * 50)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("testuser".to_string()),
                None,
                1,
            ),
        )
        .status("Running".to_string())
        .build();
    let tsv = process_to_tsv(&proc);
    assert!(tsv.starts_with("1234\trustc\t45.2%\t50.0 MiB\ttestuser\t"));
}

#[test]
fn test_process_to_json() {
    let proc = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(5678)
        .name("cargo \"builder\"".to_string())
        .current_cpu_percentage(12.0)
        .current_memory_bytes(1024 * 1024 * 10)
        .metadata_observations(
            taskmanager_core::core::process::ProcessMetadataObservations::current(
                taskmanager_core::core::process::ProcessOwner::opaque("admin".to_string()),
                None,
                1,
            ),
        )
        .status("Sleeping".to_string())
        .cmdline("cargo build --release".to_string())
        .build();
    let json = process_to_json(&proc);
    assert!(json.contains("\"pid\": 5678"));
    assert!(json.contains("\"cargo \\\"builder\\\"\""));
    assert!(json.contains("\"cmdline\": \"cargo build --release\""));
}

#[test]
fn test_redact_sensitive_text() {
    let raw = "Error at /home/<user>/project/src/main.rs and C:\\Users\\<user>\\task.exe";
    let redacted = redact_sensitive_text(raw);
    assert!(!redacted.contains("secretuser"));
    assert!(!redacted.contains("alice"));
    assert!(redacted.contains("<user>"));
}
