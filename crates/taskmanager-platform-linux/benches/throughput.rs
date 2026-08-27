//! Zero-dependency throughput benches for the hot pure parsers.
//!
//! Output contract (one line per measurement, parsed by
//! scripts/quality/bench-gate.sh):
//!   RESULT<TAB><name><TAB><nanoseconds>
//!
//! These are trend inputs, not assertions: bench-gate.sh compares each
//! measurement against the last recorded row in docs/quality/bench-trend.tsv
//! and fails on a >100% regression. No external benchmark crate: criterion
//! would add a dependency tree for a 2x trend gate that std::time covers.

use std::hint::black_box;
use std::time::Instant;

use taskmanager_platform_linux::{
    parse_proc_io, parse_proc_stat, parse_proc_status_memory, parse_thread_stat,
};

const SAMPLES: usize = 1000;
const WARMUP: usize = 200;

fn proc_stat_line(comm: &str, user_ticks: u64) -> String {
    format!(
        "1234 ({comm}) R 1 2 3 4 5 6 7 8 9 10 {user_ticks} {user_ticks} 0 0 0 0 0 0 0 15 16 17 0 0 0"
    )
}

fn status_line(rss_kib: u64) -> String {
    format!("VmRSS:\t{rss_kib} kB\nRssAnon:\t0 kB\nRssFile:\t0 kB\nRssShmem:\t0 kB")
}

fn measure(name: &str, run: impl Fn() -> usize) {
    for _ in 0..WARMUP {
        black_box(run());
    }
    let started = Instant::now();
    let count = run();
    let elapsed = started.elapsed();
    assert!(
        count >= SAMPLES,
        "{name}: parser must not drop valid samples"
    );
    println!("RESULT\t{name}\t{}", elapsed.as_nanos());
}

fn main() {
    let stat_lines: Vec<String> = (0..SAMPLES)
        .map(|i| proc_stat_line("worker", i as u64))
        .collect();
    measure("proc_stat_1k", || {
        stat_lines
            .iter()
            .filter(|line| parse_proc_stat(line).is_some())
            .count()
    });

    let status_lines: Vec<String> = (0..SAMPLES).map(|i| status_line(i as u64)).collect();
    measure("proc_status_memory_1k", || {
        status_lines
            .iter()
            .filter(|line| parse_proc_status_memory(line).is_ok())
            .count()
    });

    let io_lines: Vec<String> = (0..SAMPLES)
        .map(|i| format!("read_bytes: {i}\nwrite_bytes: {i}"))
        .collect();
    measure("proc_io_1k", || {
        io_lines
            .iter()
            .filter(|line| parse_proc_io(line).read_bytes.is_ok())
            .count()
    });

    measure("thread_stat_1k", || {
        stat_lines
            .iter()
            .filter(|line| parse_thread_stat(line).is_some())
            .count()
    });
}
