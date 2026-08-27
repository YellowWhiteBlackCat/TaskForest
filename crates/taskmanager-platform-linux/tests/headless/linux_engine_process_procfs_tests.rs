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
        })
    );
    assert_eq!(parse_proc_stat("7 malformed"), None);
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
fn process_io_fields_fail_independently_without_fabricating_zero() {
    let parsed = parse_proc_io("read_bytes: 0\nwrite_bytes: broken\n");
    assert_eq!(parsed.read_bytes, Ok(0));
    assert_eq!(parsed.write_bytes, Err(FailureKind::ProviderFault));

    let missing = parse_proc_io("rchar: 12\n");
    assert_eq!(missing.read_bytes, Err(FailureKind::Unsupported));
    assert_eq!(missing.write_bytes, Err(FailureKind::Unsupported));
}
