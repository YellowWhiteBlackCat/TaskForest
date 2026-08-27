//! Deterministic performance regression gates for shared process algorithms.
//!
//! These are deliberately coarse wall-clock checks, not microbenchmarks. The
//! limits leave orders of magnitude of headroom for debug builds and loaded CI
//! workers while still catching hangs and accidental quadratic rewrites.
//!
//! Three production performance areas are covered:
//! - **Shared data algorithms** (filter / fuzzy / sort / tree): the pure-logic
//!   helpers under `taskmanager::core::process` plus the UI's substring filter.
//! - **Hot collect path**: the per-tick `/proc/{pid}/{stat,status,io}` parse that
//!   drives the process table. A quadratic parser rewrite gets its own
//!   `Instant`-timed gate below.
//! - **Collection loop CPU + memory**: the full per-tick pipeline (three proc
//!   parsers per process + correlated host/CPU/memory ingestion) is timed in
//!   wall clock, process CPU time (`/proc/self/stat` ticks), and retained
//!   memory (counting global allocator). Bounded histories allocate at
//!   construction, so steady-state retained bytes must not grow.

use std::collections::HashSet;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "linux")]
use taskmanager::core::CpuTelemetryObservation;
#[cfg(target_os = "linux")]
use taskmanager::core::MemoryTelemetryObservation;
use taskmanager::core::process::{
    ProcessItem, ProcessSortKey, build_process_tree, flatten_tree_visible, fuzzy_filter_processes,
    sort_processes,
};
#[cfg(target_os = "linux")]
use taskmanager::core::{
    CpuMetrics, CpuScalarObservations, MemoryMetrics, MemoryScalarObservations, ScalarObservation,
    ScalarObservationGroup,
};
// Imports consumed only by the Linux-gated `/proc` collect-path tests below.
#[cfg(target_os = "linux")]
use taskmanager::core::HostRuntimeFacts;
#[cfg(target_os = "linux")]
use taskmanager::core::HostRuntimeObservation;
use taskmanager_shell::matches_process_query;
#[cfg(target_os = "linux")]
use taskmanager_telemetry_store::CorrelatedSystemTelemetryIngestor;
#[cfg(target_os = "linux")]
use taskmanager_telemetry_store::{CorrelatedTelemetryStamp, TelemetryStore};

fn refs(items: &[ProcessItem]) -> Vec<&ProcessItem> {
    items.iter().collect()
}

#[cfg(target_os = "linux")]
fn measured_cpu(usage: f32, core_usages: Vec<f32>, at_ms: u64) -> CpuMetrics {
    let core_usage_group = if core_usages.is_empty() {
        ScalarObservationGroup::default()
    } else {
        ScalarObservationGroup::available(core_usages, at_ms)
    };
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(usage, at_ms),
        core_usage_group,
        ..Default::default()
    })
}

#[cfg(target_os = "linux")]
fn measured_memory(total: u64, used: u64, at_ms: u64) -> MemoryMetrics {
    MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, at_ms),
            used_bytes: ScalarObservation::available(used, at_ms),
            ..Default::default()
        },
        Default::default(),
    )
}
const SMALL_WORKLOAD: usize = 1_000;
const LARGE_WORKLOAD: usize = 10_000;
const SMALL_WALL_LIMIT: Duration = Duration::from_secs(2);
const LARGE_WALL_LIMIT: Duration = Duration::from_secs(8);

/// One telemetry tick's `/proc` parse over a synthetic proc tree. Every tick
/// the host domain reads `/proc/{pid}/stat`, `/proc/{pid}/status`, and
/// `/proc/{pid}/io` for each process; this gates the combined parser cost.
///
/// **Host-class assumption:** as above — debug build on a modern x86_64 laptop
/// or CI runner. The three parsers are O(fields) per process, so 10k processes
/// × 3 parsers should sit well under this even unoptimized; observed steady-
/// state is ~50-85ms uninstrumented. The Coverage job runs the same parse under
/// llvm-cov instrumentation, which adds enough overhead to push it to ~240ms,
/// so 400ms absorbs instrumentation + a ~1.6x scheduler-jitter swing while a
/// dominant quadratic rewrite (~50x) would land around 2.5s and still blow this.
#[cfg(target_os = "linux")]
const PROC_PARSE_LIMIT: Duration = Duration::from_millis(400);

/// Eight-way heap-shaped parent links keep recursion shallow while exercising
/// every parent/child lookup. Reverse input order ensures algorithms cannot rely
/// on parents appearing before children.
fn synthetic_processes(count: usize) -> Vec<ProcessItem> {
    (0..count)
        .rev()
        .map(|index| {
            let pid = index as u32 + 1;
            let parent_pid = (index != 0).then(|| ((index - 1) / 8) as u32 + 1);
            let marked = index % 10 == 0;
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(pid)
                .parent_pid(parent_pid)
                .name(if marked {
                    format!("needle-worker-{index:05}")
                } else {
                    format!("worker-{index:05}")
                })
                .cmdline(format!("/usr/bin/worker --bucket={}", index % 97))
                .current_cpu_percentage(((index * 37) % 257) as f32)
                .current_memory_bytes(((index * 65_537) % 1_000_003) as u64)
                .current_disk_read_bytes_per_sec((index * 17) as u64)
                .current_disk_write_bytes_per_sec((index * 19) as u64)
                .status(if index % 7 == 0 {
                    "Running".into()
                } else {
                    "Sleeping".into()
                })
                .metadata_observations(
                    taskmanager_application::ProcessMetadataObservations::current(
                        taskmanager_application::ProcessOwner::opaque(format!(
                            "user-{}",
                            index % 16
                        )),
                        None,
                        1,
                    ),
                )
                .build()
        })
        .collect()
}

fn assert_wall_limit(path: &str, count: usize, elapsed: Duration, limit: Duration) {
    assert!(
        elapsed <= limit,
        "{path} on {count} synthetic processes took {elapsed:?}, exceeding the intentionally \
         loose {limit:?} catastrophic-regression limit"
    );
}

/// Ten times the input should not take catastrophically more than the small
/// workload. The three-second floor absorbs timer noise and heavily loaded CI;
/// the 60x multiplier is still far above expected O(n) / O(n log n) growth but
/// below the roughly 100x signature of a dominant quadratic path.
fn assert_non_catastrophic_scaling(path: &str, small: Duration, large: Duration) {
    let scaled_limit = small
        .saturating_mul(60)
        .max(Duration::from_secs(3))
        .min(LARGE_WALL_LIMIT);
    assert!(
        large <= scaled_limit,
        "{path} scaling regressed: 1k={small:?}, 10k={large:?}, allowed 10k={scaled_limit:?}"
    );
}

fn filter_and_sort(count: usize, limit: Duration) -> Duration {
    let processes = synthetic_processes(count);
    let mut sorted = processes.clone();

    let started = Instant::now();
    let processes_refs = refs(&processes);
    let filtered: Vec<_> = processes_refs
        .iter()
        .copied()
        .filter(|process| matches_process_query(process, "needle"))
        .collect();
    let fuzzy_filtered = fuzzy_filter_processes(&processes, "needle");
    sort_processes(&mut sorted, ProcessSortKey::CpuUsage, true);
    let elapsed = started.elapsed();

    assert_eq!(filtered.len(), count / 10);
    assert!(
        filtered
            .iter()
            .all(|process| process.name.starts_with("needle-worker-"))
    );
    assert_eq!(
        fuzzy_filtered
            .iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>(),
        filtered
            .iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>(),
        "UI substring filtering and the shared fuzzy-filter direct-match path agree"
    );
    assert!(sorted.windows(2).all(|pair| {
        pair[0].current_cpu_percentage() < pair[1].current_cpu_percentage()
            || (pair[0].current_cpu_percentage() == pair[1].current_cpu_percentage()
                && pair[0].pid < pair[1].pid)
    }));
    assert_wall_limit("shared filter + sort", count, elapsed, limit);
    elapsed
}

fn build_and_flatten_tree(count: usize, limit: Duration) -> Duration {
    let processes = synthetic_processes(count);
    let started = Instant::now();
    let tree = build_process_tree(&refs(&processes));
    let flattened = flatten_tree_visible(&tree, &HashSet::new());
    let elapsed = started.elapsed();

    assert_eq!(tree.len(), 1, "heap workload has exactly one root");
    assert_eq!(tree[0].item.pid, 1);
    assert_eq!(flattened.len(), count);

    let mut positions = vec![usize::MAX; count + 1];
    for (position, row) in flattened.iter().enumerate() {
        let slot = &mut positions[row.item.pid as usize];
        assert_eq!(*slot, usize::MAX, "PID must appear exactly once");
        *slot = position;
    }
    assert!(
        positions[1..]
            .iter()
            .all(|position| *position != usize::MAX)
    );
    for process in &processes {
        if let Some(parent) = process.parent_pid {
            assert!(
                positions[parent as usize] < positions[process.pid as usize],
                "parent must precede child in flattened depth-first order"
            );
        }
    }

    assert_wall_limit("tree build + flatten", count, elapsed, limit);
    elapsed
}

#[test]
fn shared_filter_and_sort_scale_to_ten_thousand_processes() {
    let small = filter_and_sort(SMALL_WORKLOAD, SMALL_WALL_LIMIT);
    let large = filter_and_sort(LARGE_WORKLOAD, LARGE_WALL_LIMIT);
    assert_non_catastrophic_scaling("shared filter + sort", small, large);
}

#[test]
fn tree_build_and_flatten_scale_to_ten_thousand_processes() {
    let small = build_and_flatten_tree(SMALL_WORKLOAD, SMALL_WALL_LIMIT);
    let large = build_and_flatten_tree(LARGE_WORKLOAD, LARGE_WALL_LIMIT);
    assert_non_catastrophic_scaling("tree build + flatten", small, large);
}

// ─── Linux collection hot-path timing gate ───────────────────────────────

/// COLLECT gate: time one telemetry tick's worth of `/proc` parsing —
/// `/proc/{pid}/stat` + `/proc/{pid}/status` + `/proc/{pid}/io` — over a
/// synthetic proc tree. The three parsers (`parse_proc_stat`,
/// `parse_proc_status_memory`, `parse_proc_io`) are the actual per-tick hot
/// path; a quadratic rewrite of any of them would blow this budget. Fixture
/// text mirrors real `/proc` field layout.
#[cfg(target_os = "linux")]
#[test]
fn proc_parse_of_ten_thousand_synthetic_ticks_stays_under_budget() {
    let stat_texts: Vec<String> = (0..LARGE_WORKLOAD).map(synthetic_proc_stat_text).collect();
    let status_texts: Vec<String> = (0..LARGE_WORKLOAD)
        .map(synthetic_proc_status_text)
        .collect();
    let io_texts: Vec<String> = (0..LARGE_WORKLOAD).map(synthetic_proc_io_text).collect();

    // Sanity: the fixtures must actually parse (the parser's own unit tests cover
    // field-by-field correctness; here we only confirm we are timing the real
    // parse path, not a `None`/error fast-path that would make the gate trivial).
    assert!(
        stat_texts
            .iter()
            .all(|t| taskmanager_platform_linux::parse_proc_stat(t).is_some())
    );
    assert!(
        status_texts
            .iter()
            .all(|t| taskmanager_platform_linux::parse_proc_status_memory(t).is_ok())
    );
    assert!(io_texts.iter().all(|t| {
        let fields = taskmanager_platform_linux::parse_proc_io(t);
        fields.read_bytes.is_ok() && fields.write_bytes.is_ok()
    }));

    // Warm up: touch every allocation once so the timed loop measures steady-state
    // parse cost, not the allocator's first-touch page faults.
    for i in 0..LARGE_WORKLOAD {
        let _ = taskmanager_platform_linux::parse_proc_stat(&stat_texts[i]);
        let _ = taskmanager_platform_linux::parse_proc_status_memory(&status_texts[i]);
        let _ = taskmanager_platform_linux::parse_proc_io(&io_texts[i]);
    }

    let started = Instant::now();
    for i in 0..LARGE_WORKLOAD {
        let _ = taskmanager_platform_linux::parse_proc_stat(&stat_texts[i]);
        let _ = taskmanager_platform_linux::parse_proc_status_memory(&status_texts[i]);
        let _ = taskmanager_platform_linux::parse_proc_io(&io_texts[i]);
    }
    let elapsed = started.elapsed();
    eprintln!(
        "per-tick /proc parse over {LARGE_WORKLOAD} processes (stat+status+io each): {elapsed:?} (limit {PROC_PARSE_LIMIT:?})"
    );

    assert!(
        elapsed <= PROC_PARSE_LIMIT,
        "per-tick /proc parse over {LARGE_WORKLOAD} synthetic processes took {elapsed:?}, \
         exceeding the {PROC_PARSE_LIMIT:?} ceiling. Assumption: a modern x86_64 developer \
         laptop or CI runner in debug mode."
    );
}

/// Build a `/proc/{pid}/stat` body with realistic field positions. Field layout
/// after the `)` matches the kernel's `fs/proc/array.c` order; the parser reads
/// `rest[11]` (utime), `rest[12]` (stime), `rest[16]` (nice), `rest[17]`
/// (num_threads), `rest[19]` (starttime) — so every field the parser touches is
/// populated with a distinct, parseable value.
#[cfg(target_os = "linux")]
fn synthetic_proc_stat_text(index: usize) -> String {
    let pid = index as u32 + 1;
    let user_ticks = ((index * 37) % 257) as u64;
    let system_ticks = ((index * 19) % 251) as u64;
    let nice: i32 = -5 + ((index % 10) as i32);
    let threads = 1 + (index % 8) as u64;
    let start_ticks = 1000 + index as u64;

    // 20 trailing fields (indices 0..=19), matching the procfs test fixture.
    let mut fields = [
        "S",       // 0: state
        "0",       // 1: ppid
        "1",       // 2: pgrp
        "1",       // 3: session
        "0",       // 4: tty_nr
        "-1",      // 5: tpgid
        "4194304", // 6: flags
        "100",     // 7: minflt
        "200",     // 8: cminflt
        "0",       // 9: majflt
        "0",       // 10: cmajflt
        "",        // 11: utime   (set per-index below)
        "",        // 12: stime
        "20",      // 13: priority
        "",        // 14: nice
        "",        // 15: num_threads
        "0",       // 16: itrealvalue
        "",        // 17: starttime
        "0",       // 18: vsize
        "4194304", // 19: rss
    ]
    .map(std::borrow::ToOwned::to_owned);
    fields[11] = user_ticks.to_string();
    fields[12] = system_ticks.to_string();
    fields[14] = nice.to_string();
    fields[15] = threads.to_string();
    fields[17] = start_ticks.to_string();

    format!("{pid} (worker-{index:05}) {}", fields.join(" "))
}

/// Build a `/proc/{pid}/status` body with the `VmRSS:` line the memory parser
/// keys on, plus enough surrounding lines to look realistic.
#[cfg(target_os = "linux")]
fn synthetic_proc_status_text(index: usize) -> String {
    let rss_kib = ((index * 65_537) % 4_000_000) as u64;
    format!(
        "Name:\tworker-{index:05}\n\
         Umask:\t0022\n\
         State:\tR\n\
         Tgid:\t{pid}\n\
         Ngid:\t0\n\
         Pid:\t{pid}\n\
         PPid:\t0\n\
         VmSize:\t{vsz} kB\n\
         VmRSS:\t{rss_kib} kB\n\
         VmData:\t{vsz} kB\n\
         Threads:\t{threads}\n",
        pid = index as u32 + 1,
        vsz = rss_kib + 4096,
        rss_kib = rss_kib,
        threads = 1 + (index % 8),
    )
}

/// Build a `/proc/{pid}/io` body with `read_bytes` + `write_bytes`, the two
/// fields the parser extracts.
#[cfg(target_os = "linux")]
fn synthetic_proc_io_text(index: usize) -> String {
    let read = (index * 17) as u64;
    let write = (index * 19) as u64;
    format!(
        "rchar:\t{read}\n\
         wchar:\t{write}\n\
         syscr:\t10\n\
         syscw:\t5\n\
         read_bytes:\t{read}\n\
         write_bytes:\t{write}\n\
         cancelled_write_bytes:\t0\n"
    )
}

// ─── Collection-loop gate: full per-tick collect + correlated ingestion ─────
//
// P1-TEST-03: the loop that actually owns the UI's 200ms cadence is (a) the
// three proc parsers per process and (b) the correlated host/CPU/memory
// ingestion into the bounded history store. This gate times both together in
// wall clock and in process CPU time, and verifies the bounded histories do
// not retain memory over time. `HeapRb` pre-allocates at construction, so any
// retained-bytes growth across ticks is a leak or an unbounded history.

/// Retained bytes (allocated minus freed) tracked by [`CountingAllocator`].
/// Integration-test binaries are per-file crates and nextest isolates each
/// test in its own process, so this global applies only to this gate binary.
#[cfg(target_os = "linux")]
static RETAINED_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Minimal bookkeeping wrapper over the system allocator. Layout/alignment
/// invariants are delegated untouched; only relaxed counter updates are added,
/// so timed allocation behavior stays faithful to the real allocator.
#[cfg(target_os = "linux")]
struct CountingAllocator;

// SAFETY: every operation is forwarded to the system allocator with the same
// `Layout`; the wrapper never changes the addresses or sizes it hands out or
// frees, so its bookkeeping cannot corrupt the heap.
#[cfg(target_os = "linux")]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            RETAINED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        RETAINED_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            RETAINED_BYTES.fetch_add(new_size.saturating_sub(layout.size()), Ordering::Relaxed);
        }
        new_ptr
    }
}

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[cfg(target_os = "linux")]
const COLLECT_TICK_PROCESSES: usize = 1_000;
#[cfg(target_os = "linux")]
const COLLECT_TICKS: usize = 100;
/// Per-tick wall-clock ceiling: the real cadence is 200ms; this allows ~1.5x
/// in debug with heavy scheduler jitter while a quadratic collect path (the
/// 10x-process blowup this gate guards) lands far above it.
#[cfg(target_os = "linux")]
const COLLECT_TICK_WALL_LIMIT: Duration = Duration::from_millis(300);
/// Total process-CPU ceiling for all ticks, in `/proc/self/stat` clock ticks.
/// ~100Hz ticks; 4,000 ticks ≈ 40s of CPU — order-of-magnitude headroom over
/// the expected sub-second measurement, far below a quadratic blowup.
#[cfg(target_os = "linux")]
const COLLECT_CPU_TICK_LIMIT: u64 = 4_000;
/// Retained-bytes growth ceiling across all ticks. Steady state must be flat
/// (ring buffers pre-allocate); 4 MiB absorbs allocator-warmup noise and a
/// small one-time cost without hiding a per-tick leak (100 × 40 KiB = 4 MiB).
#[cfg(target_os = "linux")]
const COLLECT_RETAINED_GROWTH_LIMIT: usize = 4 * 1024 * 1024;

/// Process CPU time (user + system clock ticks) of the test process, read from
/// `/proc/self/stat` with the same parser the Linux collector uses.
#[cfg(target_os = "linux")]
fn self_cpu_ticks() -> u64 {
    let text = std::fs::read_to_string("/proc/self/stat").expect("test process stat is readable");
    let fields = taskmanager_platform_linux::parse_proc_stat(&text).expect("self stat parses");
    fields.cpu_ticks_total()
}

#[cfg(target_os = "linux")]
struct ProcFixtures {
    stat: Vec<String>,
    status: Vec<String>,
    io: Vec<String>,
}

/// One full collection tick: parse the three proc files for every process,
/// fold them into typed CPU/memory observations, and push them through the
/// correlated ingestion capability exactly as the native collector does.
#[cfg(target_os = "linux")]
fn simulate_collect_tick(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    revision: u64,
    fixtures: &ProcFixtures,
) {
    let mut cpu_total: u64 = 0;
    let mut memory_rss_kib: u64 = 0;
    for index in 0..fixtures.stat.len() {
        let stat = taskmanager_platform_linux::parse_proc_stat(&fixtures.stat[index])
            .expect("synthetic stat parses");
        let rss_kib = taskmanager_platform_linux::parse_proc_status_memory(&fixtures.status[index])
            .expect("synthetic status parses");
        let io = taskmanager_platform_linux::parse_proc_io(&fixtures.io[index]);
        let _ = io.read_bytes.expect("synthetic io reads");
        let _ = io.write_bytes.expect("synthetic io writes");
        cpu_total = cpu_total.saturating_add(stat.cpu_ticks_total());
        memory_rss_kib = memory_rss_kib.saturating_add(rss_kib);
    }
    let observed_at_ms = revision.saturating_mul(200);
    let stamp = CorrelatedTelemetryStamp::from_accepted_event(revision, observed_at_ms + 10)
        .expect("non-zero revision stamps");
    ingestor
        .ingest_correlated_host(
            stamp,
            &HostRuntimeObservation::current(
                HostRuntimeFacts {
                    uptime_secs: ScalarObservation::available(
                        observed_at_ms / 1000,
                        observed_at_ms,
                    ),
                    processes: ScalarObservation::available(
                        fixtures.stat.len() as u64,
                        observed_at_ms,
                    ),
                    threads: ScalarObservation::available(
                        fixtures.stat.len() as u64 * 4,
                        observed_at_ms,
                    ),
                },
                observed_at_ms,
                Vec::new(),
            ),
        )
        .expect("increasing revisions ingest");
    ingestor
        .ingest_correlated_cpu(
            stamp,
            &CpuTelemetryObservation::current(
                measured_cpu((cpu_total % 100) as f32, Vec::new(), observed_at_ms),
                observed_at_ms,
                Vec::new(),
            ),
        )
        .expect("increasing revisions ingest");
    ingestor
        .ingest_correlated_memory(
            stamp,
            &MemoryTelemetryObservation::current(
                measured_memory(
                    16_u64 * 1024 * 1024 * 1024,
                    memory_rss_kib.saturating_mul(1024),
                    observed_at_ms,
                ),
                observed_at_ms,
                Vec::new(),
            ),
        )
        .expect("increasing revisions ingest");
}

/// COLLECT-loop gate: time the full per-tick pipeline over 1k synthetic
/// processes × 100 ticks. Asserts wall cost per tick, process-CPU time, and
/// that the bounded histories retain no memory across ticks.
#[cfg(target_os = "linux")]
#[test]
fn collection_loop_cpu_time_and_retained_memory_stay_bounded() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(600);
    let fixtures = ProcFixtures {
        stat: (0..COLLECT_TICK_PROCESSES)
            .map(synthetic_proc_stat_text)
            .collect(),
        status: (0..COLLECT_TICK_PROCESSES)
            .map(synthetic_proc_status_text)
            .collect(),
        io: (0..COLLECT_TICK_PROCESSES)
            .map(synthetic_proc_io_text)
            .collect(),
    };

    // Warm-up ticks amortize allocator first-touch; revisions keep increasing.
    for revision in 1..=3_u64 {
        simulate_collect_tick(&ingestor, revision, &fixtures);
    }

    let retained_before = RETAINED_BYTES.load(Ordering::Relaxed);
    let cpu_before = self_cpu_ticks();
    let wall_started = Instant::now();
    for revision in 4..=(COLLECT_TICKS as u64 + 3) {
        simulate_collect_tick(&ingestor, revision, &fixtures);
    }
    let wall = wall_started.elapsed();
    let cpu_ticks = self_cpu_ticks().saturating_sub(cpu_before);
    let retained_growth = RETAINED_BYTES
        .load(Ordering::Relaxed)
        .saturating_sub(retained_before);

    // The gate must have timed real work: every tick ingested one observation.
    assert_eq!(
        store.system_history.cpu_usage().samples().len(),
        COLLECT_TICKS + 3,
        "every simulated tick must land in the correlated history"
    );

    let wall_per_tick = wall / COLLECT_TICKS as u32;
    eprintln!(
        "collection loop over {COLLECT_TICK_PROCESSES} procs × {COLLECT_TICKS} ticks: \
         wall/tick {wall_per_tick:?} (limit {COLLECT_TICK_WALL_LIMIT:?}), \
         process CPU {cpu_ticks} ticks (limit {COLLECT_CPU_TICK_LIMIT}), \
         retained growth {retained_growth} B (limit {COLLECT_RETAINED_GROWTH_LIMIT} B)"
    );
    assert!(
        wall_per_tick <= COLLECT_TICK_WALL_LIMIT,
        "collection loop over {COLLECT_TICK_PROCESSES} processes averaged {wall_per_tick:?} per \
         tick, exceeding the {COLLECT_TICK_WALL_LIMIT:?} ceiling. Assumption: a modern x86_64 \
         developer laptop or CI runner in debug mode."
    );
    assert!(
        cpu_ticks <= COLLECT_CPU_TICK_LIMIT,
        "collection loop consumed {cpu_ticks} process-CPU ticks over {COLLECT_TICKS} ticks, \
         exceeding the {COLLECT_CPU_TICK_LIMIT} ceiling (~40s of CPU at 100Hz)"
    );
    assert!(
        retained_growth <= COLLECT_RETAINED_GROWTH_LIMIT,
        "collection loop retained {retained_growth} B over {COLLECT_TICKS} ticks, exceeding the \
         {COLLECT_RETAINED_GROWTH_LIMIT} B ceiling — bounded histories must pre-allocate and stay \
         flat (leak or unbounded history suspected)"
    );
}
