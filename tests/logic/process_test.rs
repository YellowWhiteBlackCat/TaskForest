use taskmanager::core::ScalarObservation;
use taskmanager::core::process::{
    ProcessApplicationIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessBatchTargetResult,
    ProcessItem, ProcessMetadataObservation, ProcessScalarObservations, ProcessSortKey,
    aggregate_apps, build_process_tree, execute_process_batch_with, fuzzy_filter_processes,
    fuzzy_match, normalize_app_name, sort_processes,
};

fn refs(items: &[ProcessItem]) -> Vec<&ProcessItem> {
    items.iter().collect()
}
// SIGSTOP/SIGCONT and setpriority are Linux-process controls exercised against
// a real child; the rest of this module is neutral core process logic.
#[cfg(target_os = "linux")]
use taskmanager_platform_linux::{ProcessManager, pause_process, resume_process};

#[test]
fn test_process_scan_and_sorting() {
    let mut items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(100)
            .parent_pid(Some(1))
            .name("alpha".to_string())
            .cmdline("alpha --arg".to_string())
            .current_cpu_percentage(15.5)
            .current_memory_bytes(1024 * 1024 * 50)
            .current_disk_read_bytes_per_sec(500)
            .current_disk_write_bytes_per_sec(2000)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("root".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(10)
            .parent_pid(Some(1))
            .name("beta".to_string())
            .cmdline("beta".to_string())
            .current_cpu_percentage(95.0)
            .current_memory_bytes(1024 * 1024 * 500)
            .current_disk_read_bytes_per_sec(10000)
            .current_disk_write_bytes_per_sec(100)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(50)
            .parent_pid(Some(1))
            .name("charlie".to_string())
            .cmdline("charlie".to_string())
            .current_cpu_percentage(2.0)
            .current_memory_bytes(1024 * 1024 * 10)
            .current_disk_read_bytes_per_sec(100)
            .current_disk_write_bytes_per_sec(50)
            .status("Sleeping".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
    ];

    // Sort by CPU Usage Descending
    sort_processes(&mut items, ProcessSortKey::CpuUsage, false);
    assert_eq!(items[0].name, "beta");
    assert_eq!(items[1].name, "alpha");
    assert_eq!(items[2].name, "charlie");

    // Sort by Disk Read Descending
    sort_processes(&mut items, ProcessSortKey::DiskRead, false);
    assert_eq!(items[0].name, "beta"); // 10000 bytes
    assert_eq!(items[1].name, "alpha"); // 500 bytes
    assert_eq!(items[2].name, "charlie"); // 100 bytes

    // Sort by Disk Write Descending
    sort_processes(&mut items, ProcessSortKey::DiskWrite, false);
    assert_eq!(items[0].name, "alpha"); // 2000 bytes
    assert_eq!(items[1].name, "beta"); // 100 bytes
    assert_eq!(items[2].name, "charlie"); // 50 bytes

    // Filter by query "bet"
    let filtered = fuzzy_filter_processes(&items, "bet");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "beta");
}

#[test]
fn test_build_process_tree() {
    let items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .parent_pid(None)
            .name("systemd".to_string())
            .cmdline("/sbin/init".to_string())
            .current_cpu_percentage(0.1)
            .current_memory_bytes(1024 * 1024)
            .current_disk_read_bytes_per_sec(1000)
            .current_disk_write_bytes_per_sec(500)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("root".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(10)
            .parent_pid(Some(1))
            .name("bash".to_string())
            .cmdline("bash".to_string())
            .current_cpu_percentage(0.0)
            .current_memory_bytes(2 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(0)
            .current_disk_write_bytes_per_sec(0)
            .status("Sleeping".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(100)
            .parent_pid(Some(10))
            .name("cargo".to_string())
            .cmdline("cargo test".to_string())
            .current_cpu_percentage(50.0)
            .current_memory_bytes(100 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(5000)
            .current_disk_write_bytes_per_sec(10000)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(200)
            .parent_pid(Some(9999))
            .name("orphan_daemon".to_string())
            .cmdline("orphan_daemon".to_string())
            .current_cpu_percentage(1.0)
            .current_memory_bytes(10 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(0)
            .current_disk_write_bytes_per_sec(0)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("nobody".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
    ];

    let tree = build_process_tree(&refs(&items));
    assert_eq!(tree.len(), 2); // systemd (1) and orphan_daemon (200) as root nodes

    // Find systemd root
    let systemd_node = tree
        .iter()
        .find(|n| n.item.pid == 1)
        .expect("systemd root node");
    assert_eq!(systemd_node.depth, 0);
    assert_eq!(systemd_node.children_pids, vec![10]);
    assert_eq!(systemd_node.children.len(), 1);

    let bash_node = &systemd_node.children[0];
    assert_eq!(bash_node.item.pid, 10);
    assert_eq!(bash_node.depth, 1);
    assert_eq!(bash_node.children_pids, vec![100]);
    assert_eq!(bash_node.children.len(), 1);

    let cargo_node = &bash_node.children[0];
    assert_eq!(cargo_node.item.pid, 100);
    assert_eq!(cargo_node.depth, 2);
    assert!(cargo_node.children_pids.is_empty());
    assert!(cargo_node.children.is_empty());

    // Find orphan daemon root
    let orphan_node = tree
        .iter()
        .find(|n| n.item.pid == 200)
        .expect("orphan root node");
    assert_eq!(orphan_node.depth, 0);
    assert!(orphan_node.children.is_empty());
}

#[test]
fn test_aggregate_apps() {
    let items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1000)
            .parent_pid(Some(1))
            .name("chrome".to_string())
            .cmdline("/usr/bin/chrome".to_string())
            .current_cpu_percentage(10.0)
            .current_memory_bytes(200 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(100)
            .current_disk_write_bytes_per_sec(200)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1001)
            .parent_pid(Some(1000))
            .name("chrome".to_string())
            .cmdline("/usr/bin/chrome --type=gpu-process".to_string())
            .current_cpu_percentage(5.0)
            .current_memory_bytes(100 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(0)
            .current_disk_write_bytes_per_sec(0)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1002)
            .parent_pid(Some(1000))
            .name("chrome".to_string())
            .cmdline("/usr/bin/chrome --type=renderer".to_string())
            .current_cpu_percentage(25.0)
            .current_memory_bytes(300 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(50)
            .current_disk_write_bytes_per_sec(50)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(2000)
            .parent_pid(Some(1))
            .name("code".to_string())
            .cmdline("/usr/share/code/code".to_string())
            .current_cpu_percentage(2.0)
            .current_memory_bytes(150 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(0)
            .current_disk_write_bytes_per_sec(0)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(3000)
            .parent_pid(Some(1))
            .name("zed".to_string())
            .cmdline("zed .".to_string())
            .current_cpu_percentage(1.0)
            .current_memory_bytes(80 * 1024 * 1024)
            .current_disk_read_bytes_per_sec(0)
            .current_disk_write_bytes_per_sec(0)
            .status("Running".to_string())
            .metadata_observations(
                taskmanager_application::ProcessMetadataObservations::current(
                    taskmanager_application::ProcessOwner::opaque("user".to_string()),
                    None,
                    1,
                ),
            )
            .build(),
    ];

    let apps = aggregate_apps(&refs(&items));
    assert_eq!(apps.len(), 3);

    // Chrome should be first due to highest total CPU usage (40.0%)
    let chrome_group = &apps[0];
    assert_eq!(chrome_group.name, "Google Chrome");
    assert_eq!(chrome_group.main_pid, 1000);
    assert_eq!(chrome_group.process_count, 3);
    assert_eq!(chrome_group.pids, vec![1000, 1001, 1002]);
    assert!((chrome_group.total_cpu_usage - 40.0).abs() < f32::EPSILON);
    assert_eq!(
        chrome_group.total_memory_bytes,
        (200 + 100 + 300) * 1024 * 1024
    );

    let vscode_group = apps.iter().find(|g| g.name == "VS Code").unwrap();
    assert_eq!(vscode_group.main_pid, 2000);
    assert_eq!(vscode_group.process_count, 1);

    let zed_group = apps.iter().find(|g| g.name == "Zed").unwrap();
    assert_eq!(zed_group.main_pid, 3000);
    assert_eq!(zed_group.process_count, 1);
}

#[test]
fn test_normalize_app_name() {
    assert_eq!(normalize_app_name("google-chrome", ""), "Google Chrome");
    assert_eq!(normalize_app_name("code-oss", ""), "VS Code");
    assert_eq!(normalize_app_name("zed-editor", ""), "Zed");
    assert_eq!(normalize_app_name("firefox-bin", ""), "Firefox");
    assert_eq!(normalize_app_name("my_app", ""), "my_app");
}

#[test]
fn verified_desktop_identity_drives_app_group_name_without_fabricating_icon_state() {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .name("editor-wrapper".into())
        .cmdline("/opt/editor/editor-wrapper".into())
        .build();
    process.apply_application_identity(ProcessMetadataObservation::available(
        ProcessApplicationIdentity::new(
            "org.example.Editor.desktop",
            "Example Editor",
            Some("example-editor".into()),
        )
        .expect("fixture identity must be non-empty"),
        10,
    ));
    let refs = vec![&process];
    let groups = aggregate_apps(&refs);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Example Editor");
    assert_eq!(
        groups[0]
            .application_identity
            .as_ref()
            .and_then(|identity| identity.icon_token.as_deref()),
        Some("example-editor")
    );
}

/// On Linux, read the kernel's process state char from `/proc/<pid>/stat`.
/// Splitting on the LAST `)` survives a `comm` field that itself contains
/// spaces or parentheses. Returns `None` if the process has already exited.
#[cfg(target_os = "linux")]
fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.trim_start().chars().next()
}

/// Poll the child state briefly until it matches `want_stopped` (`'T'` = stopped
/// by a signal). `pause_process`/`resume_process` return `Ok` once the signal is
/// queued, but the scheduler may take a moment to apply it — this tolerates that
/// race instead of asserting on a single immediate read.
#[cfg(target_os = "linux")]
fn wait_for_state(pid: u32, want_stopped: bool) -> bool {
    for _ in 0..250 {
        if let Some(state) = proc_state(pid)
            && (state == 'T') == want_stopped
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    false
}

#[cfg(target_os = "linux")]
#[test]
fn test_pause_resume_process() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn child process");

    let pid = child.id();

    // Pause (SIGSTOP) — verify the kernel actually stopped the child (state 'T'),
    // not merely that the syscall returned Ok.
    assert!(pause_process(pid).is_ok());
    #[cfg(target_os = "linux")]
    assert!(
        wait_for_state(pid, true),
        "child was not stopped (state 'T') after pause_process; last state: {:?}",
        proc_state(pid)
    );

    // Resume (SIGCONT) — verify it left the stopped state.
    assert!(resume_process(pid).is_ok());
    #[cfg(target_os = "linux")]
    assert!(
        wait_for_state(pid, false),
        "child remained stopped after resume_process; last state: {:?}",
        proc_state(pid)
    );

    // The associated-function path must also actually stop/resume the child.
    assert!(ProcessManager::pause_process(pid).is_ok());
    #[cfg(target_os = "linux")]
    assert!(
        wait_for_state(pid, true),
        "ProcessManager::pause_process did not stop the child"
    );
    assert!(ProcessManager::resume_process(pid).is_ok());
    #[cfg(target_os = "linux")]
    assert!(
        wait_for_state(pid, false),
        "ProcessManager::resume_process did not resume the child"
    );

    // Cleanup child process
    let _ = child.kill();
    let _ = child.wait();

    // Test with invalid PID
    let invalid_pid = 999_999;
    assert!(pause_process(invalid_pid).is_err());
    assert!(resume_process(invalid_pid).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn test_set_process_nice() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn child process");

    let pid = child.id();

    // Setting a higher nice value (e.g., 5 or 10) lowers priority and is allowed for unprivileged processes
    assert!(ProcessManager::set_process_nice(pid, 5).is_ok());
    assert!(ProcessManager::set_process_nice(pid, 10).is_ok());

    // Cleanup child process
    let _ = child.kill();
    let _ = child.wait();

    // Test with invalid PID
    let invalid_pid = 999_999;
    assert!(ProcessManager::set_process_nice(invalid_pid, 5).is_err());
}

#[test]
fn process_batch_freeze_excludes_rows_without_exact_identity_authority() {
    let processes = [
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(41)
            .name("unknown-identity".to_owned())
            .current_start_time_secs(0)
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(42)
            .name("known-identity".to_owned())
            .scalar_observations(ProcessScalarObservations {
                start_token: ScalarObservation::available(7_500, 10),
                ..ProcessScalarObservations::default()
            })
            .current_start_time_secs(1_720_000_000)
            .build(),
    ];

    let intent = ProcessBatchIntent::freeze(&processes, [41, 42], ProcessBatchAction::Suspend);

    assert_eq!(intent.targets.len(), 1);
    let result = execute_process_batch_with(intent, &processes, |_, target| {
        assert_eq!(
            target.pid, 42,
            "only the known identity may reach the executor"
        );
        Ok(())
    });
    assert_eq!(result.targets[0].1, ProcessBatchTargetResult::Applied);
}

// ── fuzzy_match: empty-query, substring, and subsequence paths ─────────────

#[test]
fn test_fuzzy_match_empty_query_always_matches() {
    // An empty/whitespace query short-circuits to true for any target.
    assert!(fuzzy_match("anything", ""));
    assert!(fuzzy_match("", ""));
}

#[test]
fn test_fuzzy_match_substring_path() {
    // Direct substring of the lowercased target → true.
    assert!(fuzzy_match("beta", "bet"));
    assert!(fuzzy_match("Google Chrome Helper", "chrome"));
}

#[test]
fn test_fuzzy_match_subsequence_path() {
    // "sd" is not a substring of "systemd" but the chars appear in order.
    assert!(fuzzy_match("systemd", "sd"));
    // Case-insensitive subsequence: 'h' then 'l' exist in order in "Hello".
    assert!(fuzzy_match("Hello", "hl"));
}

#[test]
fn test_fuzzy_match_order_matters_and_non_match() {
    // Right characters, wrong order → false (subsequence is order-sensitive).
    assert!(!fuzzy_match("abc", "cba"));
    // No overlapping characters → false.
    assert!(!fuzzy_match("cat", "dog"));
    // Non-empty query against an empty target → false (no chars to match).
    assert!(!fuzzy_match("", "x"));
}

// ── fuzzy_filter_processes: each predicate + the empty-query fast path ─────

/// Helper: build a ProcessItem with only the filter-relevant fields set.
fn mk_filter_item(pid: u32, name: &str, cmdline: &str, user: &str) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .parent_pid(Some(1))
        .name(name.to_string())
        .cmdline(cmdline.to_string())
        .current_cpu_percentage(0.0)
        .current_memory_bytes(0)
        .current_disk_read_bytes_per_sec(0)
        .current_disk_write_bytes_per_sec(0)
        .status("Sleeping".to_string())
        .metadata_observations(
            taskmanager_application::ProcessMetadataObservations::current(
                taskmanager_application::ProcessOwner::opaque(user.to_string()),
                None,
                1,
            ),
        )
        .build()
}

/// Three items with deliberately distinct name / cmdline / pid / user so each
/// filter predicate can be isolated. Asserts the EXACT returned pids, not just
/// a count.
#[test]
fn test_filter_processes_matches_name_predicate() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    // Name substring "gamma" only lives in item 300's name.
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "gamma")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, vec![300]);
}

#[test]
fn test_filter_processes_matches_cmdline_predicate() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    // "firefox" is only in item 100's cmdline (its name is "alpha").
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "firefox")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, vec![100]);
}

#[test]
fn test_filter_processes_matches_pid_predicate() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    // Querying the pid as a digit string returns exactly that pid.
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "200")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, vec![200]);
}

#[test]
fn test_filter_processes_matches_user_predicate() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    // "alice" is only item 100's user.
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "alice")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, vec![100]);
}

#[test]
fn test_filter_processes_no_match_returns_empty() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    assert!(fuzzy_filter_processes(&items, "zzz-no-such-thing").is_empty());
}

#[test]
fn test_filter_processes_empty_and_whitespace_query_returns_all() {
    let items = vec![
        mk_filter_item(100, "alpha", "/usr/bin/firefox", "alice"),
        mk_filter_item(200, "beta", "beta-bin", "bob"),
        mk_filter_item(300, "gamma", "gamma-svc", "charlie"),
    ];
    let all_pids: Vec<u32> = items.iter().map(|p| p.pid).collect();

    // Empty query → fast path returns every item, in input order.
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, all_pids);

    // Whitespace-only query trims to empty → same fast path.
    let pids: Vec<u32> = fuzzy_filter_processes(&items, "   ")
        .iter()
        .map(|p| p.pid)
        .collect();
    assert_eq!(pids, all_pids);
}

// ── normalize_app_name: cmdline-side matching + remaining app branches ─────

#[test]
fn test_normalize_app_name_cmdline_branches() {
    // The cmdline side is checked alongside the name side for each app.
    assert_eq!(normalize_app_name("run", "/opt/discord/discord"), "Discord");
    assert_eq!(normalize_app_name("run", "/usr/bin/firefox"), "Firefox");
    assert_eq!(
        normalize_app_name("wrapper", "/opt/google/chrome/chrome"),
        "Google Chrome"
    );
    assert_eq!(
        normalize_app_name("host", "/usr/share/spotify/spotify"),
        "Spotify"
    );
    assert_eq!(normalize_app_name("sh", "/usr/games/steam"), "Steam");
}

#[test]
fn test_normalize_app_name_remaining_name_branches() {
    // Branches uncovered by the existing name-side test.
    assert_eq!(normalize_app_name("Slack Helper", ""), "Slack");
    assert_eq!(normalize_app_name("thunderbird-bin", ""), "Thunderbird");
    // "Code - OSS" matches the `code` predicate (the vscode alias path).
    assert_eq!(normalize_app_name("Code - OSS", ""), "VS Code");
    assert_eq!(normalize_app_name("spotify", "spotify"), "Spotify");
}

#[test]
fn test_normalize_app_name_unknown_and_passthrough() {
    // Empty name with no app keyword → "Unknown".
    assert_eq!(normalize_app_name("", "anything"), "Unknown");
    assert_eq!(normalize_app_name("", ""), "Unknown");
    // Non-empty name with no app keyword → the original name, unchanged.
    assert_eq!(normalize_app_name("my_app", "unrelated"), "my_app");
}

// ── build_process_tree: cycle / self-parent termination guards ─────────────

#[test]
fn test_build_process_tree_self_parent_terminates() {
    // A PID that is its own parent must become a single root with no children,
    // not infinite-recurse. Exercises the is_root self-guard (ppid == pid) AND
    // the visited set inside build_node (the node is its own entry in
    // children_map and must not be re-entered).
    let items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(5)
            .parent_pid(Some(5))
            .name("oddity".to_string())
            .build(),
    ];
    let tree = build_process_tree(&refs(&items));
    assert_eq!(tree.len(), 1);
    let root = &tree[0];
    assert_eq!(root.item.pid, 5);
    assert_eq!(root.depth, 0);
    assert!(root.children.is_empty());
    assert!(root.children_pids.is_empty());
}

#[test]
fn test_build_process_tree_mutual_parent_cycle_terminates() {
    // Two present processes that are each other's parent: neither qualifies as
    // a root (each one's parent is present in the map and isn't itself), so the
    // cycle is dropped entirely. The contract locked in here: build_process_tree
    // terminates (does not infinite-loop) on cyclic parent links.
    let items = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .parent_pid(Some(2))
            .name("a".to_string())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(2)
            .parent_pid(Some(1))
            .name("b".to_string())
            .build(),
    ];
    let tree = build_process_tree(&refs(&items));
    assert!(
        tree.is_empty(),
        "mutual-parent cycle must yield no roots, got {} root(s)",
        tree.len()
    );
}

// ── sort tiebreak: a tied primary key falls back to pid for a deterministic order ─

#[test]
fn test_sort_tiebreak_on_pid_when_cpu_tied() {
    // Two items tied on CPU% (both 0.0) and memory (both 0) — only pid differs.
    // The comparator must fall back to pid so the order is deterministic instead
    // of returning Equal and leaving order up to the caller's sort stability.
    let mut items = vec![
        mk_filter_item(200, "alpha", "alpha", "user"),
        mk_filter_item(100, "alpha", "alpha", "user"),
    ];

    // Ascending: tied primary -> pid ascending -> 100 before 200.
    sort_processes(&mut items, ProcessSortKey::CpuUsage, true);
    assert_eq!(items[0].pid, 100);
    assert_eq!(items[1].pid, 200);

    // Descending: maybe_reverse flips the WHOLE comparison (primary + tiebreak),
    // so the tied pair comes out pid-descending -> 200 before 100. The contract
    // under test is determinism (same input -> same output every run), not the
    // direction itself.
    sort_processes(&mut items, ProcessSortKey::CpuUsage, false);
    assert_eq!(items[0].pid, 200);
    assert_eq!(items[1].pid, 100);
}

#[test]
fn test_sort_tiebreak_on_pid_when_memory_tied() {
    // Same contract for a Memory tie.
    let mut items = vec![
        mk_filter_item(200, "beta", "beta", "user"),
        mk_filter_item(100, "beta", "beta", "user"),
    ];
    sort_processes(&mut items, ProcessSortKey::Memory, true);
    assert_eq!(items[0].pid, 100);
    assert_eq!(items[1].pid, 200);
}

#[test]
fn test_sort_tiebreak_preserves_non_tie_ordering() {
    // When the primary key is NOT tied, the pid tiebreak must not perturb the
    // order: the higher-CPU process ranks above the lower-CPU one regardless of
    // pid. (`.then_with` short-circuits on a non-Equal primary.)
    let hi_cpu = taskmanager_test_support::ProcessItemFixtureBuilder::from_item(mk_filter_item(
        20, "hi", "hi", "u",
    ))
    .pid(20)
    .current_cpu_percentage(50.0)
    .build();
    let lo_cpu = taskmanager_test_support::ProcessItemFixtureBuilder::from_item(mk_filter_item(
        10, "lo", "lo", "u",
    ))
    .pid(10)
    .current_cpu_percentage(5.0)
    .build();
    let mut items = vec![hi_cpu, lo_cpu];
    // Descending CPU: 50.0 (pid 20) before 5.0 (pid 10), even though pid 10 < 20.
    sort_processes(&mut items, ProcessSortKey::CpuUsage, false);
    assert_eq!(items[0].pid, 20);
    assert_eq!(items[1].pid, 10);
}
