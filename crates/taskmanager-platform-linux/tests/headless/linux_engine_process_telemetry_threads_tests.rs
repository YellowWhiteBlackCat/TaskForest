use super::*;
use taskmanager_core::core::device_state::DeviceState;

fn stat_line(comm: &str, state: &str, utime: u64, stime: u64) -> String {
    // Fields after `)`: state(3) ppid(4) pgrp(5) session(6) tty_nr(7)
    // tpgid(8) flags(9) minflt(10) cminflt(11) majflt(12) cmajflt(13)
    // utime(14) stime(15). Fields 4..=13 are ten placeholder tokens,
    // giving utime tail-index 11 and stime tail-index 12.
    let middle = "1 2 3 4 5 6 7 8 9 10";
    format!("42 ({comm}) {state} {middle} {utime} {stime}")
}

#[test]
fn parse_handles_spaced_comm_and_cpu_counters() {
    let parsed = parse_thread_stat(&stat_line("worker thread", "R", 1_200, 800))
        .expect("valid stat line parses");
    assert_eq!(parsed.comm, "worker thread");
    assert_eq!(parsed.state_char, 'R');
    assert_eq!(parsed.utime, 1_200);
    assert_eq!(parsed.stime, 800);
}

#[test]
fn parse_rejects_malformed_lines() {
    assert_eq!(parse_thread_stat("no parens here"), None);
    assert_eq!(parse_thread_stat("42 )reversed( S 1"), None);
    assert_eq!(parse_thread_stat("42 (comm) S"), None); // too few fields
    assert_eq!(
        parse_thread_stat("42 (comm) S 1 2 3 4 5 6 7 8 9 10 notanum 5"),
        None // non-numeric utime
    );
}

#[test]
fn cpu_seconds_use_proc_clk_tck_divisor() {
    // Real test of the parse + divisor math: feed a fixture /proc stat line
    // (utime field 14 + stime field 15 in clock ticks) through the parser
    // and assert cpu_seconds = (utime + stime) / PROC_CLK_TCK — the exact
    // computation `collect_threads_from_proc_dir` performs on a live thread.
    // No /proc dependency: this exercises the parse + the formula directly.
    let parsed =
        parse_thread_stat(&stat_line("worker", "R", 1_200, 800)).expect("valid stat line parses");
    let cpu_seconds = (parsed.utime + parsed.stime) as f64 / PROC_CLK_TCK as f64;
    // 1200 user + 800 kernel == 2000 ticks; at 100 ticks/s == 20.0 s.
    assert_eq!(parsed.utime, 1_200);
    assert_eq!(parsed.stime, 800);
    assert_eq!(cpu_seconds, 20.0);
    // The divisor is the documented _SC_CLK_TCK value on mainstream Linux.
    assert_eq!(PROC_CLK_TCK, 100);

    // Asymmetric case: 100 ticks user + 0 kernel == 1.0 s — pins the
    // boundary so a regression that drops utime or stime is caught.
    let idle = parse_thread_stat(&stat_line("idle", "S", 100, 0)).expect("valid stat line parses");
    assert_eq!((idle.utime + idle.stime) as f64 / PROC_CLK_TCK as f64, 1.0);
}

#[test]
fn thread_state_char_round_trips_canonical_letters() {
    assert_eq!(ThreadState::from_char('R'), ThreadState::Running);
    assert_eq!(ThreadState::from_char('S'), ThreadState::Sleep);
    assert_eq!(
        ThreadState::from_char('D'),
        ThreadState::UninterruptibleSleep
    );
    assert_eq!(ThreadState::from_char('Z'), ThreadState::Zombie);
    assert_eq!(ThreadState::from_char('I'), ThreadState::Idle);
    assert_eq!(ThreadState::from_char('q'), ThreadState::Other);
    assert_eq!(ThreadState::from_char('R').as_short_label(), "R");
    assert_eq!(ThreadState::from_char('q').as_short_label(), "?");
}

#[cfg(target_os = "linux")]
#[test]
fn collect_from_proc_dir_returns_sorted_threads_or_typed_state() {
    use std::path::PathBuf;

    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-threads-{}", std::process::id()));
    let task_dir: PathBuf = root.join("task");
    // 432 before 99 to prove ascending-tid sort.
    std::fs::create_dir_all(task_dir.join("432")).expect("create tid dir");
    std::fs::create_dir_all(task_dir.join("99")).expect("create tid dir");
    std::fs::write(
        task_dir.join("432").join("stat"),
        stat_line("worker", "R", 250, 50),
    )
    .expect("write tid stat");
    std::fs::write(
        task_dir.join("99").join("stat"),
        stat_line("main", "S", 10_000, 0),
    )
    .expect("write tid stat");

    let facet = collect_threads_from_proc_dir(&root, 5_000);
    assert_eq!(facet.state, DeviceState::healthy(5_000));
    let tids: Vec<u32> = facet.threads.iter().map(|thread| thread.tid).collect();
    assert_eq!(tids, vec![99, 432]);
    assert_eq!(facet.threads[0].comm, "main");
    assert_eq!(facet.threads[0].state, ThreadState::Sleep);
    // 10000 ticks / 100 == 100.0 s
    assert_eq!(facet.threads[0].cpu_time_secs, Some(100.0));
    assert_eq!(facet.threads[0].cpu_percent, None);
    assert_eq!(facet.threads[1].cpu_time_secs, Some(3.0));
    assert_eq!(facet.threads[1].cpu_percent, None);

    std::fs::remove_dir_all(&root).expect("remove fixture");
    let stale = collect_threads_from_proc_dir(&root, 6_000);
    assert_eq!(stale.state.status, DeviceStatus::Stale);
    assert!(stale.threads.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn cpu_rate_is_warm_only_and_resets_on_process_identity_change() {
    use std::path::PathBuf;

    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-thread-rate-{}", std::process::id()));
    let task_dir: PathBuf = root.join("task/17");
    std::fs::create_dir_all(&task_dir).expect("create thread rate fixture");
    std::fs::write(task_dir.join("stat"), stat_line("worker", "R", 1_000, 0))
        .expect("write first thread stat");

    let identity = ProcessIdentity {
        pid: 42,
        start_token: 7,
    };
    let mut rates = ThreadCpuRateTracker::default();
    let clock_ticks = Ok(PROC_CLK_TCK);
    let first = collect_threads_with_cpu_rate(&root, identity, 1_000, &clock_ticks, &mut rates);
    assert_eq!(first.threads[0].cpu_percent, None);

    std::fs::write(task_dir.join("stat"), stat_line("worker", "R", 1_100, 0))
        .expect("write second thread stat");
    let warm = collect_threads_with_cpu_rate(&root, identity, 2_000, &clock_ticks, &mut rates);
    assert_eq!(warm.threads[0].cpu_percent, Some(100.0));

    let replacement = ProcessIdentity {
        pid: 42,
        start_token: 8,
    };
    let reused = collect_threads_with_cpu_rate(&root, replacement, 3_000, &clock_ticks, &mut rates);
    assert_eq!(reused.threads[0].cpu_percent, None);

    std::fs::write(task_dir.join("stat"), stat_line("worker", "R", 1_200, 0))
        .expect("write replacement thread stat");
    let replacement_warm =
        collect_threads_with_cpu_rate(&root, replacement, 4_000, &clock_ticks, &mut rates);
    assert_eq!(replacement_warm.threads[0].cpu_percent, Some(100.0));

    std::fs::write(task_dir.join("stat"), stat_line("worker", "R", 1_300, 0))
        .expect("write old-generation thread stat");
    let old_generation =
        collect_threads_with_cpu_rate(&root, identity, 5_000, &clock_ticks, &mut rates);
    assert_eq!(old_generation.threads[0].cpu_percent, None);

    let unavailable_clock = Err(FailureKind::TemporarilyUnavailable);
    let clock_gap =
        collect_threads_with_cpu_rate(&root, identity, 6_000, &unavailable_clock, &mut rates);
    assert_eq!(clock_gap.threads[0].cpu_percent, None);

    std::fs::write(task_dir.join("stat"), stat_line("worker", "R", 1_400, 0))
        .expect("write post-gap thread stat");
    let after_clock_gap =
        collect_threads_with_cpu_rate(&root, identity, 7_000, &clock_ticks, &mut rates);
    assert_eq!(after_clock_gap.threads[0].cpu_percent, None);

    std::fs::remove_dir_all(&root).expect("remove thread rate fixture");
}
