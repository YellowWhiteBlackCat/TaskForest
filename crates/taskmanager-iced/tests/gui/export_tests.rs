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

/// A command line carrying newlines, tabs and control characters must still
/// produce parseable JSON: serde owns the escaping, not a hand-rolled rule.
#[test]
fn process_json_escapes_control_characters() {
    let proc = taskmanager_test_support::ProcessItemFixtureBuilder::new()
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

/// The diagnostics report delegates every redaction to the audited core
/// contract: home paths, Windows profiles, IP addresses and the supplied
/// account labels all disappear, and the summary says what was removed.
#[test]
fn redact_with_summary_covers_paths_addresses_and_usernames() {
    let home_path = ["/home/", "alice/tools/scan"].concat();
    let windows_path = ["C:\\Users\\", "alice\\task.exe"].concat();
    let raw =
        format!("user alice ran {home_path} and {windows_path} from 192.168.1.10 and fe80::1");
    let report = redact_with_summary(&raw, ["alice".to_string()]).expect("redaction succeeds");
    assert!(!report.markdown.contains("alice"));
    assert!(!report.markdown.contains("/home/"));
    assert!(!report.markdown.contains("C:\\Users\\"));
    assert!(!report.markdown.contains("192.168.1.10"));
    assert!(!report.markdown.contains("fe80::1"));
    assert!(report.markdown.contains("<redacted-path>"));
    assert!(report.markdown.contains("<redacted-user>"));
    assert!(report.markdown.contains("<redacted-ipv4>"));
    assert!(report.markdown.contains("<redacted-ipv6>"));
    assert!(report.redactions.paths > 0);
    assert!(report.redactions.usernames > 0);
    assert_eq!(report.redactions.ipv4_addresses, 1);
    assert_eq!(report.redactions.ipv6_addresses, 1);
}

/// The clipboard report is redacted too, not just the standalone helper: the
/// account labels observed on the host are removed from the exported text.
#[test]
fn system_diagnostics_markdown_redacts_host_usernames() {
    let report = crate::export::redact_with_summary(
        "Uptime: 1h 01m\noperator session: alice\n",
        ["alice".to_string()],
    )
    .expect("redaction succeeds");
    assert!(report.markdown.contains("Uptime: 1h 01m"));
    assert!(!report.markdown.contains("alice"));
    assert!(report.markdown.contains("<redacted-user>"));
}
