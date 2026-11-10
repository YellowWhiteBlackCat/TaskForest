//! Long-dwell soak gate: retention, handle, and materialization stability.
//!
//! The product is a 7×24 background monitor, so the failure mode this gate
//! owns is not "slow" but "grows": a history ring that stops evicting, a
//! collector tick that leaks a file descriptor or a socket every cycle, an
//! asset cache that never releases. Budget gates (`tests/performance.rs`)
//! cannot see any of those — they measure cost, not drift — so this suite
//! drives real process collection at the application's high-frequency cadence
//! and asserts that the retained state stays flat.
//!
//! Cadence knobs (the default run is bounded so the standard gate stays
//! short; a real soak raises the tick count):
//!   TM_SOAK_TICKS     settled refresh ticks to run (default 20)
//!   TM_SOAK_TICK_MS   cadence in milliseconds (default 100)
//! A one-hour soak on a 100 ms cadence is `TM_SOAK_TICKS=36000`.

// ADR-056: the workspace lint table denies `unsafe_code`, and a process-global
// `#[global_allocator]` cannot be written without `unsafe impl GlobalAlloc`.
// The exemption is scoped to this standalone integration-test binary (never
// part of the `taskmanager-gates` library, never shipped) and to Linux, the
// only platform whose arm installs the counting allocator.
#![cfg_attr(all(test, target_os = "linux"), allow(unsafe_code))]

use std::time::{Duration, Instant};

use taskforest_contract::{EventPortError, SubmissionErrorKind};
use taskforest_core::core::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, ScalarObservation,
    ScalarObservationGroup,
};
use taskmanager_application::{PlatformClient, RefreshRequest};
// Consumed only by the Linux-gated materialization arm below; importing them
// unconditionally would be an unused-import error on Windows and macOS, where
// the counting allocator that measures them does not exist.
#[cfg(target_os = "linux")]
use taskmanager_assets::{all_asset_paths, asset_bytes, ui_icon_rgba};
use taskmanager_platform_conformance::smoke_budget::{DRAIN_DEADLINE, DRAIN_POLL};
use taskmanager_platform_conformance::{
    batch_event_count, collect_process_rows, drain_until_process_rows,
};
use taskmanager_platform_native::NativePlatformRuntime;
use taskmanager_telemetry_store::{CorrelatedTelemetryStamp, HistoryRetention, TelemetryStore};

#[cfg(target_os = "linux")]
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicUsize, Ordering};

/// The cadence the product's own collection loop owns. A soak that does not
/// drive the loop at this rate would not exercise the product's scheduling.
/// It is a target, not a promise: on a loaded host a full refresh takes longer
/// than one cadence, and the harness then paces itself to the work it can do.
const PRODUCT_TICK_MS: u64 = 100;

fn env_number(key: &str, default: u64) -> u64 {
    match std::env::var(key) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{key} must be a positive integer, got '{raw}'")),
        Err(_) => default,
    }
}

fn soak_ticks() -> u64 {
    env_number("TM_SOAK_TICKS", 20).max(3)
}

fn soak_tick_ms() -> u64 {
    env_number("TM_SOAK_TICK_MS", PRODUCT_TICK_MS).max(1)
}

/// How long the event port must stay quiet before a tick counts as settled,
/// and how long that wait may take before the tick proceeds anyway.
const SOAK_SETTLE: Duration = Duration::from_millis(20);
const SOAK_SETTLE_DEADLINE: Duration = Duration::from_secs(2);

/// Drain the event port until it has been quiet for `quiet`, or until
/// `deadline` passes. Returns how many events arrived while waiting.
///
/// A soak tick may only start once the previous process collection has settled:
/// re-submitting while the process lane is still running piles up collectors,
/// which inflates descriptor counts for reasons that have nothing to do with a
/// leak. The short quiet interval is below the product cadence, so the loop can
/// still sustain its 100ms schedule when the host finishes each process sample
/// within budget.
fn drain_until_quiet(
    client: &mut PlatformClient,
    quiet: Duration,
    deadline: Duration,
    poll: Duration,
) -> Result<usize, EventPortError> {
    let started = Instant::now();
    let mut last_event = Instant::now();
    let mut total = 0_usize;
    loop {
        let batch = client.try_drain()?;
        let arrived = batch_event_count(&batch);
        if arrived > 0 {
            total += arrived;
            last_event = Instant::now();
        }
        if last_event.elapsed() >= quiet || started.elapsed() >= deadline {
            return Ok(total);
        }
        std::thread::sleep(poll);
    }
}

fn measured_cpu(usage: f32, at_ms: u64) -> CpuMetrics {
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(usage, at_ms),
        core_usage_group: ScalarObservationGroup::default(),
        ..Default::default()
    })
}

/// Resident set size of this process in KiB, or `None` where the platform has
/// no `/proc` (the caller then records an explicit, non-fabricated gap).
#[cfg(target_os = "linux")]
fn self_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn self_rss_kib() -> Option<u64> {
    None
}

/// Open file-descriptor count of this process, or `None` without `/proc`.
/// Every `/proc` read, socket, and mmap the collector opens must be released
/// by RAII; a per-tick leak shows up here as a monotonic climb.
#[cfg(target_os = "linux")]
fn self_fd_count() -> Option<usize> {
    std::fs::read_dir("/proc/self/fd").ok().map(|dir| {
        // The iterator itself holds a descriptor for /proc/self/fd, so the
        // count is one higher than the "real" number; only the delta across
        // ticks matters, and it is constant.
        dir.count()
    })
}

#[cfg(not(target_os = "linux"))]
fn self_fd_count() -> Option<usize> {
    None
}

/// Open descriptors whose target is a `/proc/<pid>/…` path belonging to some
/// **other** process.
///
/// A collector that caches one handle per observed process makes this number
/// track the host's process population, which is a resource decision, not a
/// leak that drift can find: the count is already at its ceiling on the very
/// first settled tick, so first-vs-last comparison sees a flat line. Sampling
/// it separately is what lets the gate assert an absolute bound instead.
#[cfg(target_os = "linux")]
fn retained_procfs_handles() -> Option<usize> {
    let own = std::process::id();
    let directory = std::fs::read_dir("/proc/self/fd").ok()?;
    let mut held = 0_usize;
    for entry in directory.flatten() {
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let target = target.to_string_lossy();
        let Some(rest) = target.strip_prefix("/proc/") else {
            continue;
        };
        let pid = rest.split('/').next().unwrap_or_default();
        match pid.parse::<u32>() {
            Ok(pid) if pid != own => held += 1,
            _ => {}
        }
    }
    Some(held)
}

#[cfg(not(target_os = "linux"))]
fn retained_procfs_handles() -> Option<usize> {
    None
}

/// The soft `Max open files` limit this process currently holds, as published
/// by the kernel (`"unlimited"` is a legal value, so the token is compared
/// rather than parsed).
///
/// A dependency can raise the soft limit to the hard one and keep half of the
/// new budget for itself. The application is unprivileged by default and never
/// renegotiates its own resource limits, so a collector that moves this number
/// is taking an unobservable process-global action, not reporting telemetry.
#[cfg(target_os = "linux")]
fn max_open_files_soft_limit() -> Option<String> {
    let limits = std::fs::read_to_string("/proc/self/limits").ok()?;
    let row = limits.lines().find(|line| {
        let mut cells = line.split_whitespace();
        cells.next() == Some("Max") && cells.next() == Some("open")
    })?;
    // Cells: "Max" "open" "files" <soft> <hard> <units>.
    Some(row.split_whitespace().nth(3)?.to_owned())
}

/// How many descriptors the collector may hold for reasons that are not a
/// per-process handle: the port's own pipes, the `/proc` reads in flight, and
/// the harness. Measured on a host with ~700 processes, the fixed cost of the
/// whole product is four.
const RETAINED_HANDLE_CEILING: usize = 8;

/// The same resident-memory shape the shipped soak-stability owner judges:
/// a delayed one-time working set may settle into a plateau, but a series
/// still setting new maxima, swinging beyond the tail's noise band, or
/// drifting over its own horizon is a leak.
const RSS_TAIL_MIN_SAMPLES: usize = 10;
const RSS_TAIL_DIVISOR: usize = 3;
const RSS_TAIL_RANGE_PCT: f64 = 12.0;
const RSS_TAIL_DRIFT_PCT: f64 = 25.0;
const RSS_GROWTH_PCT: f64 = 25.0;

fn rss_tail_is_plateau(settled: &[u64]) -> bool {
    if settled.len() < 3 {
        return false;
    }
    let tail_size = (settled.len() / RSS_TAIL_DIVISOR)
        .max(RSS_TAIL_MIN_SAMPLES)
        .min(settled.len());
    let tail = &settled[settled.len() - tail_size..];
    let prefix = &settled[..settled.len() - tail_size];
    let mut sorted = tail.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2] as f64;
    if median == 0.0 {
        return false;
    }
    let range_pct = (*tail.iter().max().expect("tail") - *tail.iter().min().expect("tail")) as f64
        / median
        * 100.0;
    let x_mean = (tail.len() - 1) as f64 / 2.0;
    let y_mean = tail.iter().map(|value| *value as f64).sum::<f64>() / tail.len() as f64;
    let denominator = (0..tail.len())
        .map(|index| (index as f64 - x_mean).powi(2))
        .sum::<f64>();
    let slope = (0..tail.len())
        .map(|index| (index as f64 - x_mean) * (tail[index] as f64 - y_mean))
        .sum::<f64>()
        / denominator;
    let drift_pct = slope * (tail.len() - 1) as f64 / median * 100.0;
    let mut high = tail[0];
    for value in prefix.iter().copied().chain(tail.iter().copied()) {
        high = high.max(value);
    }
    let prior_high = prefix
        .iter()
        .copied()
        .chain(std::iter::once(tail[0]))
        .max()
        .unwrap_or(tail[0]);
    let mut observed_high = prior_high;
    let mut new_maxima = 0_usize;
    for (index, value) in tail.iter().copied().enumerate() {
        if index > 0 && value > observed_high {
            new_maxima += 1;
        }
        observed_high = observed_high.max(value);
    }
    new_maxima == 0
        && range_pct <= RSS_TAIL_RANGE_PCT
        && drift_pct.abs() <= RSS_TAIL_DRIFT_PCT
        && high <= (prior_high as f64 * 1.01) as u64
}

#[test]
fn rss_shape_owner_accepts_a_settled_working_set_but_rejects_a_live_leak() {
    let mut settled = vec![12_000];
    settled.extend((1..25).map(|index| 12_000 + index * 1_000));
    settled.extend([
        35_000, 34_200, 34_800, 34_400, 34_600, 34_500, 34_300, 34_700, 34_400, 34_600,
    ]);
    assert!(rss_tail_is_plateau(&settled));

    let mut leaking = vec![12_000];
    leaking.extend((1..40).map(|index| 12_000 + index * 1_000));
    assert!(!rss_tail_is_plateau(&leaking));
}

/// Arm A — the bounded history ring must evict, not grow.
///
/// A soak run appends far more samples than any ring's capacity. If eviction
/// regressed, the oldest samples would be retained forever: memory would climb
/// across the run and the retained window would exceed the capacity the
/// retention policy promises. This arm is deterministic and platform-neutral,
/// so it proves the eviction rule even where `/proc` is unavailable.
#[test]
fn history_ring_evicts_oldest_samples_and_stays_at_capacity() {
    const CAPACITY: usize = 8;
    const INGESTED: u64 = 40;

    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(CAPACITY));

    for revision in 1..=INGESTED {
        let at_ms = revision * PRODUCT_TICK_MS;
        let stamp = CorrelatedTelemetryStamp::from_accepted_event(revision, at_ms)
            .expect("non-zero revisions stamp");
        ingestor
            .ingest_correlated_cpu(
                stamp,
                &CpuTelemetryObservation::current(
                    measured_cpu(revision as f32, at_ms),
                    at_ms,
                    Vec::new(),
                ),
            )
            .expect("strictly increasing revisions ingest");
    }

    let history = store.system_history.cpu_usage();
    let samples = history.samples();
    assert_eq!(
        samples.len(),
        CAPACITY,
        "the ring must retain exactly its capacity after {INGESTED} samples, not grow past it"
    );
    assert_eq!(
        history.watermark().1,
        Some(INGESTED),
        "the retained window must end at the newest ingested revision"
    );
    let oldest_retained = samples
        .first()
        .expect("a full ring has a first sample")
        .stamp
        .revision();
    assert_eq!(
        oldest_retained,
        INGESTED - CAPACITY as u64 + 1,
        "the oldest retained sample must be the newest capacity-sized window; older samples must be evicted"
    );
}

/// Arm B — the real refresh loop at the product cadence, watching handles.
///
/// Every tick spawns the native composition edge's real collectors, drains
/// typed outcomes, and records the process's descriptor count and RSS. The
/// assertions are drift assertions: a collector that leaks one descriptor per
/// tick, or retains per-tick state, fails here rather than in production.
#[test]
fn high_frequency_soak_keeps_descriptors_and_memory_stable() {
    let ticks = soak_ticks();
    let tick = Duration::from_millis(soak_tick_ms());
    let mut client =
        PlatformClient::new(NativePlatformRuntime::spawn().expect("native runtime must compose"));

    let mut rows_seen = 0_usize;
    let mut fd_samples: Vec<usize> = Vec::new();
    let mut rss_samples: Vec<u64> = Vec::new();
    let mut retained_samples: Vec<usize> = Vec::new();
    let mut rows_samples: Vec<usize> = Vec::new();

    let started = Instant::now();
    for revision in 1..=ticks {
        let tick_started = Instant::now();
        let submissions = client.request_refresh(RefreshRequest::Processes, revision);
        // `Busy` is the product's normal steady state: a slow lane is still
        // working when the next cadence arrives, and the refresh loop simply
        // re-issues. Any other outcome is a real submission failure.
        let hard_failures: Vec<String> = submissions
            .iter()
            .filter_map(|result| result.as_ref().err())
            .filter(|err| err.kind != SubmissionErrorKind::Busy)
            .map(|err| format!("{err:?}"))
            .collect();
        assert!(
            hard_failures.is_empty(),
            "observation lanes failed hard on tick {revision}: {hard_failures:?}"
        );
        assert!(
            submissions.iter().any(|result| result.is_ok()),
            "tick {revision} submitted no work at all: every lane reported Busy"
        );
        let drain = drain_until_process_rows(&mut client, DRAIN_DEADLINE, DRAIN_POLL)
            .unwrap_or_else(|err| {
                panic!("the event port must stay live on tick {revision}: {err:?}")
            });
        let rows = collect_process_rows(&drain).len();
        rows_seen += rows;
        rows_samples.push(rows);

        // Let the tick settle before sampling and before the next submission,
        // so no tick is measured while another refresh is still running.
        drain_until_quiet(&mut client, SOAK_SETTLE, SOAK_SETTLE_DEADLINE, DRAIN_POLL)
            .unwrap_or_else(|err| {
                panic!("the event port must stay live on tick {revision}: {err:?}")
            });

        // Pace the loop: sleeping is what makes this a soak (many cycles spread
        // over wall-clock time) instead of a spin. When a refresh already
        // overran the cadence there is nothing left to wait for, and the host's
        // own speed sets the rate.
        if let Some(remaining) = tick.checked_sub(tick_started.elapsed()) {
            std::thread::sleep(remaining);
        }
        if let Some(fds) = self_fd_count() {
            fd_samples.push(fds);
        }
        if let Some(rss) = self_rss_kib() {
            rss_samples.push(rss);
        }
        if let Some(retained) = retained_procfs_handles() {
            retained_samples.push(retained);
        }
    }
    let elapsed = started.elapsed();
    if let (
        Some(first_fd),
        Some(last_fd),
        Some(first_rss),
        Some(last_rss),
        Some(first_retained),
        Some(last_retained),
    ) = (
        fd_samples.first().copied(),
        fd_samples.last().copied(),
        rss_samples.first().copied(),
        rss_samples.last().copied(),
        retained_samples.first().copied(),
        retained_samples.last().copied(),
    ) {
        eprintln!(
            "soak report: {ticks} process ticks × {tick:?} in {elapsed:?}, \
             last-tick rows={}..{}, descriptors {first_fd}->{last_fd}, \
             retained /proc handles {first_retained}->{last_retained}, \
             RSS {first_rss}->{last_rss} KiB",
            rows_samples.first().copied().unwrap_or_default(),
            rows_samples.last().copied().unwrap_or_default(),
        );
    } else {
        eprintln!("soak report: {ticks} process ticks × {tick:?} in {elapsed:?}, rows={rows_seen}");
    }

    assert!(
        rows_seen > 0,
        "the soak must observe at least one process row"
    );
    // The harness must sustain at least the configured cadence. A slow process
    // collection may overrun it, but must never turn the soak into a spin.
    assert!(
        elapsed >= tick.mul_f64(ticks as f64),
        "the soak cycled {ticks} process ticks in {elapsed:?}, faster than the configured \
         {tick:?} cadence allows — the loop did not pace itself"
    );

    // A soak that sampled nothing would pass vacuously. On Linux the sampler
    // must have produced a reading for every tick; on other hosts the gap is
    // reported below instead.
    #[cfg(target_os = "linux")]
    assert_eq!(
        fd_samples.len(),
        ticks as usize,
        "the descriptor sampler must produce one reading per tick on Linux"
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        rss_samples.len(),
        ticks as usize,
        "the RSS sampler must produce one reading per tick on Linux"
    );

    if fd_samples.len() >= 3 {
        // Drop the warm-up sample for the same reason as RSS below: the first
        // tick is where the runtime opens its long-lived descriptors.
        let stable = &fd_samples[1..];
        let baseline = stable[0];
        let last = stable[stable.len() - 1];
        let peak = *stable.iter().max().expect("checked length");
        // Drift alone cannot see a handle that is acquired once per process and
        // held forever: that ceiling is already reached on the first settled
        // tick, so the line is flat. The absolute bounds below are the teeth;
        // this assertion still owns the monotonic per-tick leak.
        assert!(
            last <= baseline + 4,
            "descriptor count climbed from {baseline} to {last} over {ticks} settled ticks \
             (peak {peak}) while the inventory observed {}..{} processes per tick — \
             a per-tick descriptor leak",
            rows_samples.iter().min().copied().unwrap_or_default(),
            rows_samples.iter().max().copied().unwrap_or_default(),
        );
    }
    // The absolute handle bound. Reported against the observed row count so a
    // regression reads as "one descriptor per process", not as unexplained
    // growth, and so a host with more processes can never excuse a larger
    // ceiling.
    if !retained_samples.is_empty() {
        let peak_retained = *retained_samples.iter().max().expect("checked non-empty");
        let peak_rows = rows_samples.iter().max().copied().unwrap_or_default();
        assert!(
            peak_retained <= RETAINED_HANDLE_CEILING,
            "process collection held {peak_retained} descriptors into other processes' /proc \
             entries while observing up to {peak_rows} processes (ceiling \
             {RETAINED_HANDLE_CEILING}) — the collector retains one handle per process instead \
             of opening and closing per read"
        );
    }
    if fd_samples.len() >= 3 && !retained_samples.is_empty() {
        // A flat total with a rising retained count would hide the structural
        // cost inside the drift budget, so the retained series must be flat too.
        let first_retained = retained_samples[0];
        let last_retained = retained_samples[retained_samples.len() - 1];
        assert_eq!(
            first_retained, last_retained,
            "retained cross-process /proc handles moved from {first_retained} to \
             {last_retained} across {ticks} settled ticks"
        );
    }
    if rss_samples.len() >= 3 {
        // Drop the warm-up sample: the first tick still faults in the
        // allocator arenas and the collector's thread stacks.
        let stable = &rss_samples[1..];
        let baseline = stable[0];
        let final_rss = stable[stable.len() - 1];
        let growth_pct = if baseline == 0 {
            0.0
        } else {
            (final_rss as f64 - baseline as f64) / baseline as f64 * 100.0
        };
        assert!(
            growth_pct <= RSS_GROWTH_PCT || rss_tail_is_plateau(stable),
            "resident memory grew {growth_pct:.1}% ({baseline} KiB -> {final_rss} KiB) over \
             {ticks} ticks without a settled tail, exceeding the soak stability threshold"
        );
    }

    // Non-Linux hosts have no /proc: record the gap instead of claiming a
    // handle verdict the host cannot support.
    if self_fd_count().is_none() {
        eprintln!(
            "soak: /proc is unavailable on this host; descriptor and RSS drift were not sampled"
        );
    }
    if retained_samples.is_empty() {
        eprintln!(
            "soak: /proc/self/fd is unavailable on this host; the per-process handle bound was \
             not sampled and is recorded as a gap, not a pass"
        );
    }
}

#[cfg(target_os = "linux")]
static RETAINED_BYTES: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "linux")]
struct CountingAllocator;

// SAFETY: every operation forwards to the system allocator with the same
// `Layout` and returns the same pointer, so the wrapper cannot change
// allocation behavior or corrupt the heap.
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
static SOAK_ALLOCATOR: CountingAllocator = CountingAllocator;

/// Arm C — repeated glyph/asset materialization must not accumulate.
///
/// Frontends re-materialize the shared icon set every time a row repaints, so
/// a lazily-populated cache behind `asset_bytes`/`ui_icon_rgba` would grow with
/// uptime. The registry is expected to serve borrowed static bytes; this arm
/// pins that by measuring retained allocation across many full-registry passes.
#[cfg(target_os = "linux")]
#[test]
fn repeated_glyph_materialization_retains_no_allocation() {
    const PASSES: usize = 500;
    /// One pass allocates nothing by construction; the ceiling only absorbs
    /// first-touch noise from the harness itself.
    const GROWTH_LIMIT: usize = 64 * 1024;

    let paths: Vec<&'static str> = all_asset_paths().collect();
    assert!(
        !paths.is_empty(),
        "the shared asset registry must not be empty"
    );

    for path in &paths {
        let _ = asset_bytes(path);
        let _ = ui_icon_rgba(path);
    }
    let before = RETAINED_BYTES.load(Ordering::Relaxed);
    for _ in 0..PASSES {
        for path in &paths {
            let bytes = asset_bytes(path);
            let rgba = ui_icon_rgba(path);
            assert!(
                bytes.is_some() || rgba.is_some(),
                "every advertised asset path must resolve to embedded bytes or a bitmap"
            );
        }
    }
    let growth = RETAINED_BYTES
        .load(Ordering::Relaxed)
        .saturating_sub(before);
    eprintln!(
        "soak: {PASSES} materialization passes over {} assets retained {growth} B (limit {GROWTH_LIMIT} B)",
        paths.len()
    );
    assert!(
        growth <= GROWTH_LIMIT,
        "repeated glyph/asset materialization retained {growth} B across {PASSES} passes, \
         exceeding the {GROWTH_LIMIT} B ceiling — an unbounded materialization cache is suspected"
    );
}

/// Arm D — the structural handle bound, measured against the host's process
/// population rather than against time.
///
/// The soak's drift assertions compare the first and last settled tick. A
/// collector that keeps one open `/proc/<pid>/stat` handle per observed process
/// is already at its ceiling on the first settled tick, so drift reads that
/// ceiling as a flat, healthy line: on a host with ~700 processes the run
/// reported `descriptors 727->751` and the structural cost of 736 parked
/// descriptors was invisible. This arm states the invariant absolutely: the
/// number of descriptors the product holds is a fixed cost, and the product
/// never renegotiates its own resource limits to make room for a cache.
#[cfg(target_os = "linux")]
#[test]
fn process_collection_holds_no_per_process_handles_and_never_moves_the_budget() {
    // Read the limit before any collection exists, so a dependency that raises
    // the soft limit on first use cannot be measured against its own change.
    let limit_before = max_open_files_soft_limit().expect("/proc/self/limits must be readable");
    let mut client =
        PlatformClient::new(NativePlatformRuntime::spawn().expect("native runtime must compose"));

    let mut peak_retained = 0_usize;
    let mut peak_rows = 0_usize;
    let mut settled_ticks = 0_usize;
    for revision in 1..=6_u64 {
        let _ = client.request_refresh(RefreshRequest::Processes, revision);
        let drain = drain_until_process_rows(&mut client, DRAIN_DEADLINE, DRAIN_POLL)
            .expect("the event port must stay live");
        peak_rows = peak_rows.max(collect_process_rows(&drain).len());
        drain_until_quiet(&mut client, SOAK_SETTLE, SOAK_SETTLE_DEADLINE, DRAIN_POLL)
            .expect("the event port must stay live");
        peak_retained =
            peak_retained.max(retained_procfs_handles().expect("/proc/self/fd must be readable"));
        settled_ticks += 1;
    }

    assert_eq!(
        settled_ticks, 6,
        "the arm must observe a settled sample per tick"
    );
    assert!(
        peak_rows > 20,
        "the sample must cover enough processes for the bound to mean anything (observed          {peak_rows})"
    );
    assert!(
        peak_retained <= RETAINED_HANDLE_CEILING,
        "after {settled_ticks} settled ticks over up to {peak_rows} observed processes, the          product still held {peak_retained} descriptors pointing into other processes' /proc          entries (ceiling {RETAINED_HANDLE_CEILING}) — the descriptor table tracks the host's          process population instead of the product's own fixed cost"
    );
    let limit_after = max_open_files_soft_limit().expect("/proc/self/limits must be readable");
    assert_eq!(
        limit_after, limit_before,
        "process collection moved the process's soft `Max open files` limit from          {limit_before} to {limit_after}; an unprivileged monitor never widens its own          resource limits as a side effect of reading telemetry"
    );
}
