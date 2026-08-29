//! TUI-006 per-frame ALLOCATION budget for the Applications render path.
//!
//! This is the deterministic, primary half of the perf-budget contract: the
//! wall-clock numbers are only a smoke check (see the library-registered
//! `tests/gui/perf_budget_tests.rs` for the structural and timing halves).
//!
//! WHY THIS IS A STANDALONE INTEGRATION-TEST BINARY: counting allocations
//! needs a process-global `#[global_allocator]` with an
//! `unsafe impl GlobalAlloc`. The `taskmanager-tui` library forbids unsafe
//! crate-wide, and every `tests/gui/*.rs` file is compiled INSIDE that
//! library through `#[cfg(test)] #[path = ...]` module registration, so the
//! allocator cannot live there. This file is its own test crate: the unsafe
//! counting allocator is confined to this test binary, links only the
//! library's public API, and never ships.
//!
//! Measurement protocol (identical for every budget below): build the app and
//! render one warm-up frame (first-paint costs — shell filter/sort memo,
//! visual-row-count build, ratatui buffer allocation — are intentionally
//! excluded), then reset the counters, render exactly one steady-state frame
//! and snapshot. Budgets are measured value x >=5x headroom, recorded with
//! the measurement date, so a loaded CI machine cannot flake them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::{CpuMetrics, MemoryMetrics, SystemSnapshot};
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_tui::{TuiApp, TuiTheme, render};

/// The counting allocator: forwards to [`System`] and records the count and
/// byte size of every successful allocation. Reallocations count as one
/// allocation of the new size (the dealloc side is not measured — budgets
/// bound per-frame allocation PRESSURE, not net memory growth).
struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }
}

#[global_allocator]
static COUNTING: CountingAllocator = CountingAllocator;

/// One allocator snapshot over a measured render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AllocSnapshot {
    allocations: u64,
    bytes: u64,
}

fn reset_counters() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
}

fn snapshot_counters() -> AllocSnapshot {
    AllocSnapshot {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

/// The fixed fixture observation timestamp (the shared demo-fixture instant).
const FIXTURE_TIMESTAMP_MS: u64 = 1_785_292_800_000;

/// Scale of the "10k" fixture: 10_000 visible processes.
const FIXTURE_10K: (usize, usize, usize, usize) = (1_000, 5, 2_500, 1_500);
/// Scale of the "50k" fixture: 50_000 visible processes.
const FIXTURE_50K: (usize, usize, usize, usize) = (5_000, 5, 12_500, 7_500);

/// Deterministic process-tree fixture, same shape as
/// `tests/gui/perf_budget_tests.rs` (kept local: this binary links only the
/// public API and cannot reach the library's test modules).
struct TreeFixture {
    processes: Vec<ProcessItem>,
}

fn base_process(pid: u32, name: String, cpu: f32) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name)
        .current_cpu_percentage(cpu)
        .current_memory_bytes(1024 * 1024)
        .build()
}

fn identified(pid: u32, name: String, cpu: f32, parent: Option<u32>) -> ProcessItem {
    let mut item = base_process(pid, name.clone(), cpu);
    let identity = ProcessApplicationIdentity::new(name.clone(), name, None)
        .expect("fixture identity carries real values");
    item.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    item.parent_pid = parent;
    item
}

fn background(pid: u32, name: String, cpu: f32) -> ProcessItem {
    let mut item = base_process(pid, name, cpu);
    item.apply_application_identity(
        ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10),
    );
    item
}

fn unknown(pid: u32, name: String, cpu: f32) -> ProcessItem {
    base_process(pid, name, cpu)
}

/// `apps` identified application roots with 5 identified children each,
/// 2.5x `apps` background roots and 1.5x `apps` unknown-identity processes —
/// 10x `apps` processes in total.
fn tree_fixture(apps: usize) -> TreeFixture {
    let (children_per_app, background_count, uncategorized) = (5usize, apps * 5 / 2, apps * 3 / 2);
    let mut processes =
        Vec::with_capacity(apps * (1 + children_per_app) + background_count + uncategorized);
    let mut next_child_pid = 2_000_000u32;
    for index in 0..apps {
        let parent = 1_000 + index as u32;
        processes.push(identified(
            parent,
            format!("prc-app-{index:05}"),
            (apps - index) as f32,
            None,
        ));
        for child in 0..children_per_app {
            processes.push(identified(
                next_child_pid,
                format!("prc-kid-{index:05}-{child:02}"),
                0.5,
                Some(parent),
            ));
            next_child_pid += 1;
        }
    }
    for index in 0..background_count {
        processes.push(background(
            5_000_000 + index as u32,
            format!("prc-bg-{index:05}"),
            0.2,
        ));
    }
    for index in 0..uncategorized {
        processes.push(unknown(
            7_000_000 + index as u32,
            format!("prc-unc-{index:05}"),
            0.1,
        ));
    }
    TreeFixture { processes }
}

/// A minimal but complete telemetry snapshot; without it the shell keeps its
/// first-frame "collecting" gate and the render never reaches the table.
fn minimal_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms: FIXTURE_TIMESTAMP_MS,
        cpu: CpuMetrics::from_observations(Default::default()),
        memory: MemoryMetrics::from_observations(Default::default(), Default::default()),
        disks: Vec::new(),
        networks: Vec::new(),
        gpu: Vec::new(),
        telemetry_sources: Vec::new(),
        provider_states: Vec::new(),
        device_lifecycles: Default::default(),
        uptime_secs: 0,
        processes: 0,
        threads: None,
    }
}

/// Build the Applications-page app through the public composition seam: seed
/// the shell fixture BEFORE wrapping it in the terminal frontend.
fn seeded_app(fixture: &TreeFixture) -> TuiApp {
    let mut shell = ShellApp::new();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(minimal_snapshot()))),
    );
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Processes(Some(fixture.processes.clone())),
    );
    let mut app = TuiApp::from_shell(shell);
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

/// Render one full 120x40 frame through the public render entry point.
fn render_frame(app: &TuiApp) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
}

/// Measure one steady-state frame's allocations (warm-up excluded).
fn measure_steady_state_frame(app: &TuiApp) -> AllocSnapshot {
    render_frame(app);
    reset_counters();
    render_frame(app);
    snapshot_counters()
}

#[test]
fn per_frame_allocation_budget_10k_and_50k() {
    // ── Measured steady-state 120x40 frames (debug build, dev workstation,
    //    2026-08-29, allocator = System + atomic counters), AFTER the
    //    per-frame visible-pointer vector was eliminated (the Applications
    //    render now resolves rows lazily through the shell's memoized
    //    visible-row indices; three consecutive runs reproduced these exact
    //    numbers):
    //      10k processes: 644 allocations / 789,450 bytes
    //      50k processes: 644 allocations / 789,475 bytes
    //    (Intermediate evidence, kept for honesty — after the
    //    revision-keyed CanonicalRowId projection cache, before the lazy
    //    index accessor: 661 allocations, 871,643 vs 1,191,668 bytes — the
    //    +320 KB byte growth was the per-frame O(N) `Vec<&ProcessItem>`
    //    pointer vector this slice removed. Pre-cache evidence: 8,777 /
    //    40,807 allocations.)
    //
    //    PROFILE OF THE REMAINING FLAT COST (same day, same harness): each
    //    `render_frame` call here constructs a FRESH Terminal, which alone
    //    costs 3 allocations / 691,200 bytes (TestBackend buffer + the
    //    Terminal's two 4,800-cell frame buffers at 48 bytes/cell). That is
    //    test-only harness cost — N-independent and absent from the live
    //    loop, which reuses its Terminal across frames. The render-side
    //    remainder is ~641 allocations / ~98 KB, splitting into ~183
    //    allocations / ~26 KB of page-independent chrome (header, footer,
    //    empty/sanitize passes) and ~458 allocations / ~72 KB of
    //    painted-window content (table rows, aggregate headers, details
    //    panel — the formatted strings the ui renderers build for exactly
    //    the bounded window). Supporting measurements: the per-draw theme
    //    rebuild is 0 allocations (const font resolution + typed tokens), a
    //    40-process frame is 668 allocations / ~97 KB (window-bound and
    //    N-independent — slightly MORE than 10k because its whole tree fits
    //    the window), and a second identical draw with an empty render
    //    callback is 0 allocations (ratatui's lazy cell diff emits no
    //    updates for unchanged content).
    let app_10k = seeded_app(&tree_fixture(FIXTURE_10K.0));
    let measured_10k = measure_steady_state_frame(&app_10k);

    let app_50k = seeded_app(&tree_fixture(FIXTURE_50K.0));
    let measured_50k = measure_steady_state_frame(&app_50k);

    // Contract 1: per-frame ALLOCATION COUNT is flat in N — the canonical
    // row-id slice is cached under the presentation key, so a 5x larger list
    // must not add a single allocation to a steady-state frame. A growth here
    // means an O(N) rebuild crept back into the per-frame path.
    assert_eq!(
        measured_50k.allocations, measured_10k.allocations,
        "per-frame allocation count must be independent of N (cached row \
         projection): 10k={measured_10k:?} 50k={measured_50k:?}"
    );

    // Contract 2: per-frame BYTES are flat in N — the last O(N) byte source
    // (the per-frame visible-process pointer vector, ~8 bytes per process)
    // was removed: rows resolve on demand through the memoized indices, so a
    // 5x larger list may add only sub-linear noise to a frame's bytes. The
    // measured 10k→50k delta is 25 bytes; a 1,000-byte tolerance is 40x that
    // while an 8-bytes-per-process vector over the same 40k span (+320,000
    // bytes) still trips it instantly.
    let byte_growth = measured_50k.bytes.saturating_sub(measured_10k.bytes);
    assert!(
        byte_growth <= 1_000,
        "per-frame bytes must be independent of N (lazy indexed row \
         resolution): growth {byte_growth} bytes, 10k={measured_10k:?} \
         50k={measured_50k:?}"
    );

    // Absolute ceilings: 5x the measured steady-state frame (789,475 bytes /
    // 644 allocations), identical for BOTH scales because the frames are
    // N-independent — one flat budget instead of per-N budgets.
    assert!(
        measured_10k.allocations <= 5_000,
        "10k frame made {} allocations, budget is 5,000",
        measured_10k.allocations
    );
    assert!(
        measured_10k.bytes <= 4_000_000,
        "10k frame allocated {} bytes, budget is 4,000,000",
        measured_10k.bytes
    );
    assert!(
        measured_50k.allocations <= 5_000,
        "50k frame made {} allocations, budget is 5,000",
        measured_50k.allocations
    );
    assert!(
        measured_50k.bytes <= 4_000_000,
        "50k frame allocated {} bytes, budget is 4,000,000 (halved from \
         8,000,000: bytes no longer scale with N)",
        measured_50k.bytes
    );
}

#[test]
fn repeated_identical_frames_allocate_identically() {
    let app = seeded_app(&tree_fixture(FIXTURE_10K.0));
    render_frame(&app); // warm-up

    reset_counters();
    render_frame(&app);
    let first = snapshot_counters();
    reset_counters();
    render_frame(&app);
    let second = snapshot_counters();

    // The render path is deterministic: two identical frames against an
    // unchanged projection must exert identical allocation pressure. A drift
    // here means hidden per-frame state (a growing cache, shuffling iteration
    // order) — exactly what this budget exists to catch.
    assert_eq!(first, second, "{first:?} vs {second:?}");
}
