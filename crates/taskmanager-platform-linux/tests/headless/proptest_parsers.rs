//! Property tests for the Linux procfs parsers.
//!
//! Fuzz targets prove parsers never panic on arbitrary bytes; these
//! properties prove the parsed values are the input values: CPU ticks
//! saturate exactly, the start token moves monotonically with the input,
//! memory retains the exact byte conversion, and I/O counters round-trip.

#![cfg(feature = "test-support")]

use proptest::prelude::*;
use taskmanager_platform_linux::{parse_proc_io, parse_proc_stat, parse_proc_status_memory};

fn proc_stat_text(
    user_ticks: u64,
    system_ticks: u64,
    nice: i32,
    threads: u64,
    start_ticks: u64,
) -> String {
    // Positions mirror /proc/<pid>/stat after `(comm)`: 0=state …
    // 11=utime 12=stime 13=cutime 14=cstime 15=priority 16=nice
    // 17=num_threads 18=itrealvalue 19=starttime — exactly the slots
    // `parse_proc_stat` reads, so empties never shift `split_whitespace`
    // positions (every slot that stays a placeholder is non-empty).
    let mut fields = [
        "S", "0", "1", "1", "0", "-1", "4194304", "100", "200", "0", "0", "", "", "0", "20", "10",
        "", "", "0", "",
    ]
    .map(str::to_owned);
    fields[11] = user_ticks.to_string();
    fields[12] = system_ticks.to_string();
    fields[16] = nice.to_string();
    fields[17] = threads.to_string();
    fields[19] = start_ticks.to_string();
    format!("42 (proptest) {}", fields.join(" "))
}

proptest! {
    #[test]
    fn proc_stat_totals_saturate_exactly(
        user_ticks in 0u64..u64::MAX,
        system_ticks in 0u64..u64::MAX,
        nice in -20i32..20,
        threads in 0u64..1_000_000,
        start_ticks in 0u64..u64::MAX,
    ) {
        let parsed = parse_proc_stat(&proc_stat_text(
            user_ticks,
            system_ticks,
            nice,
            threads,
            start_ticks,
        ))
        .expect("valid stat text parses");
        prop_assert_eq!(
            parsed.cpu_ticks_total(),
            user_ticks.saturating_add(system_ticks)
        );
        prop_assert_eq!(parsed.start_ticks(), start_ticks);
    }

    #[test]
    fn proc_stat_start_token_is_monotonic_in_input(
        base in 0u64..(u64::MAX - 1000),
        delta in 0u64..1000,
    ) {
        let earlier =
            parse_proc_stat(&proc_stat_text(0, 0, 0, 1, base)).expect("valid stat text parses");
        let later = parse_proc_stat(&proc_stat_text(0, 0, 0, 1, base + delta))
            .expect("valid stat text parses");
        prop_assert!(later.start_ticks() >= earlier.start_ticks());
        prop_assert_eq!(later.start_ticks() - earlier.start_ticks(), delta);
    }

    #[test]
    fn proc_status_memory_retains_exact_byte_conversion(kib in 0u64..(u64::MAX / 1024)) {
        let text = format!("Name:\tproptest\nVmRSS:\t{kib} kB\nVmSwap:\t0 kB\n");
        let parsed = parse_proc_status_memory(&text).expect("valid status parses");
        prop_assert_eq!(parsed, kib * 1024);
    }

    #[test]
    fn proc_io_counters_round_trip(
        read_bytes in 0u64..u64::MAX,
        write_bytes in 0u64..u64::MAX,
    ) {
        let text = format!(
            "rchar:\t0\nwchar:\t0\nread_bytes:\t{read_bytes}\nwrite_bytes:\t{write_bytes}\n\
             cancelled_write_bytes:\t0\n"
        );
        let parsed = parse_proc_io(&text);
        prop_assert_eq!(parsed.read_bytes, Ok(read_bytes));
        prop_assert_eq!(parsed.write_bytes, Ok(write_bytes));
    }
}
