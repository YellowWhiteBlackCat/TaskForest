#[test]
fn write_proc_path_builds_the_exact_path_for_pid_boundaries() {
    let mut buffer = [0_u8; 32];
    assert_eq!(
        super::write_proc_path(&mut buffer, 0, "stat"),
        Ok("/proc/0/stat")
    );
    assert_eq!(
        super::write_proc_path(&mut buffer, 42, "status"),
        Ok("/proc/42/status")
    );
    assert_eq!(
        super::write_proc_path(&mut buffer, u32::MAX, "io"),
        Ok("/proc/4294967295/io")
    );
}

#[test]
fn write_proc_path_returns_typed_error_when_leaf_overflows_the_buffer() {
    let mut buffer = [0_u8; 32];
    // "/proc/42/" occupies 9 of the 32 bytes: a 23-byte leaf is the last one
    // that fits; a 24-byte leaf must be a typed ProviderFault, never a
    // slice-bounds panic.
    let fits = "a".repeat(23);
    assert_eq!(
        super::write_proc_path(&mut buffer, 42, &fits),
        Ok(format!("/proc/42/{fits}").as_str())
    );
    let overflows = "b".repeat(24);
    assert_eq!(
        super::write_proc_path(&mut buffer, 42, &overflows),
        Err(FailureKind::ProviderFault)
    );
}

#[test]
fn nth_fields_extracts_only_the_wanted_positions() {
    let text = "a b c d e f g h i j k l m";
    assert_eq!(super::nth_fields(text, [0, 2]), Some(["a", "c"]));
    assert_eq!(super::nth_fields(text, [12]), Some(["m"]));
    // Absent wanted fields yield None, never a default.
    assert_eq!(super::nth_fields(text, [99]), None);
    assert_eq!(super::nth_fields("", [0]), None);
}

use super::*;
use taskforest_core::ProcessSchedulingPolicy;

fn proc_stat(threads: u32, start_ticks: u64, user: u64, system: u64, nice: i32) -> String {
    let mut fields = vec!["0".to_owned(); 20];
    fields[0] = "S".to_owned();
    fields[11] = user.to_string();
    fields[12] = system.to_string();
    fields[16] = nice.to_string();
    fields[17] = threads.to_string();
    fields[19] = start_ticks.to_string();
    format!("7 (worker name) {}", fields.join(" "))
}

#[test]
fn one_stat_parse_owns_identity_threads_cpu_time_and_nice() {
    assert_eq!(
        parse_proc_stat(&proc_stat(8, 1_500, 40, 2, -5)),
        Some(ProcStatFields {
            threads: 8,
            start_ticks: 1_500,
            user_ticks: 40,
            system_ticks: 2,
            nice: -5,
            policy: None,
            minflt: 0,
            majflt: 0,
        })
    );
    assert_eq!(parse_proc_stat("7 malformed"), None);
}

#[test]
fn parse_proc_stat_extracts_scheduling_policy_when_available() {
    let mut fields = vec!["0".to_owned(); 45];
    fields[0] = "R".to_owned();
    fields[7] = "420".to_string();
    fields[9] = "15".to_string();
    fields[11] = "100".to_string();
    fields[12] = "50".to_string();
    fields[16] = "0".to_string();
    fields[17] = "4".to_string();
    fields[19] = "1000".to_string();
    fields[38] = "1".to_string(); // SCHED_FIFO
    let text = format!("123 (realtime) {}", fields.join(" "));
    let parsed = parse_proc_stat(&text).expect("parsed");
    assert_eq!(parsed.policy, Some(ProcessSchedulingPolicy::Fifo));
    assert_eq!(parsed.minflt, 420);
    assert_eq!(parsed.majflt, 15);
}

#[test]
fn clock_ticks_never_fall_back_to_a_guessed_frequency() {
    assert_eq!(normalize_clock_ticks(250), Ok(250));
    assert_eq!(normalize_clock_ticks(0), Err(FailureKind::ProviderFault));
    assert_eq!(normalize_clock_ticks(-1), Err(FailureKind::ProviderFault));
}

#[test]
fn status_memory_keeps_zero_current_and_rejects_missing_or_malformed_units() {
    assert_eq!(
        parse_proc_status_memory("Name:\tworker\nVmRSS:\t0 kB\n"),
        Ok(0)
    );
    assert_eq!(parse_proc_status_memory("VmRSS:\t42 kB\n"), Ok(42 * 1024));
    assert_eq!(
        parse_proc_status_memory("Name:\tworker\n"),
        Err(FailureKind::Unsupported)
    );
    assert_eq!(
        parse_proc_status_memory("VmRSS:\t42 MB\n"),
        Err(FailureKind::ProviderFault)
    );
}

#[test]
fn status_memory_breakdown_keeps_zero_swap_and_rejects_partial_kernels() {
    let text = concat!(
        "VmRSS:\t42 kB\n",
        "RssAnon:\t10 kB\n",
        "RssFile:\t30 kB\n",
        "RssShmem:\t2 kB\n",
        "VmSwap:\t0 kB\n",
    );
    assert_eq!(
        parse_proc_status_memory_fields(text),
        Ok(ProcStatusMemoryFields {
            rss_bytes: 42 * 1024,
            rss_anon_bytes: 10 * 1024,
            rss_file_bytes: 30 * 1024,
            rss_shmem_bytes: 2 * 1024,
        })
    );
    assert_eq!(
        parse_proc_status_memory_fields("VmRSS: 1 kB\nVmSwap: 0 kB\n"),
        Err(FailureKind::Unsupported)
    );
    assert_eq!(
        parse_proc_status_memory_fields(
            "VmRSS: 1 kB\nRssAnon: 1 kB\nRssFile: 1 kB\nRssShmem: 1 MB\nVmSwap: 0 kB\n"
        ),
        Err(FailureKind::ProviderFault)
    );
    // The two results are independent: an older kernel missing the PSS
    // breakdown can still expose a valid VmSwap field.
    assert_eq!(
        parse_proc_status_memory_fields("VmSwap: 0 kB\n"),
        Err(FailureKind::Unsupported)
    );
    assert_eq!(parse_unique_kib_field("VmSwap: 0 kB\n", "VmSwap:"), Ok(0));
}

#[test]
fn smaps_rollup_keeps_uss_and_thp_counters_independently_available() {
    let text = concat!(
        "Rss:           100 kB\n",
        "Pss:             80 kB\n",
        "Private_Clean:  12 kB\n",
        "Private_Dirty:  34 kB\n",
        "AnonHugePages:   8 kB\n",
    );
    let fields = parse_proc_smaps_rollup(text);
    assert_eq!(fields.private_clean_bytes, Ok(12 * 1024));
    assert_eq!(fields.private_dirty_bytes, Ok(34 * 1024));
    assert_eq!(fields.anon_huge_pages_bytes, Ok(8 * 1024));

    let missing_thp = parse_proc_smaps_rollup("Private_Clean: 1 kB\nPrivate_Dirty: 2 kB\n");
    assert_eq!(missing_thp.private_clean_bytes, Ok(1024));
    assert_eq!(missing_thp.private_dirty_bytes, Ok(2 * 1024));
    assert_eq!(
        missing_thp.anon_huge_pages_bytes,
        Err(FailureKind::Unsupported)
    );
}

#[test]
fn process_io_fields_fail_independently_without_fabricating_zero() {
    let parsed = parse_proc_io("read_bytes: 0\nwrite_bytes: broken\ncancelled_write_bytes: 12\n");
    assert_eq!(parsed.read_bytes, Ok(0));
    assert_eq!(parsed.write_bytes, Err(FailureKind::ProviderFault));
    assert_eq!(parsed.cancelled_write_bytes, Ok(12));

    let missing = parse_proc_io("rchar: 12\n");
    assert_eq!(missing.read_bytes, Err(FailureKind::Unsupported));
    assert_eq!(missing.write_bytes, Err(FailureKind::Unsupported));
    assert_eq!(missing.cancelled_write_bytes, Err(FailureKind::Unsupported));
}

/// Build a stat payload with an explicit comm and state so identity tests can
/// exercise the characters the kernel actually allows inside field 2.
fn stat_with(comm: &str, state: &str, parent: &str) -> String {
    let mut fields = vec!["0".to_owned(); 20];
    fields[0] = state.to_owned();
    fields[1] = parent.to_owned();
    fields[7] = "100".to_owned();
    fields[9] = "200".to_owned();
    fields[11] = "1500".to_owned();
    fields[12] = "40".to_owned();
    fields[16] = "-5".to_owned();
    fields[17] = "8".to_owned();
    fields[19] = "2".to_owned();
    format!("7 ({comm}) {}", fields.join(" "))
}

#[test]
fn identity_and_scalars_come_from_one_payload_and_never_disagree() {
    let text = stat_with("worker name", "S", "1");
    let identity = parse_proc_identity(&text).expect("a well-formed stat has identity");
    assert_eq!(identity.comm, "worker name");
    assert_eq!(identity.state.label(), "Sleeping");
    assert_eq!(identity.parent_pid, Some(1));
    // The scalar half parses from the same bytes, so a row cannot carry one
    // process's name and another process's counters.
    let scalars = parse_proc_stat(&text).expect("the same payload yields scalars");
    assert_eq!(scalars.start_ticks, 2);
    assert_eq!(scalars.threads, 8);
}

#[test]
fn comm_containing_parentheses_and_spaces_uses_the_outermost_delimiters() {
    // proc(5): the only safe split points are the FIRST '(' and the LAST ')'.
    let text = stat_with("a (b) c", "R", "42");
    let identity = parse_proc_identity(&text).expect("nested parentheses parse");
    assert_eq!(identity.comm, "a (b) c");
    assert_eq!(identity.state.label(), "Running");
    assert_eq!(identity.parent_pid, Some(42));
    assert!(
        parse_proc_stat(&text).is_some(),
        "the scalar half must use the same delimiters"
    );
}

#[test]
fn parent_pid_zero_is_absent_rather_than_a_process_named_zero() {
    let identity = parse_proc_identity(&stat_with("init", "S", "0")).expect("parses");
    assert_eq!(identity.parent_pid, None);
}

#[test]
fn malformed_or_short_stat_yields_no_identity() {
    assert_eq!(parse_proc_identity("7 malformed S 1"), None);
    // A ')' before '(' would invert the slice; it is a parse failure, not a
    // panic and not a garbage comm.
    assert_eq!(parse_proc_identity("7 ) ( S 1"), None);
    assert_eq!(parse_proc_stat("7 ) ( S 1"), None);
    // State and parent are the first two tail fields; a payload with only the
    // state has no parent to publish.
    assert_eq!(parse_proc_identity("7 (x) S"), None);
}

#[test]
fn every_kernel_state_byte_maps_to_the_published_status_vocabulary() {
    let cases = [
        ("R", "Running"),
        ("S", "Sleeping"),
        ("I", "Idle"),
        ("D", "UninterruptibleDiskSleep"),
        ("Z", "Zombie"),
        ("T", "Stopped"),
        ("t", "Tracing"),
        ("X", "Dead"),
        ("x", "Dead"),
        ("K", "Wakekill"),
        ("W", "Waking"),
        ("P", "Parked"),
        ("Q", "Other"),
        ("", "Other"),
    ];
    for (state, label) in cases {
        let text = stat_with("proc", state, "1");
        let identity = parse_proc_identity(&text);
        if state.is_empty() {
            // An empty state field collapses into the next whitespace split,
            // so the honest result is either Other or a typed parse failure —
            // never a guessed state.
            assert!(
                identity.is_none() || identity.expect("parsed").state.label() == label,
                "an empty state field must not invent a state"
            );
            continue;
        }
        assert_eq!(identity.expect("parses").state.label(), label, "{state}");
    }
    assert_eq!(ProcessState::from_stat_field(None).label(), "Other");
}

#[test]
fn cmdline_splits_on_nul_and_keeps_the_published_arguments_verbatim() {
    assert_eq!(split_cmdline(b"grep\0 foo \0"), ["grep", " foo "]);
    // Only the trailing terminator is dropped; an interior empty argument is
    // real argv and must not shift the display.
    assert_eq!(split_cmdline(b"a\0\0b\0"), ["a", "", "b"]);
    assert_eq!(split_cmdline(b"a\0b"), ["a", "b"]);
    assert_eq!(split_cmdline(&[]), Vec::<String>::new());
    assert_eq!(split_cmdline(b"\0"), Vec::<String>::new());
    // Non-UTF-8 argv is lossy-decoded for display, never dropped.
    let decoded = split_cmdline(b"a\xffb\0c\0");
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[1], "c");
    assert!(decoded[0].contains('b'));
}
