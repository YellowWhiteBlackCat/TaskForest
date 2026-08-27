use super::*;
use taskmanager_core::ThreadState;

fn stat(token: u64) -> String {
    format!("42 (worker) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 {token} 20")
}

fn fixture_root(label: &str) -> PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-process-facet-{label}-{}",
        std::process::id()
    ))
}

#[test]
fn raw_identity_uses_procfs_start_ticks_and_detects_reuse() {
    let root = fixture_root("identity");
    let process_dir = root.join("42");
    std::fs::create_dir_all(&process_dir).expect("create fixture proc directory");
    std::fs::write(process_dir.join("stat"), stat(9_000)).expect("write initial identity");
    let identity = read_process_identity(&root, 42).expect("read fixture identity");
    assert_eq!(identity.start_token, 9_000);

    std::fs::write(process_dir.join("stat"), stat(9_001)).expect("replace fixture identity");
    assert_eq!(
        validate_post_collection_identity(&root, identity),
        Err(ProviderFailure::IdentityChanged)
    );

    std::fs::remove_dir_all(root).expect("remove fixture proc directory");
}

#[cfg(target_os = "linux")]
#[test]
fn network_collector_preserves_success_and_resets_state_on_pid_reuse() {
    let root = fixture_root("network-state");
    let proc_dir = root.join("42");
    std::fs::create_dir_all(proc_dir.join("fd")).expect("create fd fixture");
    std::fs::create_dir_all(proc_dir.join("net")).expect("create net fixture");
    std::fs::write(proc_dir.join("stat"), stat(100)).expect("write initial identity");
    for table in ["tcp", "tcp6", "udp", "udp6"] {
        std::fs::write(proc_dir.join("net").join(table), "header\n").expect("write socket table");
    }
    std::fs::write(
        proc_dir.join("net/unix"),
        "Num RefCount Protocol Flags Type St Inode Path\n",
    )
    .expect("write Unix table");

    let mut collector = ProcessNetworkCollector::default();
    let healthy = collector
        .collect_from_root(&root, 42, 1_000)
        .expect("collect healthy network facet");
    assert_eq!(healthy.value.state, DeviceState::healthy(1_000));

    std::fs::remove_file(proc_dir.join("net/tcp")).expect("remove socket table");
    let stale = collector
        .collect_from_root(&root, 42, 2_000)
        .expect("collect stale network facet");
    assert_eq!(
        stale.value.state.status,
        taskmanager_core::DeviceStatus::Stale
    );
    assert_eq!(stale.value.state.last_success_ms, Some(1_000));

    std::fs::write(proc_dir.join("stat"), stat(200)).expect("replace process identity");
    let reused = collector
        .collect_from_root(&root, 42, 3_000)
        .expect("collect reused process facet");
    assert_eq!(reused.identity.start_token, 200);
    assert_eq!(reused.value.state.last_success_ms, None);
    std::fs::remove_dir_all(root).expect("remove network fixture");
}

/// The collector-level live-pid prune: per-identity state for a pid that left
/// the authoritative set is dropped (a later collection re-seeds its state),
/// while a live pid — including another open insight target — keeps its
/// recorded history. The stale transition makes both halves observable: a
/// kept state entry preserves `last_success_ms`, a pruned one loses it.
#[cfg(target_os = "linux")]
#[test]
fn network_collector_prunes_exited_pid_state_but_keeps_live_targets() {
    let root = fixture_root("network-prune");
    for pid in [42_u32, 43] {
        let proc_dir = root.join(pid.to_string());
        std::fs::create_dir_all(proc_dir.join("fd")).expect("create fd fixture");
        std::fs::create_dir_all(proc_dir.join("net")).expect("create net fixture");
        std::fs::write(proc_dir.join("stat"), stat(100)).expect("write identity");
        for table in ["tcp", "tcp6", "udp", "udp6"] {
            std::fs::write(proc_dir.join("net").join(table), "header\n")
                .expect("write socket table");
        }
        std::fs::write(
            proc_dir.join("net/unix"),
            "Num RefCount Protocol Flags Type St Inode Path\n",
        )
        .expect("write Unix table");
    }

    let mut collector = ProcessNetworkCollector::default();
    for pid in [42_u32, 43] {
        let snapshot = collector
            .collect_from_root(&root, pid, 1_000)
            .expect("collect healthy network facet");
        assert_eq!(snapshot.value.state, DeviceState::healthy(1_000));
    }

    // Only pid 42 is still live; both pids' socket tables then go unreadable.
    std::fs::remove_file(root.join("42/net/tcp")).expect("remove live target table");
    std::fs::remove_file(root.join("43/net/tcp")).expect("remove exited target table");
    collector.retain_live_pids(&HashSet::from([42]));

    let kept = collector
        .collect_from_root(&root, 42, 2_000)
        .expect("collect live target facet");
    assert_eq!(
        kept.value.state.status,
        taskmanager_core::DeviceStatus::Stale
    );
    assert_eq!(
        kept.value.state.last_success_ms,
        Some(1_000),
        "a live pid's collector state survives the prune"
    );
    let pruned = collector
        .collect_from_root(&root, 43, 2_000)
        .expect("collect exited target facet");
    assert_eq!(
        pruned.value.state.status,
        taskmanager_core::DeviceStatus::Stale
    );
    assert_eq!(
        pruned.value.state.last_success_ms, None,
        "an exited pid's collector state is dropped and re-seeds from scratch"
    );
    std::fs::remove_dir_all(root).expect("remove prune fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn gpu_collector_rate_state_is_scoped_to_raw_process_identity() {
    let root = fixture_root("gpu-rate");
    let proc_dir = root.join("42");
    std::fs::create_dir_all(proc_dir.join("fdinfo")).expect("create fdinfo fixture");
    std::fs::write(proc_dir.join("stat"), stat(100)).expect("write initial identity");
    std::fs::write(
        proc_dir.join("fdinfo/7"),
        "drm-pdev:\t0000:03:00.0\ndrm-engine-render:\t1000000 ns\n",
    )
    .expect("write first GPU counter");

    let mut collector = ProcessGpuCollector::default();
    let first = collector
        .collect_from_root(&root, 42, 1_000)
        .expect("collect first GPU facet");
    assert_eq!(first.value.devices[0].utilization_pct, None);

    std::fs::write(
        proc_dir.join("fdinfo/7"),
        "drm-pdev:\t0000:03:00.0\ndrm-engine-render:\t2000000 ns\n",
    )
    .expect("write second GPU counter");
    let second = collector
        .collect_from_root(&root, 42, 2_000)
        .expect("collect second GPU facet");
    assert!(second.value.devices[0].utilization_pct.is_some());

    std::fs::write(proc_dir.join("stat"), stat(200)).expect("replace process identity");
    let reused = collector
        .collect_from_root(&root, 42, 3_000)
        .expect("collect reused GPU facet");
    assert_eq!(reused.identity.start_token, 200);
    assert_eq!(reused.value.devices[0].utilization_pct, None);
    std::fs::remove_dir_all(root).expect("remove GPU fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn open_files_collector_carries_identity_and_classifies_targets() {
    use std::os::unix::fs::symlink;
    let root = fixture_root("open-files");
    let proc_dir = root.join("42");
    let fd_dir = proc_dir.join("fd");
    std::fs::create_dir_all(&fd_dir).expect("create fd fixture");
    std::fs::write(proc_dir.join("stat"), stat(100)).expect("write identity");
    symlink("socket:[4242]", fd_dir.join("3")).expect("ln socket");
    symlink("/var/log/app.log", fd_dir.join("7")).expect("ln file");

    let mut collector = ProcessOpenFilesCollector;
    let snapshot = collector
        .collect_from_root(&root, 42, 1_000)
        .expect("collect open-files facet");
    assert_eq!(snapshot.identity.start_token, 100);
    assert_eq!(snapshot.value.state, DeviceState::healthy(1_000));
    assert_eq!(snapshot.value.entries.len(), 2);
    assert_eq!(snapshot.value.entries[0].fd, 3);
    std::fs::remove_dir_all(root).expect("remove open-files fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn threads_collector_carries_identity_and_parses_per_tid_stat() {
    let root = fixture_root("threads");
    let proc_dir = root.join("42");
    let task_dir = proc_dir.join("task");
    std::fs::create_dir_all(task_dir.join("99")).expect("create tid dir");
    std::fs::create_dir_all(task_dir.join("432")).expect("create tid dir");
    std::fs::write(proc_dir.join("stat"), stat(100)).expect("write identity");
    // After `)`, state is tail index 0; utime/stime are fields 14/15, i.e.
    // tail indices 11/12, so exactly ten filler tokens sit between them.
    std::fs::write(
        task_dir.join("99").join("stat"),
        "42 (main) S 1 2 3 4 5 6 7 8 9 10 10000 0",
    )
    .expect("write tid stat");
    std::fs::write(
        task_dir.join("432").join("stat"),
        "42 (worker) R 1 2 3 4 5 6 7 8 9 10 250 50",
    )
    .expect("write tid stat");

    let mut collector = ProcessThreadsCollector::default();
    let snapshot = collector
        .collect_from_root(&root, 42, 1_000)
        .expect("collect threads facet");
    assert_eq!(snapshot.identity.start_token, 100);
    assert_eq!(snapshot.value.state, DeviceState::healthy(1_000));
    let tids: Vec<u32> = snapshot.value.threads.iter().map(|t| t.tid).collect();
    assert_eq!(tids, vec![99, 432]);
    assert_eq!(snapshot.value.threads[0].comm, "main");
    assert_eq!(snapshot.value.threads[0].state, ThreadState::Sleep);
    // 10000 ticks / 100 == 100.0 s
    assert_eq!(snapshot.value.threads[0].cpu_time_secs, Some(100.0));
    assert_eq!(snapshot.value.threads[1].cpu_time_secs, Some(3.0));
    std::fs::remove_dir_all(root).expect("remove threads fixture");
}
