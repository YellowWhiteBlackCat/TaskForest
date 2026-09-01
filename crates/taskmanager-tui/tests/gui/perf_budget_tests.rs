//! TUI-006 bounded-render-cost budget contract for the Applications page.
//!
//! These tests pin the three structural halves of the perf budget:
//!
//! 1. The canonical category-tree row slice ([`super::build_process_rows`])
//!    is O(visible processes): its length is the pure function of the fixture
//!    shape and the expansion/collapse/query inputs asserted here.
//! 2. The full-frame render materializes only the bounded table window, so the
//!    number of painted process rows is identical for a 10k and a 50k process
//!    fixture at the same terminal size — row paint cost never grows with N.
//! 3. The revision/expand/collapse/query-keyed [`TuiApp::visual_row_count`]
//!    cache stays equal to a freshly rebuilt canonical slice after every input
//!    changes (hit and invalidation behavior), and a steady-state 50k frame
//!    stays inside a generous wall-clock smoke budget.
//!
//! The per-frame ALLOCATION budget — the primary, deterministic half of the
//! contract — lives in `tests/perf_budget_alloc_tests.rs`, a standalone
//! integration-test binary: the counting `#[global_allocator]` needs an
//! `unsafe impl GlobalAlloc`, which cannot compile inside this library
//! (`#![forbid(unsafe_code)]` at the crate root).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::process_category_projection::category_expansion_key;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::{CpuMetrics, MemoryMetrics, SystemSnapshot};
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessLiveKey, ProcessMetadataObservation,
};
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::{SortCol, SortDir};

use crate::{TuiApp, TuiTheme, render};

use super::*;

/// The fixed fixture observation timestamp (the shared demo-fixture instant).
/// No test reads the host clock for data — only the wall-clock smoke budgets
/// measure `Instant::now`, and those never assert host values.
const FIXTURE_TIMESTAMP_MS: u64 = 1_785_292_800_000;

/// Scale of the "10k" fixture: 10_000 visible processes.
const FIXTURE_10K: (usize, usize, usize, usize) = (1_000, 5, 2_500, 1_500);
/// Scale of the "50k" fixture: 50_000 visible processes.
const FIXTURE_50K: (usize, usize, usize, usize) = (5_000, 5, 12_500, 7_500);

/// A deterministic process-tree fixture.
///
/// Shape: `apps` identified application roots (each with `children_per_app`
/// identified children), `background` provider-confirmed background roots and
/// `uncategorized` processes whose identity truth is unknown. Every process
/// name carries the unique `prc-` marker so a painted frame's process rows can
/// be counted from the TestBackend text without depending on locale chrome.
struct TreeFixture {
    processes: Vec<ProcessItem>,
    apps: usize,
    children_per_app: usize,
    background: usize,
    uncategorized: usize,
}

impl TreeFixture {
    fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Canonical rows in the DEFAULT TuiApp state: every category bucket
    /// expanded, no `app-tree:` aggregate expanded, nothing collapsed. Three
    /// category headers (one per non-empty bucket) + one aggregate header per
    /// application + the flat background/uncategorized rows.
    fn rows_default(&self) -> usize {
        3 + self.apps + self.background + self.uncategorized
    }

    /// Canonical rows when every `app-tree:<root pid>` aggregate is expanded
    /// and nothing is collapsed: each application adds its root TreeNode and
    /// its `children_per_app` TreeNodes under the aggregate header.
    fn rows_all_expanded(&self) -> usize {
        3 + self.apps * (2 + self.children_per_app) + self.background + self.uncategorized
    }

    /// Expansion-set keys for the fully expanded state.
    fn all_expanded_groups(&self) -> HashSet<String> {
        let mut groups: HashSet<String> = ProcessCategory::ALL
            .iter()
            .copied()
            .map(category_expansion_key)
            .collect();
        for index in 0..self.apps {
            groups.insert(format!(
                "{}{}",
                taskmanager_shell::APP_TREE_EXPANSION_KEY_PREFIX,
                ProcessLiveKey::from_parts(
                    root_pid(index),
                    taskmanager_test_support::fixture_start_token(root_pid(index)),
                )
                .expect("fixture identity")
                .stable_key()
            ));
        }
        groups
    }
}

/// The pid of fixture application root number `index` (deterministic).
fn root_pid(index: usize) -> u32 {
    1_000 + index as u32
}

fn base_process(pid: u32, name: String, cpu: f32) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name)
        .current_cpu_percentage(cpu)
        .current_memory_bytes(1024 * 1024)
        .build()
}

/// An identified application process: its stable identity and display name
/// share the `prc-` marker, so both the aggregate header label and the row
/// name column show the same deterministic text.
fn app_process(pid: u32, name: String, cpu: f32) -> ProcessItem {
    let mut item = base_process(pid, name.clone(), cpu);
    let identity = ProcessApplicationIdentity::new(name.clone(), name, None)
        .expect("fixture identity carries real values");
    item.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    item
}

/// An identified child process under `parent`.
fn child_process(pid: u32, parent: u32, name: String, cpu: f32) -> ProcessItem {
    let mut item = base_process(pid, name.clone(), cpu);
    let identity = ProcessApplicationIdentity::new(name.clone(), name, None)
        .expect("fixture identity carries real values");
    item.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    item.parent_pid = Some(parent);
    item
}

/// A provider-confirmed background process (identity observation absent).
fn background_process(pid: u32, name: String, cpu: f32) -> ProcessItem {
    let mut item = base_process(pid, name, cpu);
    item.apply_application_identity(
        ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10),
    );
    item
}

/// A process whose identity truth is currently unknown (no observation):
/// classified Uncategorized, never fabricated into Background.
fn uncategorized_process(pid: u32, name: String, cpu: f32) -> ProcessItem {
    base_process(pid, name, cpu)
}

fn tree_fixture(
    apps: usize,
    children_per_app: usize,
    background: usize,
    uncategorized: usize,
) -> TreeFixture {
    let mut processes =
        Vec::with_capacity(apps * (1 + children_per_app) + background + uncategorized);
    let mut next_child_pid = 2_000_000u32;
    for index in 0..apps {
        let parent = root_pid(index);
        processes.push(app_process(
            parent,
            format!("prc-app-{index:05}"),
            (apps - index) as f32,
        ));
        for child in 0..children_per_app {
            processes.push(child_process(
                next_child_pid,
                parent,
                format!("prc-kid-{index:05}-{child:02}"),
                0.5,
            ));
            next_child_pid += 1;
        }
    }
    for index in 0..background {
        processes.push(background_process(
            5_000_000 + index as u32,
            format!("prc-bg-{index:05}"),
            0.2,
        ));
    }
    for index in 0..uncategorized {
        processes.push(uncategorized_process(
            7_000_000 + index as u32,
            format!("prc-unc-{index:05}"),
            0.1,
        ));
    }
    TreeFixture {
        processes,
        apps,
        children_per_app,
        background,
        uncategorized,
    }
}

/// A minimal but complete telemetry snapshot. Without it the shell keeps its
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

/// Build an uncomposed TuiApp seeded with the fixture through the shell's
/// typed fixture boundary (no `/proc`, no network, no host state).
fn seeded_app(fixture: &TreeFixture) -> TuiApp {
    let mut shell = taskmanager_shell::ShellApp::new();
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(minimal_snapshot()))),
    );
    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Processes(Some(fixture.processes.clone())),
    );
    let mut app = TuiApp::from_shell(shell);
    // Same explicit UTC fixture rule as the demo frame: render timestamps must
    // not depend on the host timezone.
    app.local_time_rules =
        LocalTimeRulesObservation::current(LocalTimeRules::utc(), FIXTURE_TIMESTAMP_MS);
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

/// Render one full 120x40 frame and return its painted text lines.
fn frame_lines(app: &TuiApp) -> Vec<String> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal
        .backend()
        .to_string()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Painted visual rows: every fixture row (aggregate headers included) carries
/// the unique `prc-` marker, category headers do not, and with the cursor on
/// the row-0 category header the details panel shows its empty state, so the
/// count equals the materialized table window exactly.
fn painted_process_rows(app: &TuiApp) -> usize {
    frame_lines(app)
        .iter()
        .filter(|line| line.contains("prc-"))
        .count()
}

#[test]
fn canonical_rows_follow_the_pure_count_formula_at_10k() {
    let fixture = tree_fixture(FIXTURE_10K.0, FIXTURE_10K.1, FIXTURE_10K.2, FIXTURE_10K.3);
    assert_eq!(fixture.process_count(), 10_000);
    let refs: Vec<&ProcessItem> = fixture.processes.iter().collect();

    // Default TuiApp state: category buckets expanded, aggregates collapsed.
    let default_groups: HashSet<String> = ProcessCategory::ALL
        .iter()
        .copied()
        .map(category_expansion_key)
        .collect();
    let rows = process_view_support::build_process_rows(
        &refs,
        &default_groups,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    assert_eq!(rows.len(), fixture.rows_default());

    // Fully expanded: every application aggregate opens its recursive tree.
    let expanded = fixture.all_expanded_groups();
    let rows = process_view_support::build_process_rows(
        &refs,
        &expanded,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    assert_eq!(rows.len(), fixture.rows_all_expanded());

    // Collapsing the first application root hides exactly its child subtree;
    // the aggregate header and the root row stay visible (flatten emits the
    // node itself, then gates the children on the collapsed set).
    let collapsed: HashSet<ProcessLiveKey> = HashSet::from([ProcessLiveKey::from_parts(
        root_pid(0),
        taskmanager_test_support::fixture_start_token(root_pid(0)),
    )
    .expect("fixture identity")]);
    let rows = process_view_support::build_process_rows(
        &refs,
        &expanded,
        &collapsed,
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    assert_eq!(
        rows.len(),
        fixture.rows_all_expanded() - fixture.children_per_app
    );

    // The row list opens with the first category header, which is structural:
    // it aggregates but never resolves to a process.
    assert!(matches!(
        rows.first(),
        Some(ProcessRow::Group { depth: 0, .. })
    ));
    assert_eq!(process_at(&rows, 0), None);
}

#[test]
fn canonical_slice_at_50k_stays_a_pure_o_n_function_of_the_inputs() {
    let fixture = tree_fixture(FIXTURE_50K.0, FIXTURE_50K.1, FIXTURE_50K.2, FIXTURE_50K.3);
    assert_eq!(fixture.process_count(), 50_000);
    let refs: Vec<&ProcessItem> = fixture.processes.iter().collect();

    let rows = process_view_support::build_process_rows(
        &refs,
        &fixture.all_expanded_groups(),
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    // The canonical slice grows with N (55_005 rows for 50_000 processes, the
    // same pure formula the 10k fixture satisfies) — the bounded paint window
    // below is what keeps the FRAME cost independent of it.
    assert_eq!(rows.len(), fixture.rows_all_expanded());
}

#[test]
fn visual_row_count_matches_the_canonical_slice_and_invalidates_per_input() {
    let fixture = tree_fixture(FIXTURE_10K.0, FIXTURE_10K.1, FIXTURE_10K.2, FIXTURE_10K.3);
    let mut app = seeded_app(&fixture);

    assert_eq!(app.visual_row_count(), fixture.rows_default());
    // A repeat call takes the revision-keyed cache and must stay correct.
    assert_eq!(app.visual_row_count(), fixture.rows_default());
    assert_eq!(app.visual_row_count(), app.process_rows_snapshot().len());

    // Geometry input: expanding every application aggregate adds their trees.
    for index in 0..fixture.apps {
        app.expanded_groups.insert(format!(
            "{}{}",
            taskmanager_shell::APP_TREE_EXPANSION_KEY_PREFIX,
            ProcessLiveKey::from_parts(
                root_pid(index),
                taskmanager_test_support::fixture_start_token(root_pid(index)),
            )
            .expect("fixture identity")
            .stable_key()
        ));
    }
    assert_eq!(app.visual_row_count(), fixture.rows_all_expanded());

    // Tree input: collapsing one root hides exactly its child subtree.
    app.collapsed_tree.insert(
        ProcessLiveKey::from_parts(
            root_pid(0),
            taskmanager_test_support::fixture_start_token(root_pid(0)),
        )
        .expect("fixture identity"),
    );
    assert_eq!(
        app.visual_row_count(),
        fixture.rows_all_expanded() - fixture.children_per_app
    );
    app.collapsed_tree.clear();

    // Query input: only identified application roots match "prc-app"; the
    // emptied background/uncategorized buckets fabricate no headers, so the
    // count is one category header plus one aggregate + one root row per app.
    app.query = "prc-app".to_string();
    assert_eq!(app.visual_row_count(), 1 + 2 * fixture.apps);
    app.query.clear();

    // Revision input: a new provider batch (process revision bump) must
    // invalidate the cache even with identical presentation inputs. The
    // all-expanded geometry from above is still in force, so the extra
    // background process adds exactly one visible row to that state.
    let mut rebatch = fixture.processes.clone();
    rebatch.push(background_process(
        5_999_999,
        "prc-bg-extra".to_string(),
        0.1,
    ));
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Processes(Some(rebatch)));
    assert_eq!(app.visual_row_count(), fixture.rows_all_expanded() + 1);
    assert_eq!(app.visual_row_count(), app.process_rows_snapshot().len());
}

#[test]
fn materialized_window_is_bounded_and_independent_of_process_count() {
    let fixture_10k = tree_fixture(FIXTURE_10K.0, FIXTURE_10K.1, FIXTURE_10K.2, FIXTURE_10K.3);
    let app_10k = seeded_app(&fixture_10k);
    let painted_10k = painted_process_rows(&app_10k);

    let fixture_50k = tree_fixture(FIXTURE_50K.0, FIXTURE_50K.1, FIXTURE_50K.2, FIXTURE_50K.3);
    let app_50k = seeded_app(&fixture_50k);
    let painted_50k = painted_process_rows(&app_50k);

    // The table materializes only the bounded viewport: the same terminal
    // paints the same number of rows whether the projection holds 10k or 50k
    // processes, and that number can never exceed the 40-row terminal.
    assert_eq!(painted_10k, painted_50k);
    assert!(
        painted_50k >= 3,
        "a 40-row terminal must paint several table rows, got {painted_50k}"
    );
    assert!(
        painted_50k < 40,
        "painted rows must stay inside the terminal, got {painted_50k}"
    );
}

#[test]
fn deep_selection_keeps_the_materialized_window_bounded() {
    let fixture = tree_fixture(FIXTURE_10K.0, FIXTURE_10K.1, FIXTURE_10K.2, FIXTURE_10K.3);
    let mut app = seeded_app(&fixture);
    // Park the cursor deep in the O(N) canonical slice, past every visible
    // row; the window re-centers instead of materializing up to the cursor.
    app.selected = 8_000;
    let painted = painted_process_rows(&app);
    // The details panel may echo the selected process name on up to a few
    // lines on top of the table window, hence the small constant slack.
    assert!(
        painted < 44,
        "a deep cursor must not pull O(N) rows into the frame, got {painted}"
    );
    assert!(painted >= 3, "the centered window must still paint rows");
}

#[test]
fn steady_state_frame_smoke_budget_at_50k() {
    let fixture = tree_fixture(FIXTURE_50K.0, FIXTURE_50K.1, FIXTURE_50K.2, FIXTURE_50K.3);
    let app = seeded_app(&fixture);

    // Warm-up frame: first-paint costs (shell memo, visual_row_count build,
    // ratatui buffer allocation) are intentionally excluded from the smoke.
    let _ = frame_lines(&app);
    let start = Instant::now();
    let _ = frame_lines(&app);
    let steady_state = start.elapsed();

    // Wall-clock is the SMOKE half of the budget only; the deterministic
    // half is the allocation contract in tests/perf_budget_alloc_tests.rs.
    // Measured steady-state 50k frame (debug build, dev workstation,
    // 2026-08-29): ~70 ms. The 2 s cap is ~28x that measurement, so a loaded
    // or slower CI machine cannot flake it while still catching a real
    // per-frame complexity regression (an accidental O(N) paint would push a
    // debug frame well past it).
    assert!(
        steady_state < Duration::from_millis(2_000),
        "50k steady-state frame took {steady_state:?}, smoke budget is 2s"
    );
}

// ─── Layered dirty-repaint behavior (TUI-006 charter tail) ──────────────────
//
// The run loop must not repaint at the fixed poll cadence: an idle cycle (no
// platform batch, no queued refresh, no ancillary effect, no event) must skip
// `terminal.draw` entirely, and a cycle whose poll surfaced an event must
// paint on the next cycle. The pure predicate has its own unit tests
// (`tests/gui/runtime/tests/draw_predicate.rs`); the tests below pin the LOOP
// side of the contract end to end — a regression that deletes the
// `pending_draw || should_draw(cycle)` guard fails here on the draw COUNT,
// which no predicate unit test can catch.
//
// Ratatui supplies the second layer by construction: `Terminal::flush` feeds
// `Backend::draw` from a lazy cell diff of the two frame buffers
// (ratatui-core `terminal/buffers.rs`), so even a forced draw writes only
// changed cells. The layers compose: loop-level skip on idle, cell-level diff
// on paint — no per-layer partial-repaint machinery in this frontend.

use std::convert::Infallible;
use std::io;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Position, Size};

/// A [`TestBackend`] that counts how many times ratatui asked it to draw. The
/// count is the observable the dirty-repaint contract is stated in: one draw
/// for the initial frame, then draws only for cycles that produced render
/// state. Everything else is delegated untouched so the painted bytes stay
/// exactly what a plain `TestBackend` would hold.
struct DrawCountingBackend {
    inner: TestBackend,
    draw_calls: std::cell::Cell<usize>,
}

impl DrawCountingBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            inner: TestBackend::new(width, height),
            draw_calls: std::cell::Cell::new(0),
        }
    }
}

impl Backend for DrawCountingBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Infallible>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.draw_calls.set(self.draw_calls.get() + 1);
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Infallible> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Infallible> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Infallible> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Infallible> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Infallible> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Infallible> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Infallible> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Infallible> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Infallible> {
        self.inner.flush()
    }
}

/// One scripted poll window. `Idle` models a quiet terminal (the blocking
/// poll returns "not ready"); `Events` models the events that arrived during
/// that window. Steps are consumed one per BLOCKING poll, so a step's events
/// land in exactly one loop cycle — the deterministic stand-in for wall-clock
/// event timing, with no sleeping (the timeout value only tells the blocking
/// window poll, which advances the script, apart from the loop's in-batch
/// zero probe, which only reports the current window's readiness).
enum ScriptStep {
    Idle,
    Events(Vec<Event>),
}

/// The scripted event source for the loop-level dirty-repaint tests.
/// Fail-closed: when a NEW blocking wait is requested with no scripted step
/// left, it surfaces a typed error instead of blocking forever, so a
/// regression that drops the quit key fails the test rather than hanging.
struct SteppedEventSource {
    steps: std::collections::VecDeque<Vec<Event>>,
    ready: Option<std::collections::VecDeque<Event>>,
}

impl SteppedEventSource {
    fn new(steps: Vec<ScriptStep>) -> Self {
        Self {
            steps: steps
                .into_iter()
                .map(|step| match step {
                    ScriptStep::Idle => Vec::new(),
                    ScriptStep::Events(events) => events,
                })
                .collect(),
            ready: None,
        }
    }
}

impl crate::runtime::runtime_support::TerminalEventSource for SteppedEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        if timeout > Duration::ZERO {
            // A fresh blocking wait: the next scripted window's events become
            // ready (an empty window stays an honest "nothing arrived").
            match self.steps.pop_front() {
                Some(events) => {
                    self.ready = Some(events.into_iter().collect());
                }
                None => {
                    return Err(io::Error::other("script exhausted without a quit key"));
                }
            }
        }
        // A zero timeout (the loop's batch-drain probe) only reports whether
        // events are ALREADY queued from the current window.
        Ok(self.ready.as_ref().is_some_and(|batch| !batch.is_empty()))
    }

    fn read(&mut self) -> io::Result<Event> {
        self.ready
            .as_mut()
            .and_then(std::collections::VecDeque::pop_front)
            .ok_or_else(|| io::Error::other("read on an empty script window"))
    }
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new_with_kind(
        code,
        KeyModifiers::NONE,
        KeyEventKind::Press,
    ))
}

/// Drive the production run loop against the counting backend with the given
/// scripted windows and return how many times the loop drew.
fn driven_draw_count(steps: Vec<ScriptStep>) -> io::Result<usize> {
    let fixture = tree_fixture(4, 5, 10, 6);
    let mut app = seeded_app(&fixture);
    let mut terminal = Terminal::new(DrawCountingBackend::new(80, 24)).expect("test terminal");
    let outcome = crate::runtime::runtime_support::run_event_loop(
        &mut terminal,
        &mut app,
        None,
        SteppedEventSource::new(steps),
        false,
        None,
    );
    outcome?;
    Ok(terminal.backend().draw_calls.get())
}

#[test]
fn idle_cycles_do_not_draw_and_only_the_initial_frame_paints() {
    // Initial frame + three fully idle poll windows + the quit window: the
    // draw count must stay at ONE (the initial frame). This is the loop-side
    // behavioral guarantee of the dirty flag — a regression that repaints at
    // the poll cadence turns this into 4 and fails.
    let draws = driven_draw_count(vec![
        ScriptStep::Idle,
        ScriptStep::Idle,
        ScriptStep::Idle,
        ScriptStep::Events(vec![press(KeyCode::Char('q'))]),
    ])
    .expect("scripted loop must exit via the quit key, never an error");
    assert_eq!(
        draws, 1,
        "idle cycles must skip terminal.draw entirely (initial frame only), got {draws} draws"
    );
}

#[test]
fn an_event_window_forces_exactly_one_follow_up_repaint() {
    // Initial frame, then one Down-press window (the key arrives in the poll
    // phase, so the repaint lands on the NEXT cycle), then an idle window
    // (must NOT draw again), then quit. Exactly 2 draws: the key's repaint
    // happens once and the quiet window after it stays skipped.
    let draws = driven_draw_count(vec![
        ScriptStep::Events(vec![press(KeyCode::Down)]),
        ScriptStep::Idle,
        ScriptStep::Events(vec![press(KeyCode::Char('q'))]),
    ])
    .expect("scripted loop must exit via the quit key, never an error");
    assert_eq!(
        draws, 2,
        "an event cycle must produce exactly one follow-up repaint and the \
         idle window after it none, got {draws} draws"
    );
}
