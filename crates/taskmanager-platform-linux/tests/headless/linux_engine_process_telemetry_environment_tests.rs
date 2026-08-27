use super::*;

#[test]
fn parse_entries_splits_orderly_and_skips_noise() {
    let bytes = b"PATH=/usr/bin\0HOME=/home/<user>\0NO_EQ\0EMPTY=\0";
    let (entries, truncated) = parse_entries(bytes, false);
    assert_eq!(truncated, 0);
    assert_eq!(
        entries,
        vec![
            ProcessEnvironmentEntry {
                key: "PATH".into(),
                value: "/usr/bin".into(),
            },
            ProcessEnvironmentEntry {
                key: "HOME".into(),
                value: "/home/<user>".into(),
            },
            ProcessEnvironmentEntry {
                key: "EMPTY".into(),
                value: String::new(),
            },
        ]
    );
}

#[test]
fn byte_cap_partial_tail_counts_as_truncated() {
    // The final entry is clipped with no trailing NUL: it must be counted,
    // not surfaced as a corrupt partial value.
    let (entries, truncated) = parse_entries(b"K=V\0SECRET=PARTIAL", true);
    assert_eq!(entries.len(), 1);
    assert_eq!(truncated, 1);
}

#[test]
fn entry_cap_counts_dropped_entries() {
    let mut bytes = Vec::new();
    for index in 0..(MAX_ENVIRONMENT_ENTRIES + 5) {
        bytes.extend_from_slice(format!("KEY{index}=value\0").as_bytes());
    }
    let (entries, truncated) = parse_entries(&bytes, false);
    assert_eq!(entries.len(), MAX_ENVIRONMENT_ENTRIES);
    assert_eq!(truncated, 5);
}

#[test]
fn missing_proc_dir_is_typed_stale() {
    let value = collect_environment_from_proc_dir(Path::new("/nonexistent/pid"), 1);
    assert_eq!(value.state.status, DeviceStatus::Stale);
    assert!(value.entries.is_empty());
}

#[test]
#[cfg(target_os = "linux")]
fn fixture_cwd_and_environ_are_read_with_healthy_state() {
    use std::os::unix::fs::symlink;

    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-environment-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create environment fixture");
    symlink("/srv/app", root.join("cwd")).expect("symlink cwd");
    std::fs::write(root.join("environ"), b"PATH=/usr/bin\0HOME=/home/<user>\0")
        .expect("write environ fixture");

    let value = collect_environment_from_proc_dir(&root, 1);
    std::fs::remove_dir_all(&root).expect("remove environment fixture");
    assert_eq!(value.state.status, DeviceStatus::Healthy);
    assert_eq!(
        value.working_directory.as_deref(),
        Some(Path::new("/srv/app"))
    );
    assert_eq!(value.entries.len(), 2);
}
