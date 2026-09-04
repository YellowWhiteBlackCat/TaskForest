//! CPU Affinity editor modal: the fail-closed open/observe/apply discipline.
//!
//! The editor never fabricates an editable default: opening queues a fresh
//! correlated read, an unobserved mask renders "collecting" and rejects Enter
//! with an honest notice, and only a snapshot whose frozen target matches the
//! identity captured at open time may seed or rewrite the visible mask. The
//! chains below drive the real key paths, the real `queue_effect` submission
//! (a recording process-affinity port), and the real platform-batch fold.

use super::super::*;

use std::sync::{Arc, Mutex};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    AppAction, AppPage, CorrelatedEvent, PlatformEffect, PlatformEvent, PlatformEventBatch,
    PlatformFacets, ProcessAffinityEvent, ProcessAffinityRequest, ProcessFacets,
};
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    EventSequence, RequestEnvelope, RequestId, RequestPort, SubmissionError,
};

use crate::render;
use crate::{TuiApp, TuiSurfaceKind, TuiTheme};

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

fn app_on_processes() -> TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.shell.selected = 0;
    app.reconcile_applications_cursor();
    app
}

#[derive(Default)]
struct EmptyCapabilities;
impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct EmptyEvents;
impl EventPort for EmptyEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

/// Records every accepted affinity read (with its envelope request id), so a
/// correlated completion event can echo the exact id back.
#[derive(Default)]
struct RecordingAffinityReads(Mutex<Vec<(RequestId, ProcessAffinityRequest)>>);
impl RequestPort for RecordingAffinityReads {
    type Request = ProcessAffinityRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((request.id, request.payload));
        Ok(())
    }
}

fn affinity_client(
    recorder: Arc<RecordingAffinityReads>,
) -> taskmanager_application::PlatformClient {
    taskmanager_application::PlatformClient::new(taskmanager_application::PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default().with_process(ProcessFacets::default().with_affinity(recorder)),
    ))
}

fn recorded_reads(
    recorder: &Arc<RecordingAffinityReads>,
) -> Vec<(RequestId, ProcessAffinityRequest)> {
    recorder
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Open the editor for the selected process and complete its queued read with
/// `cpus` — the same open → effect → shared seam → correlated batch chain the
/// live runtime runs.
fn open_and_observe(app: &mut TuiApp, cpus: Vec<u32>) -> FrozenProcessIdentity {
    let recorder = Arc::new(RecordingAffinityReads::default());
    let effect = app
        .open_process_affinity()
        .expect("the affinity editor opens for a selected process");
    let PlatformEffect::ProcessAffinity(request) = &effect else {
        panic!("opening queues the correlated read, got {effect:?}");
    };
    let target = request.target.clone();
    let mut client = affinity_client(Arc::clone(&recorder));
    taskmanager_shell::queue_effect(&mut app.shell, &mut client, effect);

    let reads = recorded_reads(&recorder);
    let (request_id, _) = reads
        .first()
        .cloned()
        .expect("exactly one affinity read is submitted");
    let mut batch = PlatformEventBatch::default();
    batch.process_affinity_events.push(CorrelatedEvent {
        request_id,
        capability: CapabilityId::PROCESS_AFFINITY,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: ProcessAffinityEvent::Snapshot {
            target: target.clone(),
            cpus,
        },
    });
    app.apply_platform_batch(batch);
    target
}

#[test]
fn opening_shows_collecting_state_and_queues_a_correlated_read() {
    let mut app = app_on_processes();
    let process = app
        .selected_detail_process()
        .expect("selected process on applications page");
    let name = process.name.clone();
    let pid = process.pid;

    assert!(app.process_affinity().is_none());
    let effect = app.open_process_affinity();
    let Some(PlatformEffect::ProcessAffinity(request)) = &effect else {
        panic!("opening queues the correlated read, got {effect:?}");
    };
    assert_eq!(request.target.pid, pid);
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::ProcessAffinity)
    );

    let modal = app.process_affinity().expect("modal state is populated");
    assert_eq!(modal.target.name, name);
    assert_eq!(modal.logical_cpu_count, app.logical_cpu_count());
    // An unobserved mask is never fabricated into an editable default.
    assert!(!modal.mask_observed);
    assert!(modal.selected_mask.is_empty());

    let text = frame_text(&app, 100, 30);
    assert!(
        text.contains("CPU affinity"),
        "must display modal title, got:\n{text}"
    );
    assert!(text.contains(&name));
    assert!(text.contains(&format!("PID: {pid}")));
    assert!(
        text.contains("Collecting telemetry"),
        "the unobserved editor must report the honest collecting state, got:\n{text}"
    );
    assert!(
        text.contains("[ ] CPU 0"),
        "every CPU renders unchecked before the read lands, got:\n{text}"
    );
}

#[test]
fn correlated_read_seeds_the_grid_and_unblocks_toggles() {
    let mut app = app_on_processes();
    let target = open_and_observe(&mut app, vec![0, 2]);

    let modal = app.process_affinity().expect("modal stays open");
    assert!(modal.mask_observed);
    assert_eq!(modal.selected_mask, vec![0, 2]);

    let text = frame_text(&app, 100, 30);
    assert!(
        text.contains("(2/"),
        "header counts the observed mask: {text}"
    );
    assert!(text.contains("[x] CPU 0"));
    assert!(text.contains("[x] CPU 2"));
    assert!(text.contains("[ ] CPU 1"));
    let _ = target;
}

#[test]
fn enter_before_observed_mask_reports_collecting_and_keeps_editor_open() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let mut app = app_on_processes();
    let effect = app.open_process_affinity();
    assert!(effect.is_some(), "opening queues the read");

    let effect = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(effect.is_none(), "an unobserved mask never submits");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::ProcessAffinity),
        "the editor stays open for the decision"
    );
    assert!(
        app.feedback_text().contains("Collecting telemetry"),
        "the honest collecting notice is reported, got: {}",
        app.feedback_text()
    );
}

#[test]
fn empty_observed_mask_blocks_apply_with_warning() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let mut app = app_on_processes();
    open_and_observe(&mut app, Vec::new());

    let effect = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(effect.is_none(), "an empty observed mask never submits");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::ProcessAffinity)
    );
    assert!(
        app.feedback_text()
            .contains("Select at least one logical CPU"),
        "the selection warning is reported, got: {}",
        app.feedback_text()
    );
}

#[test]
fn enter_submits_affinity_control_request() {
    let mut app = app_on_processes();
    let all_cpus: Vec<u32> = (0..app.logical_cpu_count() as u32).collect();
    let target = open_and_observe(&mut app, all_cpus);
    let initial_count = app.logical_cpu_count();

    // Toggle CPU 0 off
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );

    // Press Enter to submit
    let effect = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.local_surface_kind(), None);
    assert!(app.process_affinity().is_none());

    let Some(PlatformEffect::ProcessAffinityControl(request)) = effect else {
        panic!("expected ProcessAffinityControl effect, got {effect:?}");
    };
    assert_eq!(request.target, target);
    assert_eq!(request.cpus.len(), initial_count - 1);
    assert!(!request.cpus.contains(&0));
}

#[test]
fn esc_cancels_without_changes() {
    let mut app = app_on_processes();
    let all_cpus: Vec<u32> = (0..app.logical_cpu_count() as u32).collect();
    open_and_observe(&mut app, all_cpus);

    // Toggle CPU 0 off
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );
    assert!(!app.process_affinity().unwrap().selected_mask.contains(&0));

    // Press Esc to cancel
    let effect = handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(effect.is_none());
    assert_eq!(app.local_surface_kind(), None);
    assert!(app.process_affinity().is_none());
}

#[test]
fn space_toggles_cpu_selection() {
    let mut app = app_on_processes();
    let all_cpus: Vec<u32> = (0..app.logical_cpu_count() as u32).collect();
    open_and_observe(&mut app, all_cpus);
    let initial_count = app.process_affinity().unwrap().logical_cpu_count;
    assert!(app.process_affinity().unwrap().selected_mask.contains(&0));

    // Press Space on CPU 0 to toggle it off
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert_eq!(
        app.process_affinity().unwrap().selected_mask.len(),
        initial_count - 1
    );
    assert!(!app.process_affinity().unwrap().selected_mask.contains(&0));

    // Press Space again on CPU 0 to toggle it back on
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert_eq!(
        app.process_affinity().unwrap().selected_mask.len(),
        initial_count
    );
    assert!(app.process_affinity().unwrap().selected_mask.contains(&0));
}

#[test]
fn a_key_toggles_all_cpus() {
    let mut app = app_on_processes();
    let all_cpus: Vec<u32> = (0..app.logical_cpu_count() as u32).collect();
    open_and_observe(&mut app, all_cpus);
    let count = app.process_affinity().unwrap().logical_cpu_count;
    assert_eq!(app.process_affinity().unwrap().selected_mask.len(), count);

    // Press 'a' to deselect all CPUs
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert!(app.process_affinity().unwrap().selected_mask.is_empty());

    // Press 'a' again to select all CPUs
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
    );
    assert!(effect.is_none());
    assert_eq!(app.process_affinity().unwrap().selected_mask.len(), count);
}

#[test]
fn process_menu_opens_affinity_modal_via_action_and_hotkey() {
    let mut app = app_on_processes();
    assert!(app.open_process_menu());
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::ProcessMenu));

    // Pressing 'a' while in the process context menu opens the affinity modal
    // and queues the correlated read for the menu row's frozen identity.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
    );
    let Some(PlatformEffect::ProcessAffinity(request)) = &effect else {
        panic!("the hotkey queues the read, got {effect:?}");
    };
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::ProcessAffinity)
    );
    let expected_pid = app.process_affinity().expect("modal open").target.pid;
    assert_eq!(request.target.pid, expected_pid);

    // Close and open via selecting Affinity action in menu
    app.close_local_overlays();
    assert!(app.open_process_menu());
    let affinity_index = crate::ui::process_menu::MENU_ACTIONS
        .iter()
        .position(|&action| action == crate::ui::process_menu::ProcessMenuAction::Affinity)
        .expect("affinity action exists in menu");
    if let Some(menu) = app.process_menu_mut() {
        menu.selection = affinity_index;
    }
    let effect = handle_key(&mut app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(effect.is_some(), "the menu action also queues the read");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::ProcessAffinity)
    );
}

#[test]
fn navigation_arrows_move_selected_cpu() {
    let mut app = app_on_processes();
    let effect = app.open_process_affinity();
    assert!(effect.is_some());
    assert_eq!(app.process_affinity().unwrap().selected_cpu, 0);

    // Right -> 1
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.process_affinity().unwrap().selected_cpu, 1);

    // Down -> 1 + AFFINITY_GRID_COLS
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        app.process_affinity().unwrap().selected_cpu,
        1 + crate::surface::AFFINITY_GRID_COLS
    );

    // Left -> 0 + AFFINITY_GRID_COLS
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(
        app.process_affinity().unwrap().selected_cpu,
        crate::surface::AFFINITY_GRID_COLS
    );

    // Up -> 0
    let _ = handle_key(&mut app, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.process_affinity().unwrap().selected_cpu, 0);
}

#[test]
fn fresh_authoritative_read_rewrites_in_progress_edits() {
    let mut app = app_on_processes();
    let all_cpus: Vec<u32> = (0..app.logical_cpu_count() as u32).collect();
    let target = open_and_observe(&mut app, all_cpus);
    let count = app.logical_cpu_count();

    // The user toggles CPU 0 off…
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
    );
    assert!(!app.process_affinity().unwrap().selected_mask.contains(&0));

    // …then a NEW correlated read for the same frozen identity lands (the
    // runtime queued a second read through the same seam).
    let recorder = Arc::new(RecordingAffinityReads::default());
    let effect = app
        .shell
        .request_process_affinity()
        .expect("the selected row has an identity");
    let mut client = affinity_client(Arc::clone(&recorder));
    taskmanager_shell::queue_effect(&mut app.shell, &mut client, effect);
    let (request_id, _) = recorded_reads(&recorder)
        .first()
        .cloned()
        .expect("the second read is submitted");
    let mut batch = PlatformEventBatch::default();
    batch.process_affinity_events.push(CorrelatedEvent {
        request_id,
        capability: CapabilityId::PROCESS_AFFINITY,
        provider: None,
        sequence: EventSequence::new(2),
        observed_at_ms: 2_000,
        event: ProcessAffinityEvent::Snapshot {
            target: target.clone(),
            cpus: (0..count as u32).collect(),
        },
    });
    app.apply_platform_batch(batch);

    // The authoritative read wins over the in-progress edit (the Iced
    // editor's same-wave sync rule), and the count moves back to full.
    let modal = app.process_affinity().expect("modal stays open");
    assert_eq!(modal.selected_mask.len(), count);
    assert!(modal.selected_mask.contains(&0));
}

#[test]
fn a_foreign_identity_read_never_enters_the_editor() {
    let mut app = app_on_processes();
    let _ = app.open_process_affinity();
    assert!(app.process_affinity().unwrap().selected_mask.is_empty());

    // A snapshot naming a different frozen identity arrives on the wire. Its
    // request id matches no open session, so the fold drops it before any
    // renderer state can see it.
    let mut batch = PlatformEventBatch::default();
    batch.process_affinity_events.push(CorrelatedEvent {
        request_id: taskmanager_platform_contract::RequestId::MIN,
        capability: CapabilityId::PROCESS_AFFINITY,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: ProcessAffinityEvent::Snapshot {
            target: FrozenProcessIdentity::from_authoritative_parts(999_999, "recycled", 0, 1)
                .expect("a frozen recycled identity"),
            cpus: (0..8u32).collect(),
        },
    });
    app.apply_platform_batch(batch);

    // The editor stays unobserved: no foreign mask, no fabricated default.
    let modal = app.process_affinity().expect("modal stays open");
    assert!(!modal.mask_observed);
    assert!(modal.selected_mask.is_empty());
}
