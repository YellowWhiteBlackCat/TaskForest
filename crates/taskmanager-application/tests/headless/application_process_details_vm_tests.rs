use super::*;
use std::path::PathBuf;
use taskmanager_core::DeviceState;
use taskmanager_core::ProcessSchedulingPolicy;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    ProcessItem, ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner,
    ProcessOwnerIdentity, ProcessScalarObservations,
};
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_core::core::units::UnitPreferences;

fn process_details_rows(item: &ProcessItem, units: &UnitPreferences) -> Vec<ProcessDetailsRowVm> {
    super::process_details_rows_with_local_time(
        item,
        units,
        &LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0),
    )
}

/// A fully observed row: every scalar carries a current typed
/// observation, so every foldable field has a Text value.
fn fully_observed_item() -> ProcessItem {
    let mut item = ProcessItem::new(4242, "sample");
    item.parent_pid = Some(1);
    item.cmdline = "sample --flag value".to_owned();
    item.status = "S".to_owned();
    item.scheduling_policy = Some(ProcessSchedulingPolicy::Other);
    item.oom_score = Some(250);
    item.minor_page_faults = Some(15000);
    item.major_page_faults = Some(3);
    item.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Opaque("root".into()),
                label: None,
            },
            42,
        ),
        executable_path: ProcessMetadataObservation::available(
            PathBuf::from("/usr/bin/sample"),
            42,
        ),
    });
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(600, 42),
        cpu_percentage: ScalarObservation::available(12.5, 42),
        memory_bytes: ScalarObservation::available(100 * 1024 * 1024, 42),
        memory_pss_bytes: ScalarObservation::available(50 * 1024 * 1024, 42),
        swap_bytes: ScalarObservation::available(2 * 1024 * 1024, 42),
        disk_read_bytes_total: ScalarObservation::available(10 * 1024 * 1024, 42),
        disk_write_bytes_total: ScalarObservation::available(20 * 1024 * 1024, 42),
        disk_read_bytes_per_sec: ScalarObservation::available(1536, 42),
        disk_write_bytes_per_sec: ScalarObservation::available(1024 * 1024, 42),
        network_rx_bytes_per_sec: ScalarObservation::available(1000, 42),
        network_tx_bytes_per_sec: ScalarObservation::available(2000, 42),
        memory_uss_bytes: ScalarObservation::available(40 * 1024 * 1024, 42),
        memory_anon_huge_pages_bytes: ScalarObservation::available(8 * 1024 * 1024, 42),
        threads: ScalarObservation::available(8, 42),
        start_time_secs: ScalarObservation::available(1_600_000_000, 42),
        cpu_time_secs: ScalarObservation::available(3_690, 42),
        fds: ScalarObservation::available(42, 42),
        nice: ScalarObservation::available(10, 42),
    });
    item
}

fn value(rows: &[ProcessDetailsRowVm], field: ProcessDetailsField) -> &DetailValue {
    detail_value(rows, field)
}

/// Every field folds to its adjudicated display string under the
/// default (Mission-Center-parity, base-2 bytes) preferences.
#[test]
fn fully_observed_row_folds_every_field_to_text() {
    let rows = process_details_rows(&fully_observed_item(), &UnitPreferences::default());
    assert_eq!(
        value(&rows, ProcessDetailsField::Name),
        &DetailValue::Text("sample".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Pid),
        &DetailValue::Text("4242".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::ParentPid),
        &DetailValue::Text("1".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::User),
        &DetailValue::Text("root".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Status),
        &DetailValue::Text("S".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Cpu),
        &DetailValue::Text("12.5%".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Memory),
        &DetailValue::Text("100.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Pss),
        &DetailValue::Text("50.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Uss),
        &DetailValue::Text("40.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Shared),
        &DetailValue::Text("60.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::AnonHugePages),
        &DetailValue::Text("8.0 MiB (8.0% RSS)".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Swap),
        &DetailValue::Text("2.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Threads),
        &DetailValue::Text("8".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Fds),
        &DetailValue::Text("42".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Nice),
        &DetailValue::Text("+10".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::SchedPolicy),
        &DetailValue::Text("Normal (SCHED_OTHER)".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::OomScore),
        &DetailValue::Text("250".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::PageFaults),
        &DetailValue::Text("15000 (I/O: 3)".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::StartTime),
        &DetailValue::Text("2020-09-13 12:26:40".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::CpuTime),
        &DetailValue::Text("01h 01m".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::DiskReadRate),
        &DetailValue::Text("1.5 KiB/s".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::DiskWriteRate),
        &DetailValue::Text("1.0 MiB/s".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::NetworkRate),
        &DetailValue::Text("2.9 KiB/s".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::DiskReadTotal),
        &DetailValue::Text("10.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::DiskWriteTotal),
        &DetailValue::Text("20.0 MiB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Exe),
        &DetailValue::Text("/usr/bin/sample".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Cmdline),
        &DetailValue::Text("sample --flag value".to_owned())
    );
}

/// The derived-shared facet is the core authority's `RSS - USS` difference:
/// it needs both observations, and a missing USS keeps the honest dash
/// instead of reading as a fabricated `0 B`.
#[test]
fn shared_facet_requires_both_observations_and_never_fabricates_a_zero() {
    let mut item = fully_observed_item();
    let observed = *item.scalar_observations();
    assert_eq!(
        value(
            &process_details_rows(&item, &UnitPreferences::default()),
            ProcessDetailsField::Shared
        ),
        &DetailValue::Text("60.0 MiB".to_owned())
    );

    item.apply_scalar_observations(ProcessScalarObservations {
        memory_uss_bytes: ScalarObservation::default(),
        ..observed
    });
    let cold = process_details_rows(&item, &UnitPreferences::default());
    assert!(
        value(&cold, ProcessDetailsField::Shared).is_missing(),
        "an unobserved private facet makes the derived share missing, never 0"
    );

    item.apply_scalar_observations(ProcessScalarObservations {
        memory_uss_bytes: ScalarObservation::available(100 * 1024 * 1024, 42),
        ..observed
    });
    let fully_private = process_details_rows(&item, &UnitPreferences::default());
    assert_eq!(
        value(&fully_private, ProcessDetailsField::Shared),
        &DetailValue::Text("0 B".to_owned()),
        "a measured RSS == USS is a real zero, not an absence"
    );
}

/// An empty row has no current observations: every foldable field is
/// Missing — never a fabricated zero. `Name`/`Pid`/`Status` are identity
/// strings with no missing state, so they stay Text.
#[test]
fn empty_item_folds_every_observation_to_missing() {
    let rows = process_details_rows(&ProcessItem::default(), &UnitPreferences::default());
    let foldable = ProcessDetailsField::ALL.iter().copied().filter(|field| {
        !matches!(
            field,
            ProcessDetailsField::Name | ProcessDetailsField::Pid | ProcessDetailsField::Status
        )
    });
    for field in foldable {
        assert!(
            value(&rows, field).is_missing(),
            "{field:?} must fold to Missing on an empty item"
        );
    }
    assert_eq!(
        value(&rows, ProcessDetailsField::Pid),
        &DetailValue::Text("0".to_owned())
    );
}

/// Unit preferences flow through the fold: base-10 memory preferences
/// re-spell every memory-family field on the decimal ladder.
#[test]
fn unit_preferences_respell_byte_fields() {
    let prefs = UnitPreferences {
        memory_use_base2: false,
        ..UnitPreferences::default()
    };
    let rows = process_details_rows(&fully_observed_item(), &prefs);
    assert_eq!(
        value(&rows, ProcessDetailsField::Memory),
        &DetailValue::Text("104.9 MB".to_owned())
    );
    assert_eq!(
        value(&rows, ProcessDetailsField::Swap),
        &DetailValue::Text("2.1 MB".to_owned())
    );
    // The drive family keeps its own (default base-2) preferences.
    assert_eq!(
        value(&rows, ProcessDetailsField::DiskReadRate),
        &DetailValue::Text("1.5 KiB/s".to_owned())
    );
}

/// A whitespace-only command line is missing (the dash-on-empty
/// semantics the TUI and Iced already rendered); a real one passes
/// through verbatim.
#[test]
fn cmdline_empty_and_whitespace_fold_to_missing() {
    let mut item = fully_observed_item();
    item.cmdline = "   ".to_owned();
    let rows = process_details_rows(&item, &UnitPreferences::default());
    assert!(value(&rows, ProcessDetailsField::Cmdline).is_missing());

    item.cmdline = "\tsleep 60\n".to_owned();
    let rows = process_details_rows(&item, &UnitPreferences::default());
    assert_eq!(
        value(&rows, ProcessDetailsField::Cmdline),
        &DetailValue::Text("\tsleep 60\n".to_owned())
    );
}

/// The zero epoch is the documented unknown-start sentinel: Missing,
/// never a formatted 1970 date. A zero-valued nice renders `0` (a
/// measured zero is not a missing value).
#[test]
fn zero_start_time_is_missing_and_zero_nice_is_a_measured_zero() {
    let mut item = fully_observed_item();
    item.apply_scalar_observations(ProcessScalarObservations {
        start_time_secs: ScalarObservation::available(0, 42),
        nice: ScalarObservation::available(0, 42),
        ..*item.scalar_observations()
    });
    let rows = process_details_rows(&item, &UnitPreferences::default());
    assert!(value(&rows, ProcessDetailsField::StartTime).is_missing());
    assert_eq!(
        value(&rows, ProcessDetailsField::Nice),
        &DetailValue::Text("0".to_owned())
    );
}

/// The row order is the stable ALL order — the sequence every end's
/// parity test pins against, and the ids are the unique vocabulary.
#[test]
fn row_order_matches_all_and_ids_are_unique() {
    let rows = process_details_rows(&fully_observed_item(), &UnitPreferences::default());
    assert_eq!(rows.len(), ProcessDetailsField::ALL.len());
    let fields: Vec<ProcessDetailsField> = rows.iter().map(|row| row.field).collect();
    assert_eq!(fields, ProcessDetailsField::ALL.to_vec());

    let ids: Vec<&str> = ProcessDetailsField::ALL.iter().map(|f| f.id()).collect();
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "field ids must be unique");
}

/// `detail_value` on an absent field returns the shared Missing
/// sentinel; the text/dash accessors cover both variants.
#[test]
fn lookup_helpers_fail_closed() {
    let rows = process_details_rows(&fully_observed_item(), &UnitPreferences::default());
    assert!(detail_value(&[], ProcessDetailsField::Pid).is_missing());
    assert_eq!(value(&rows, ProcessDetailsField::Pid).text_or("—"), "4242");
    assert_eq!(
        value(&rows, ProcessDetailsField::Swap).as_str(),
        Some("2.0 MiB")
    );
    let empty = process_details_rows(&ProcessItem::default(), &UnitPreferences::default());
    assert_eq!(
        detail_value(&empty, ProcessDetailsField::Memory).text_or("—"),
        "—"
    );
    assert!(
        detail_value(&empty, ProcessDetailsField::Memory)
            .as_str()
            .is_none()
    );
}

/// The duration spelling pins the shell contract on both sides of a day.
#[test]
fn duration_pins_the_shell_contract() {
    let rows = |secs: u64| {
        let mut item = fully_observed_item();
        item.apply_scalar_observations(ProcessScalarObservations {
            cpu_time_secs: ScalarObservation::available(secs, 42),
            ..*item.scalar_observations()
        });
        process_details_rows(&item, &UnitPreferences::default())
    };
    fn cpu_time(rows: &[ProcessDetailsRowVm]) -> Option<&str> {
        detail_value(rows, ProcessDetailsField::CpuTime).as_str()
    }
    assert_eq!(cpu_time(&rows(90)), Some("00h 01m"));
    assert_eq!(cpu_time(&rows(3_690)), Some("01h 01m"));
    assert_eq!(cpu_time(&rows(86_400 + 3_600)), Some("1d 01h 00m"));
}

/// The start-time datetime pins the authoritative `date -u` outputs
/// moved from the GPUI `root/chrome.rs` test matrix: epoch base, day
/// rollover, century leap day, leap years, non-leap centuries, and the
/// 23:59:59 time-of-day ceiling.
#[test]
fn start_time_pins_the_explicit_fixed_utc_fixture_matrix() {
    let start = |secs: u64| {
        let mut item = fully_observed_item();
        item.apply_scalar_observations(ProcessScalarObservations {
            start_time_secs: ScalarObservation::available(secs, 42),
            ..*item.scalar_observations()
        });
        process_details_rows(&item, &UnitPreferences::default())
    };
    let start_text = |secs: u64| {
        detail_value(&start(secs), ProcessDetailsField::StartTime)
            .as_str()
            .map(str::to_owned)
    };
    assert_eq!(start_text(1), Some("1970-01-01 00:00:01".to_owned()));
    assert_eq!(start_text(86_400), Some("1970-01-02 00:00:00".to_owned()));
    assert_eq!(
        start_text(946_684_800),
        Some("2000-01-01 00:00:00".to_owned())
    );
    assert_eq!(
        start_text(951_825_600),
        Some("2000-02-29 12:00:00".to_owned())
    );
    assert_eq!(
        start_text(1_677_628_800),
        Some("2023-03-01 00:00:00".to_owned())
    );
    assert_eq!(
        start_text(1_709_164_800),
        Some("2024-02-29 00:00:00".to_owned())
    );
    assert_eq!(
        start_text(1_709_251_199),
        Some("2024-02-29 23:59:59".to_owned())
    );
    assert_eq!(
        start_text(1_709_251_200),
        Some("2024-03-01 00:00:00".to_owned())
    );
    assert_eq!(
        start_text(4_102_444_800),
        Some("2100-01-01 00:00:00".to_owned())
    );
}

/// Nice pins the signed spelling on all three sign branches.
#[test]
fn nice_pins_the_signed_spelling() {
    let nice = |value: i32| {
        let mut item = fully_observed_item();
        item.apply_scalar_observations(ProcessScalarObservations {
            nice: ScalarObservation::available(value, 42),
            ..*item.scalar_observations()
        });
        process_details_rows(&item, &UnitPreferences::default())
    };
    for (input, expected) in [(10, "+10"), (0, "0"), (-5, "-5")] {
        assert_eq!(
            detail_value(&nice(input), ProcessDetailsField::Nice).as_str(),
            Some(expected)
        );
    }
}

/// Sensitive environment variable keys containing TOKEN, KEY, SECRET, PASSWORD, or AUTH
/// are detected case-insensitively, while benign keys pass through unflagged.
#[test]
fn sensitive_env_key_detection() {
    let sensitive_keys = [
        "API_KEY",
        "PRIVATE_KEY",
        "SECRET_KEY",
        "apiKey",
        "key",
        "GITHUB_TOKEN",
        "OAUTH_TOKEN",
        "authToken",
        "access_token",
        "token",
        "CLIENT_SECRET",
        "JWT_SECRET",
        "appSecret",
        "secret",
        "DB_PASSWORD",
        "PASSWORD",
        "user_password",
        "passWord",
        "AUTH_HEADER",
        "AUTHORIZATION",
        "HTTP_AUTH",
        "SSH_AUTH_SOCK",
        "auth",
        "AWS_SECRET_ACCESS_KEY",
    ];
    for key in sensitive_keys {
        assert!(
            is_sensitive_env_key(key),
            "key '{key}' must be detected as sensitive"
        );
        assert!(
            is_sensitive_environment_key(key),
            "key '{key}' must be detected as sensitive via alias"
        );
    }

    let benign_keys = [
        "PATH",
        "HOME",
        "USER",
        "SHELL",
        "TERM",
        "LANG",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "RUST_LOG",
        "PWD",
        "EDITOR",
    ];
    for key in benign_keys {
        assert!(
            !is_sensitive_env_key(key),
            "key '{key}' must NOT be detected as sensitive"
        );
    }
}

/// Environment variable rendering masks sensitive values with asterisks while
/// preserving benign values intact.
#[test]
fn environment_value_masking_with_asterisks() {
    assert!(SENSITIVE_VALUE_MASK.chars().all(|c| c == '*'));
    assert_eq!(SENSITIVE_VALUE_MASK, "********");

    // Sensitive keys have values replaced with asterisks
    assert_eq!(
        render_environment_value("API_KEY", "super_secret_value_123"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(
        render_environment_value("GITHUB_TOKEN", "ghp_xxxxxxxxxxxx"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(
        render_environment_value("CLIENT_SECRET", "xyz789"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(
        render_environment_value("DB_PASSWORD", "p@ssword!"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(
        render_environment_value("AUTH_TOKEN", "bearer_abc"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(render_environment_value("SECRET", ""), SENSITIVE_VALUE_MASK);

    // Benign keys preserve value
    assert_eq!(
        render_environment_value("PATH", "/usr/bin:/bin"),
        "/usr/bin:/bin"
    );
    assert_eq!(render_environment_value("USER", "alice"), "alice");
    assert_eq!(
        render_environment_value("LANG", "en_US.UTF-8"),
        "en_US.UTF-8"
    );

    // Mask helper aliases
    assert_eq!(
        mask_environment_value("API_KEY", "secret"),
        SENSITIVE_VALUE_MASK
    );
    assert_eq!(
        mask_sensitive_env_value("PASSWORD", "pass"),
        SENSITIVE_VALUE_MASK
    );
}

/// Key=value rendering and entry-level conversions mask sensitive variables with asterisks.
#[test]
fn environment_variable_and_entry_rendering() {
    assert_eq!(
        render_environment_variable("API_KEY", "my_secret_token"),
        format!("API_KEY={SENSITIVE_VALUE_MASK}")
    );
    assert_eq!(
        render_environment_variable("PATH", "/usr/local/bin"),
        "PATH=/usr/local/bin"
    );
    assert_eq!(
        format_environment_entry("AUTH_TOKEN", "token_val"),
        format!("AUTH_TOKEN={SENSITIVE_VALUE_MASK}")
    );

    let (key, val) = render_environment_key_value("DB_PASSWORD", "db_pass_123");
    assert_eq!(key, "DB_PASSWORD");
    assert_eq!(val, SENSITIVE_VALUE_MASK);

    let entry_sensitive = ProcessEnvironmentEntry {
        key: "SECRET_KEY".to_string(),
        value: "topsecret".to_string(),
    };
    let rendered_sensitive = render_environment_entry(&entry_sensitive);
    assert_eq!(rendered_sensitive.key, "SECRET_KEY");
    assert_eq!(rendered_sensitive.value, SENSITIVE_VALUE_MASK);

    let entry_benign = ProcessEnvironmentEntry {
        key: "HOME".to_string(),
        value: "/example/profile".to_string(),
    };
    let rendered_benign = render_environment_entry(&entry_benign);
    assert_eq!(rendered_benign.key, "HOME");
    assert_eq!(rendered_benign.value, "/example/profile");

    // Batch rendering over a slice
    let entries = vec![
        entry_benign,
        entry_sensitive,
        ProcessEnvironmentEntry {
            key: "ACCESS_TOKEN".to_string(),
            value: "tok".to_string(),
        },
    ];
    let rendered_list = render_environment_entries(&entries);
    assert_eq!(rendered_list.len(), 3);
    assert_eq!(rendered_list[0].value, "/example/profile");
    assert_eq!(rendered_list[1].value, SENSITIVE_VALUE_MASK);
    assert_eq!(rendered_list[2].value, SENSITIVE_VALUE_MASK);

    // Full snapshot rendering
    let snapshot = ProcessEnvironment {
        state: DeviceState::healthy(42),
        working_directory: Some(PathBuf::from("/app")),
        entries,
        truncated_count: 5,
    };
    let rendered_snapshot = render_process_environment(&snapshot);
    assert_eq!(rendered_snapshot.entries.len(), 3);
    assert_eq!(rendered_snapshot.entries[0].value, "/example/profile");
    assert_eq!(rendered_snapshot.entries[1].value, SENSITIVE_VALUE_MASK);
    assert_eq!(rendered_snapshot.entries[2].value, SENSITIVE_VALUE_MASK);
    assert_eq!(rendered_snapshot.truncated_count, 5);
    assert_eq!(
        rendered_snapshot.working_directory,
        Some(PathBuf::from("/app"))
    );

    // format_env_entry escapes newlines and masks sensitive values
    let entry_multiline = ProcessEnvironmentEntry {
        key: "MULTILINE".to_string(),
        value: "line1\nline2\rline3".to_string(),
    };
    assert_eq!(
        format_env_entry(&entry_multiline),
        "MULTILINE=line1\\nline2\\rline3"
    );
    let entry_sensitive_multiline = ProcessEnvironmentEntry {
        key: "SECRET_MULTILINE".to_string(),
        value: "line1\nline2".to_string(),
    };
    assert_eq!(
        format_env_entry(&entry_sensitive_multiline),
        format!("SECRET_MULTILINE={SENSITIVE_VALUE_MASK}")
    );
}
