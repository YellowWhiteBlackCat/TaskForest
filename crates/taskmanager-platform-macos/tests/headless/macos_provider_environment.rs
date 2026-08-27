use super::*;

#[test]
fn mac_environment_providers_degrade_honestly_off_macos() {
    // `sysctl -n kern.boottime` is macOS-specific: on Linux CI procps
    // `sysctl` runs but exits non-zero for the BSD OID, so boot evidence
    // degrades Ok with a MissingTool marker on both evidence channels,
    // empty failed-units + critical-chain lists, and a healthy device
    // state. Never Err, never fabricated failed-units or critical-chain.
    let mut evidence = MacStartupEvidenceProvider;
    let snapshot = evidence
        .observe(1)
        .expect("boot evidence degrades to Ok snapshot");
    assert_eq!(snapshot.state, DeviceState::healthy(1));
    assert_eq!(snapshot.failed_units_state, DeviceState::healthy(1));
    assert_eq!(snapshot.critical_chain_state, DeviceState::healthy(1));
    assert_eq!(
        snapshot.failed_units_failure,
        Some(StartupEvidenceFailure::MissingTool)
    );
    assert_eq!(
        snapshot.critical_chain_failure,
        Some(StartupEvidenceFailure::MissingTool)
    );
    assert!(snapshot.failed_units.is_empty());
    assert!(snapshot.critical_chain.is_empty());
    // Session control (disconnect/lock) still has no safe CLI route on
    // macOS, so it completes with a typed Unsupported (stays pending).
    let mut session_control = PendingSessionControlProvider;
    assert_eq!(
        session_control.control(&SessionId::new("1"), SessionControlAction::Lock),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn boottime_marker_parser_detects_boottime_struct() {
    assert!(parse_boottime_marker(
        "{ sec = 1692824565, usec = 0 } Sat Aug 26 14:42:45 2023\n"
    ));
    assert!(parse_boottime_marker(
        "noise\n{ sec = 1692824565, usec = 123456 } trailing\n"
    ));
    // Leading/trailing whitespace around the struct line is tolerated.
    assert!(parse_boottime_marker("   { sec = 1, usec = 2 } \n"));
}

#[test]
fn boottime_marker_parser_rejects_absent_or_partial() {
    // Linux procps `sysctl -n kern.boottime` writes the error to stderr;
    // stdout is empty (or, without -n, a single error line) — never matches.
    assert!(!parse_boottime_marker(""));
    assert!(!parse_boottime_marker(
        "sysctl: cannot stat /proc/sys/kern/boottime: No such file or directory\n"
    ));
    assert!(!parse_boottime_marker("kern.boottime: unknown\n"));
    assert!(!parse_boottime_marker("random stdout\n"));
    // Partial struct fragments must not match — both fields are required.
    assert!(!parse_boottime_marker("sec = 1692824565\n"));
    assert!(!parse_boottime_marker("usec = 0\n"));
}

#[test]
fn launch_dir_paths_expand_home_only_when_present() {
    assert!(expand_home("~/Library/LaunchAgents").is_absolute());
    assert_eq!(
        expand_home("/Library/LaunchDaemons"),
        PathBuf::from("/Library/LaunchDaemons")
    );
}

#[test]
fn malformed_plist_parses_to_none() {
    let path = crate::test_support::repo_temp_dir().join("tm-broken-unsigned.plist");
    std::fs::write(&path, b"not a plist").expect("write");
    assert!(parse_launch_plist(&path).is_none());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn valid_launch_plist_parses_fields() {
    let path = crate::test_support::repo_temp_dir().join("tm-test-launch.plist");
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.test.agent</string>
  <key>ProgramArguments</key><array><string>/usr/bin/true</string><string>--flag</string></array>
  <key>Disabled</key><true/>
  <key>RunAtLoad</key><true/>
</dict></plist>"#;
    std::fs::write(&path, xml).expect("write");
    let parsed = parse_launch_plist(&path).expect("parses");
    assert_eq!(parsed.label, "com.test.agent");
    assert_eq!(parsed.program_arguments, vec!["/usr/bin/true", "--flag"]);
    assert!(parsed.disabled);
    let _ = std::fs::remove_file(&path);
}
