use super::*;
use taskmanager_core::{DeviceGeneration, DeviceId, StorageDeviceKey};

fn target() -> StorageDeviceTarget {
    StorageDeviceTarget {
        device_id: DeviceId::new("test"),
        device_generation: DeviceGeneration::INITIAL,
        locator: StorageDeviceKey::new("test"),
    }
}

#[test]
fn observation_and_control_never_degrade_to_unsupported() {
    // Both providers are implemented (route-C smartctl shell-out): each must
    // NEVER degrade to Unsupported, regardless of host state. On Linux CI
    // (no smartmontools) both return MissingDependency via Spawn(NotFound);
    // on a host with smartctl but an unopenable device they return Rejected;
    // when smartctl runs against a real device they return a real report.
    // None of these is Unsupported, so the host-independent contract (mirrors
    // the observation provider's pre-existing assertion) is assert_ne on
    // both. An exact MissingDependency equality would be host-dependent: it
    // fails on any machine with smartmontools installed.
    let mut observation = WinSmartSelfTestObservationProvider;
    assert_ne!(
        observation.refresh(&target(), DeviceState::healthy(1), 1),
        Err(ProviderFailure::Unsupported)
    );
    let mut control = WinSmartSelfTestControlProvider::new();
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("test"),
        device_generation: DeviceGeneration::INITIAL,
        device_key: StorageDeviceKey::new("test"),
        display_name: "test".into(),
        kind: taskmanager_core::SmartSelfTestKind::Short,
    };
    assert_ne!(control.start(&intent, 1), Err(ProviderFailure::Unsupported));
}

#[test]
fn smartctl_tokens_are_stable() {
    assert_eq!(smartctl_token(SmartSelfTestKind::Short), "short");
    assert_eq!(smartctl_token(SmartSelfTestKind::Extended), "long");
    assert_eq!(smartctl_token(SmartSelfTestKind::Conveyance), "conveyance");
}

#[test]
fn directory_usage_scans_a_real_fixture_tree() {
    // The wired provider is the shared pure-safe std::fs scanner: a real
    // bounded scan of a fixture tree must complete with real aggregates —
    // never a typed Unsupported or fabricated entries.
    let unique = format!(
        "taskmanager-win-dir-usage-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let root = crate::test_support::repo_temp_dir().join(unique);
    std::fs::create_dir_all(root.join("logs")).expect("fixture parent");
    std::fs::write(root.join("a.txt"), vec![0_u8; 100]).expect("fixture file");
    std::fs::write(root.join("logs/b.log"), vec![0_u8; 50]).expect("fixture file");

    let mut provider = WinDirectoryUsageProvider::new();
    let spec = DirectoryScanSpec {
        root: root.to_string_lossy().into_owned(),
        bounds: taskmanager_core::DirectoryScanBounds::default(),
    };
    let control = DirectoryScanControl::new(
        taskmanager_core::DirectoryScanId::new(1),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let mut latest = None;
    for _ in 0..10_000 {
        let snapshot = provider
            .scan_chunk(&spec, &control, 1)
            .expect("a real scan must not hard-fail");
        let terminal = snapshot.is_terminal();
        latest = Some(snapshot);
        if terminal {
            break;
        }
    }
    let snapshot = latest.expect("bounded fixture scan must terminate");
    assert_eq!(
        snapshot.status,
        taskmanager_core::DirectoryScanStatus::Completed
    );
    assert_eq!(snapshot.totals.files_counted, 2);
    assert_eq!(snapshot.totals.bytes_counted.current_value(), Some(&150));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn directory_usage_missing_root_is_a_typed_terminal_failure() {
    // A missing scan root must surface as a typed terminal failure inside
    // the snapshot, never an Unsupported provider error and never a
    // silent "empty tree".
    let missing = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-win-dir-usage-missing-{:?}",
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut provider = WinDirectoryUsageProvider::new();
    let spec = DirectoryScanSpec {
        root: missing.to_string_lossy().into_owned(),
        bounds: taskmanager_core::DirectoryScanBounds::default(),
    };
    let control = DirectoryScanControl::new(
        taskmanager_core::DirectoryScanId::new(2),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let snapshot = provider
        .scan_chunk(&spec, &control, 1)
        .expect("missing-root failure is typed into the snapshot");
    assert_eq!(
        snapshot.status,
        taskmanager_core::DirectoryScanStatus::Failed(FailureKind::TemporarilyUnavailable)
    );
    assert!(
        snapshot.entries[0].unreadable.is_some(),
        "the root report entry must carry the typed unreadable marker"
    );
}

#[test]
fn parser_maps_ata_completed_self_test_row() {
    let stdout = "SMART Self-test log structure revision number 1\n\
            Num  Test_Description  Status  Remaining  LifeTime(hours)  LBA_of_first_error\n\
            # 1  Short offline    Completed without error       00%      1234         -\n\
            # 2  Short offline    Completed: read failure        10%      1000    1234567\n";
    let parsed = parse_selftest_log(stdout);
    assert!(parsed.found);
    // The most recent entry (#1) is the first row: completed, no error.
    assert_eq!(parsed.phase, SmartSelfTestPhase::Completed);
    assert_eq!(parsed.progress_pct, None);
    assert_eq!(parsed.lifetime_hours, Some(1234));
    assert_eq!(parsed.first_error_lba, None);
}

#[test]
fn parser_maps_ata_self_test_with_read_failure_lba() {
    let stdout = "SMART Self-test log structure revision number 1\n\
            Num  Test_Description  Status  Remaining  LifeTime(hours)  LBA_of_first_error\n\
            # 1  Extended offline    Completed: read failure        10%      1000    1234567\n";
    let parsed = parse_selftest_log(stdout);
    assert_eq!(parsed.phase, SmartSelfTestPhase::Completed);
    assert_eq!(parsed.lifetime_hours, Some(1000));
    assert_eq!(parsed.first_error_lba, Some(1_234_567));
}

#[test]
fn parser_reports_running_phase_and_completion_percent_for_ata() {
    let stdout = "SMART Self-test log structure revision number 1\n\
            Num  Test_Description  Status  Remaining  LifeTime(hours)  LBA_of_first_error\n\
            # 1  Extended offline    Self-test routine in progress    90%       -         -\n";
    let parsed = parse_selftest_log(stdout);
    assert_eq!(parsed.phase, SmartSelfTestPhase::Running);
    // 90% remaining -> 10% complete.
    assert_eq!(parsed.progress_pct, Some(10.0));
    // An in-progress row has no completion lifetime / error LBA yet.
    assert_eq!(parsed.lifetime_hours, None);
    assert_eq!(parsed.first_error_lba, None);
}

#[test]
fn parser_maps_nvme_self_test_in_progress_block() {
    let stdout = "NVMe Self-test Log (Log Identifier 06)\n\
            Self-test status:                       Extended self-test in progress (90% remaining)\n\
            Current Self-test Operation:            Extended self-test\n\
            Self-test result[0]:\n\
              Status:                               Completed without error\n\
              Segment Number:                       0\n\
              Power on Hours:                       1234\n\
            Self-test result[1]:\n\
              Status:                               Aborted by host\n\
              Power on Hours:                       100\n";
    let parsed = parse_selftest_log(stdout);
    assert!(parsed.found);
    // The in-progress status line wins over the historical result block.
    assert_eq!(parsed.phase, SmartSelfTestPhase::Running);
    assert_eq!(parsed.progress_pct, Some(10.0));
    // result[0] is the most recent historical entry; result[1] is ignored.
    assert_eq!(parsed.lifetime_hours, Some(1234));
}

#[test]
fn parser_marks_absent_self_test_log_as_not_found() {
    // smartctl ran but the device exposes no self-test log at all.
    let stdout = "SMART overall-health self-assessment test result: PASSED\n";
    let parsed = parse_selftest_log(stdout);
    assert!(!parsed.found);
    assert_eq!(parsed.phase, SmartSelfTestPhase::Idle);
    assert_eq!(parsed.progress_pct, None);
    assert_eq!(parsed.lifetime_hours, None);
    assert_eq!(parsed.first_error_lba, None);
}

#[test]
fn parser_does_not_misread_nvme_idle_marker_as_running() {
    // "No self-test in progress" contains the negated substring
    // "in progress" -- it must NOT read as Running. The result[0] block
    // drives the phase instead.
    let stdout = "NVMe Self-test Log (Log Identifier 06)\n\
            Self-test status:                       No self-test in progress\n\
            Self-test result[0]:\n\
              Status:                               Completed without error\n\
              Power on Hours:                       5678\n";
    let parsed = parse_selftest_log(stdout);
    assert_eq!(parsed.phase, SmartSelfTestPhase::Completed);
    assert_eq!(parsed.progress_pct, None);
    assert_eq!(parsed.lifetime_hours, Some(5678));
}

#[test]
fn remaining_percent_extractor_clamps_and_ignores_garbage() {
    // Wrapped "(NN% remaining)" form (smartctl NVMe) must parse.
    assert_eq!(parse_remaining_percent("(90% remaining)"), Some(10.0));
    assert_eq!(
        parse_remaining_percent("Extended self-test in progress (90% remaining)"),
        Some(10.0)
    );
    assert_eq!(parse_remaining_percent("0% remaining"), Some(100.0));
    assert_eq!(parse_remaining_percent("100% remaining"), Some(0.0));
    assert_eq!(parse_remaining_percent("no number here"), None);
    assert_eq!(parse_remaining_percent("% remaining"), None);
}
