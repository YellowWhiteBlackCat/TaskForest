use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::process_telemetry::{OpenFileKind, ThreadState};

/// Pin English and serialize against the language-flipping i18n test, so
/// the chrome assertions below stay deterministic.
fn en() -> std::sync::MutexGuard<'static, ()> {
    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    guard
}

/// Render a line vector through the same TestBackend path the frame tests
/// use, returning the flattened text so preview structure can be asserted.
fn render_text(lines: Vec<ratatui::text::Line<'static>>) -> String {
    let backend = TestBackend::new(96, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| frame.render_widget(Paragraph::new(lines), frame.area()))
        .expect("draw");
    terminal.backend().to_string()
}

/// Honesty: a thread whose `stat` lacked parseable CPU counters must render
/// explicit dashes, never a fabricated `0.0s` / `0.0%`. A warm thread keeps
/// both its cumulative time and instantaneous rate.
#[test]
fn format_thread_row_keeps_missing_cpu_honest() {
    let warm = ProcessThreadInfo {
        tid: 4242,
        comm: "telemetry-main".into(),
        state: ThreadState::Sleep,
        cpu_time_secs: Some(12.5),
        cpu_percent: Some(18.5),
    };
    let gap = ProcessThreadInfo {
        tid: 4243,
        comm: "reaper".into(),
        state: ThreadState::Running,
        cpu_time_secs: None,
        cpu_percent: None,
    };
    let warm_line = format_thread_row(&warm);
    let gap_line = format_thread_row(&gap);
    assert!(
        warm_line.contains("4242") && warm_line.contains("12.5s") && warm_line.contains("18.5%")
    );
    assert!(warm_line.contains("S"));
    assert!(
        gap_line.contains("—") && !gap_line.contains("0.0s") && !gap_line.contains("0.0%"),
        "a thread with missing CPU values must show dashes, got: {gap_line}"
    );
    // The short state label still renders for the gap row.
    assert!(gap_line.contains("R"));
}

/// Honesty: a descriptor whose readlink failed keeps its row with the typed
/// unreadable marker, never a blank target or a fabricated path.
#[test]
fn format_open_file_row_keeps_unreadable_target_honest() {
    let readable = OpenFileEntry {
        fd: 0,
        kind: OpenFileKind::File,
        target: Some("/dev/null".into()),
    };
    let unreadable = OpenFileEntry {
        fd: 9,
        kind: OpenFileKind::Other,
        target: None,
    };
    assert!(format_open_file_row(&readable, "unreadable").contains("/dev/null"),);
    let denied = format_open_file_row(&unreadable, "unreadable");
    assert!(
        denied.ends_with("unreadable"),
        "an unreadable fd must surface the typed marker, got: {denied}"
    );
}

/// Honesty: a cold-start engine (no current usage) must render an explicit
/// dash, never a fabricated `0.0%`; a warmed engine reports its percentage.
#[test]
fn format_engine_usage_line_keeps_cold_start_honest() {
    let warm = format_engine_usage_line("video", &ScalarObservation::available(12.5, 1));
    let gap = format_engine_usage_line("render", &ScalarObservation::<f32>::default());
    assert!(warm.contains("video") && warm.contains("12.5%"));
    assert!(
        gap.contains("render") && gap.contains("—") && !gap.contains("0.0%"),
        "a cold-start engine must show a dash, got: {gap}"
    );
}

/// The threads preview renders the column header, the first preview rows,
/// and an honest "…" when more remain; a thread whose CPU counters parsed
/// still surfaces its time and rate inside the rendered frame.
#[test]
fn thread_preview_renders_header_rows_and_ellipsis() {
    let _guard = en();
    let threads = ProcessThreads {
        state: DeviceState::healthy(1),
        threads: vec![
            ProcessThreadInfo {
                tid: 100,
                comm: "main".into(),
                state: ThreadState::Running,
                cpu_time_secs: Some(2.0),
                cpu_percent: Some(5.0),
            },
            ProcessThreadInfo {
                tid: 101,
                comm: "worker".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
            },
            ProcessThreadInfo {
                tid: 102,
                comm: "extra-1".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
            },
            ProcessThreadInfo {
                tid: 103,
                comm: "extra-2".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
            },
        ],
    };
    let text = render_text(thread_preview_lines(&threads, TuiTheme::default()));
    // Header + count title are present.
    assert!(
        text.contains("Threads 4"),
        "count title must render, got:\n{text}"
    );
    assert!(text.contains("TID"), "column header must render");
    assert!(text.contains("CPU %"), "CPU% header column must render");
    // First three rows render with their tid + state label.
    assert!(text.contains("100") && text.contains("main") && text.contains("2.0s"));
    assert!(text.contains("102") && text.contains("extra-1"));
    // The fourth thread is beyond the preview bound: only the ellipsis shows.
    assert!(text.contains('…'));
    assert!(
        !text.contains("extra-2"),
        "a truncated thread must not render"
    );
}

/// An empty thread list renders the explicit empty state, never a
/// fabricated header or row.
#[test]
fn thread_preview_renders_empty_state_for_no_threads() {
    let _guard = en();
    let threads = ProcessThreads {
        state: DeviceState::healthy(1),
        threads: Vec::new(),
    };
    let text = render_text(thread_preview_lines(&threads, TuiTheme::default()));
    assert!(text.contains("No threads"), "empty state must render");
    assert!(
        !text.contains("TID"),
        "an empty list must not fabricate a header"
    );
}

/// The open-files preview renders the count plus the unreadable marker, the
/// first descriptors as `fd → target` (None target → typed marker), and an
/// ellipsis when more than the preview bound remain.
#[test]
fn open_files_preview_renders_count_unreadable_and_rows() {
    let _guard = en();
    let open_files = ProcessOpenFiles {
        state: DeviceState::healthy(1),
        unreadable_count: 1,
        entries: vec![
            OpenFileEntry {
                fd: 0,
                kind: OpenFileKind::File,
                target: Some("/dev/null".into()),
            },
            OpenFileEntry {
                fd: 3,
                kind: OpenFileKind::Socket,
                target: Some("socket:[4242]".into()),
            },
            OpenFileEntry {
                fd: 9,
                kind: OpenFileKind::Other,
                target: None,
            },
            OpenFileEntry {
                fd: 10,
                kind: OpenFileKind::Pipe,
                target: Some("pipe:[5]".into()),
            },
        ],
    };
    let text = render_text(open_files_preview_lines(&open_files, TuiTheme::default()));
    // Header carries the count and the unreadable marker.
    assert!(
        text.contains("Open files 4") && text.contains("1 unreadable"),
        "count + unreadable marker must render, got:\n{text}"
    );
    // Readable descriptors render as `fd → target`; the None target keeps
    // the typed unreadable marker.
    assert!(text.contains("0 → /dev/null"));
    assert!(text.contains("9 → unreadable"));
    // The fourth descriptor is beyond the preview bound.
    assert!(text.contains('…'));
    assert!(
        !text.contains("pipe:[5]"),
        "a truncated descriptor must not render"
    );
}

/// A healthy process with no readable descriptors renders the explicit
/// empty state, never a fabricated count or row.
#[test]
fn open_files_preview_renders_empty_state_for_no_descriptors() {
    let _guard = en();
    let open_files = ProcessOpenFiles {
        state: DeviceState::healthy(1),
        unreadable_count: 0,
        entries: Vec::new(),
    };
    let text = render_text(open_files_preview_lines(&open_files, TuiTheme::default()));
    assert!(
        text.contains("No readable file descriptors"),
        "empty state must render"
    );
    assert!(
        !text.contains("Open files 0"),
        "an empty list must not fabricate a count"
    );
}
