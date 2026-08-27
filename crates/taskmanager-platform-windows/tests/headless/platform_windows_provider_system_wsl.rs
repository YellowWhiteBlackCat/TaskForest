use super::*;

fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[test]
fn utf16_running_list_with_localized_header_yields_registered_names() {
    // BOM + UTF-16LE + CRLF + a colon-bearing header + the `*` default
    // marker + an unregistered stray line: only registry names survive.
    let text = "\u{feff}Running distributions:\r\n  Ubuntu-22.04 \r\nDebian\r\n* Arch\r\nnot-registered\r\n";
    let registered: HashSet<&str> = ["Ubuntu-22.04", "Debian", "Arch"].into_iter().collect();
    let names = running_names_from_list_text(&decode_wsl_console_text(&utf16le(text)), &registered);
    assert_eq!(
        names,
        vec![
            "Ubuntu-22.04".to_owned(),
            "Debian".to_owned(),
            "Arch".to_owned()
        ]
    );
}

#[test]
fn utf8_list_output_decodes_without_nuls() {
    let bytes = b"Windows Subsystem for Linux distributions:\nUbuntu\n";
    let registered: HashSet<&str> = ["Ubuntu"].into_iter().collect();
    let names = running_names_from_list_text(&decode_wsl_console_text(bytes), &registered);
    assert_eq!(names, vec!["Ubuntu".to_owned()]);
}

#[test]
fn proc_listing_keeps_numeric_entries_sorted_and_deduped() {
    assert_eq!(
        numeric_proc_ids(b"10\n1\nself\n1\r\ncpuinfo\n\n42\n"),
        vec![1, 10, 42]
    );
}

#[test]
fn stat_line_addresses_fields_after_parenthesized_comm() {
    // comm contains spaces and parentheses; utime/stime sit at stat fields
    // 14/15 relative to the last closing paren.
    let line = "123 (my (fancy) proc) S 1 2 3 4 5 6 7 8 9 10 200 300 0 0 0 0 0 0 0 0 0 7 8";
    assert_eq!(parse_proc_stat_line(line), Some((123, 500)));
}

#[test]
fn truncated_stat_line_is_rejected_not_guessed() {
    assert_eq!(parse_proc_stat_line("123 (comm) S 1 2 3"), None);
    assert_eq!(parse_proc_stat_line("nonsense"), None);
}

#[test]
fn merged_payload_pairs_vmrss_with_own_pid_and_defaults_missing_rss() {
    let payload = "\
1 (init) S 0 1 1 0 -1 4194560 100 50 0 0 10 5 0 0 0 0 0 0 0 0 0 1 2\n\
Pid:\t1\nVmRSS:\t 4 kB\nThreads:\t1\n\
42 (worker) R 40 42 42 0 -1 4194560 500 100 0 0 20 10 0 0 0 0 0 0 0 0 0 3 4\n\
Pid:\t42\nThreads:\t4\n\
99 (ghost) S 2 99 99 0 -1 4194560 1 1 0 0 1 1 0 0 0 0 0 0 0 0 0 1 2\n";
    let samples = merge_proc_samples(payload);
    assert_eq!(
        samples,
        vec![
            WslProcSample {
                pid: 1,
                cpu_jiffies: 15,
                rss_bytes: 4096
            },
            WslProcSample {
                pid: 42,
                cpu_jiffies: 30,
                rss_bytes: 0
            },
            WslProcSample {
                pid: 99,
                cpu_jiffies: 2,
                rss_bytes: 0
            },
        ]
    );
}

#[test]
fn rate_tracker_first_sample_is_a_typed_gap() {
    let mut tracker = WslCpuRateTracker::default();
    assert_eq!(
        tracker.percentage("Ubuntu", 1_000, 10_000),
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn rate_tracker_converts_jiffy_delta_to_single_core_percentage() {
    let mut tracker = WslCpuRateTracker::default();
    tracker.percentage("Ubuntu", 1_000, 10_000);
    // +1_000 jiffies (10 s of CPU at USER_HZ=100) over 10_000 ms -> 100.0%.
    assert_eq!(
        tracker.percentage("Ubuntu", 2_000, 20_000),
        ScalarObservation::available(100.0_f32, 20_000)
    );
}

#[test]
fn rate_tracker_saturates_exited_member_counter_rollbacks() {
    let mut tracker = WslCpuRateTracker::default();
    tracker.percentage("Ubuntu", 5_000, 10_000);
    // Members exited and the aggregate counter fell: an idle floor, not a reset.
    assert_eq!(
        tracker.percentage("Ubuntu", 4_000, 20_000),
        ScalarObservation::available(0.0_f32, 20_000)
    );
}

#[test]
fn rate_tracker_marks_zero_elapsed_and_backwards_clock_as_identity_change() {
    let mut tracker = WslCpuRateTracker::default();
    tracker.percentage("Ubuntu", 5_000, 10_000);
    assert_eq!(
        tracker.percentage("Ubuntu", 5_000, 10_000),
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(
        tracker.percentage("Ubuntu", 5_000, 9_999),
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn parsers_survive_hostile_byte_payloads() {
    // Fuzz contract for the /wsl console + /proc parser surface: arbitrary
    // byte sequences decode and parse without panicking; the refresh path
    // degrades to typed gaps, never to a crash.
    let hostile: [&[u8]; 5] = [
        &[],
        &[0xff, 0xfe, 0xd8, 0x00, 0xdf],
        &[0x00, 0xd8, 0x00, 0x00, 0xff],
        b"\n\r\n\x00\x00\n",
        &[0xc3, 0x28, 0xf0, 0x9f, 0x92],
    ];
    for bytes in hostile {
        let text = decode_wsl_console_text(bytes);
        let registered: HashSet<&str> = HashSet::new();
        let _ = running_names_from_list_text(&text, &registered);
        let _ = numeric_proc_ids(bytes);
        let _ = merge_proc_samples(&text);
    }
}
