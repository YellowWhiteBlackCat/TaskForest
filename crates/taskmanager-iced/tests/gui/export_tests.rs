use super::*;
use taskmanager_core::core::process::ProcessMetadataObservations;
use taskmanager_core::core::process::ProcessOwner;
use taskmanager_test_support::ProcessItemFixtureBuilder;

#[test]
fn test_process_to_tsv() {
    let proc = ProcessItemFixtureBuilder::new()
        .pid(1234)
        .name("rustc".to_string())
        .current_cpu_percentage(45.2)
        .current_memory_bytes(1024 * 1024 * 50)
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque("testuser".to_string()),
            None,
            1,
        ))
        .status("Running".to_string())
        .build();
    let tsv = process_to_tsv(&proc);
    assert!(tsv.starts_with("1234\trustc\t45.2%\t50.0 MiB\ttestuser\t"));
}

#[test]
fn test_process_to_json() {
    let proc = ProcessItemFixtureBuilder::new()
        .pid(5678)
        .name("cargo \"builder\"".to_string())
        .current_cpu_percentage(12.0)
        .current_memory_bytes(1024 * 1024 * 10)
        .metadata_observations(ProcessMetadataObservations::current(
            ProcessOwner::opaque("admin".to_string()),
            None,
            1,
        ))
        .status("Sleeping".to_string())
        .cmdline("cargo build --release".to_string())
        .build();
    let json = process_to_json(&proc);
    assert!(json.contains("\"pid\": 5678"));
    assert!(json.contains("\"cargo \\\"builder\\\"\""));
    assert!(json.contains("\"cmdline\": \"cargo build --release\""));
}

/// A command line carrying newlines, tabs and control characters must still
/// produce parseable JSON: serde owns the escaping, not a hand-rolled rule.
#[test]
fn process_json_escapes_control_characters() {
    let proc = ProcessItemFixtureBuilder::new()
        .pid(9)
        .name("tricky".to_string())
        .cmdline("a\nb\tc\rd\\e \"f\"\u{7}g".to_string())
        .build();
    let json = process_to_json(&proc);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        parsed["cmdline"],
        serde_json::json!("a\nb\tc\rd\\e \"f\"\u{7}g")
    );
}
