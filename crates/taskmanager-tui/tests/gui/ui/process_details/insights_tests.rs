use super::formatting::format_open_file_row;
use super::formatting::{
    format_capability_line, format_engine_cycles, format_engine_time, is_dangerous_capability,
};
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::widgets::Paragraph;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::process_telemetry::{
    OpenFileEntry, ProcessEnvironment, ProcessEnvironmentEntry, ProcessOpenFiles,
};
use taskmanager_core::core::process_telemetry::{
    OpenFileKind, ProcessGpuDevice, ProcessGpuEngineUsage, ProcessGpuEngines, ProcessGpuSnapshot,
    ThreadState,
};
use taskmanager_platform_contract::RequestId;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::presentation::missing_value;

/// Compact preview row bounds asserted by the headless preview tests. The
/// production renderer always passes its own limit, so these bounded wrappers
/// live with the tests that exercise them.
const OPEN_FILES_PREVIEW: usize = 3;
const ENVIRONMENT_PREVIEW: usize = 3;

fn open_files_preview_lines(
    open_files: &ProcessOpenFiles,
    theme: TuiTheme,
) -> Vec<ratatui::text::Line<'static>> {
    open_files_preview_lines_with_limit(open_files, theme, OPEN_FILES_PREVIEW)
}

fn environment_preview_lines(
    env: &ProcessEnvironment,
    theme: TuiTheme,
) -> Vec<ratatui::text::Line<'static>> {
    environment_preview_lines_with_limit(env, theme, ENVIRONMENT_PREVIEW)
}

/// String projection of one capability row for the warning-marker assertions;
/// the styled production path is `format_capability_line`.
fn format_capability_row(cap: &str) -> String {
    if is_dangerous_capability(cap) {
        format!("[!] \u{26a0} {cap}")
    } else {
        cap.to_string()
    }
}

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
        wchan: None,
        run_queue_wait_ns: None,
        wait_kind: None,
    };
    let gap = ProcessThreadInfo {
        tid: 4243,
        comm: "reaper".into(),
        state: ThreadState::Running,
        cpu_time_secs: None,
        cpu_percent: None,
        wchan: None,
        run_queue_wait_ns: None,
        wait_kind: None,
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
        deleted: false,
    };
    let unreadable = OpenFileEntry {
        fd: 9,
        kind: OpenFileKind::Other,
        target: None,
        deleted: false,
    };
    assert!(format_open_file_row(&readable, "unreadable").contains("/dev/null"),);
    let denied = format_open_file_row(&unreadable, "unreadable");
    assert!(
        denied.ends_with("unreadable"),
        "an unreadable fd must surface the typed marker, got: {denied}"
    );
}

/// Honesty: a cold-start engine (no current usage) must render an explicit
/// dash, never a fabricated `0.0%`; a warmed engine reports its percentage and
/// its cumulative busy time / cycles.
#[test]
fn format_engine_usage_line_keeps_cold_start_honest() {
    let warm_engine = ProcessGpuEngineUsage {
        name: "video".into(),
        usage_pct: ScalarObservation::available(12.5, 1),
        engine_time_ns: ScalarObservation::available(2_500_000_000, 1),
        engine_cycles: ScalarObservation::default(),
    };
    let gap_engine = ProcessGpuEngineUsage {
        name: "render".into(),
        usage_pct: ScalarObservation::default(),
        engine_time_ns: ScalarObservation::default(),
        engine_cycles: ScalarObservation::default(),
    };
    let warm = format_engine_usage_line(&warm_engine);
    let gap = format_engine_usage_line(&gap_engine);
    assert!(warm.contains("video") && warm.contains("12.5%") && warm.contains("2.5s"));
    assert!(
        gap.contains("render") && gap.contains("—") && !gap.contains("0.0%"),
        "a cold-start engine must show a dash, got: {gap}"
    );
}

/// xe fdinfo exposes cycles instead of busy ns: the cycle count must render
/// as the honest cumulative observable, never a fabricated time.
#[test]
fn format_engine_usage_line_cycles_only_engine_renders_cycles_not_time() {
    let engine = ProcessGpuEngineUsage {
        name: "vcs".into(),
        usage_pct: ScalarObservation::default(),
        engine_time_ns: ScalarObservation::default(),
        engine_cycles: ScalarObservation::available(643_228_675_411, 1),
    };
    let line = format_engine_usage_line(&engine);
    assert!(line.contains("vcs"), "{line}");
    assert!(line.contains("643.23G cycles"), "{line}");
    assert!(
        !line.contains("0.0s"),
        "a cycles-only source must not fabricate a duration: {line}"
    );
}

/// GPU device row renders `GPU #<id>` and utilization + VRAM.
#[test]
fn format_gpu_device_row_renders_device_id_and_vram() {
    let _guard = en();
    let device = ProcessGpuDevice {
        device_id: "0".into(),
        memory_bytes: Some(1024 * 1024 * 1024),
        utilization_pct: Some(42.0),
        engine_time_ns: None,
    };
    let line = format_gpu_device_row(&device);
    assert!(line.contains("GPU #0"), "{line}");
    assert!(line.contains("42.0%"), "{line}");
    assert!(line.contains("1.0 GiB"), "{line}");
}

/// Honesty: a GPU device with unobserved values renders dashes, not fabricated zeroes.
#[test]
fn format_gpu_device_row_keeps_missing_honest() {
    let _guard = en();
    let device = ProcessGpuDevice {
        device_id: "pci:0000:03:00.0".into(),
        memory_bytes: None,
        utilization_pct: None,
        engine_time_ns: None,
    };
    let line = format_gpu_device_row(&device);
    assert!(line.contains("GPU #pci:0000:03:00.0"), "{line}");
    assert!(line.contains("—"), "{line}");
    assert!(
        !line.contains("0.0%"),
        "missing utilization must not fabricate 0%"
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
                wchan: None,
                run_queue_wait_ns: None,
                wait_kind: None,
            },
            ProcessThreadInfo {
                tid: 101,
                comm: "worker".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
                wchan: None,
                run_queue_wait_ns: None,
                wait_kind: None,
            },
            ProcessThreadInfo {
                tid: 102,
                comm: "extra-1".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
                wchan: None,
                run_queue_wait_ns: None,
                wait_kind: None,
            },
            ProcessThreadInfo {
                tid: 103,
                comm: "extra-2".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: None,
                cpu_percent: None,
                wchan: None,
                run_queue_wait_ns: None,
                wait_kind: None,
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
    // A thread whose `stat` lacked parseable CPU counters keeps its own row
    // with the explicit dash in both CPU columns — never a fabricated
    // `0.0s` / `0.0%`.
    let dash = missing_value();
    assert!(
        text.contains(&format!("101  worker  S  {dash}  {dash}")),
        "a thread without parsed CPU counters must render both dashes:\n{text}"
    );
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
                deleted: false,
            },
            OpenFileEntry {
                fd: 3,
                kind: OpenFileKind::Socket,
                target: Some("socket:[4242]".into()),
                deleted: false,
            },
            OpenFileEntry {
                fd: 9,
                kind: OpenFileKind::Other,
                target: None,
                deleted: false,
            },
            OpenFileEntry {
                fd: 10,
                kind: OpenFileKind::Pipe,
                target: Some("pipe:[5]".into()),
                deleted: false,
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
    assert!(text.contains("0 [file] → /dev/null"));
    assert!(text.contains("9 [other] → unreadable"));
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

/// An empty environment renders the explicit empty state.
#[test]
fn environment_preview_renders_empty_state_for_no_entries() {
    let _guard = en();
    let env = ProcessEnvironment::default();
    let text = render_text(environment_preview_lines(&env, TuiTheme::default()));
    assert!(
        text.contains("No environment variables observed"),
        "empty state must render, got:\n{text}"
    );
    assert!(
        !text.contains("Environment variables 0"),
        "empty environment must not fabricate a count title"
    );
}

/// The environment preview renders the count title, the first key=value entries,
/// and an honest "…" when more remain or entries were truncated.
#[test]
fn environment_preview_renders_count_and_bounded_entries_with_truncation() {
    let _guard = en();
    let env = ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: None,
        entries: vec![
            ProcessEnvironmentEntry {
                key: "FOO".into(),
                value: "BAR".into(),
            },
            ProcessEnvironmentEntry {
                key: "PATH".into(),
                value: "/usr/bin".into(),
            },
            ProcessEnvironmentEntry {
                key: "USER".into(),
                value: "alice".into(),
            },
            ProcessEnvironmentEntry {
                key: "SHELL".into(),
                value: "/bin/bash".into(),
            },
        ],
        truncated_count: 5,
    };
    let text = render_text(environment_preview_lines(&env, TuiTheme::default()));
    assert!(
        text.contains("Environment variables 4"),
        "count title must render, got:\n{text}"
    );
    assert!(text.contains("FOO=BAR"), "first entry must render");
    assert!(text.contains("PATH=/usr/bin"), "second entry must render");
    assert!(text.contains("USER=alice"), "third entry must render");
    assert!(
        !text.contains("SHELL=/bin/bash"),
        "fourth entry beyond bound must not render"
    );
    assert!(text.contains('…'), "ellipsis must render when truncated");
}

/// Full `insights_lines` pipeline verifies Environment and enhanced GPU facets
/// across Pending, Unavailable, and Current states.
#[test]
fn insights_lines_renders_environment_and_gpu_facets() {
    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(100, "proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("snapshot exists");

    // 1. Pending environment renders collecting line
    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection.clone()))),
    );
    let text = render_text(insights_lines(&app, TuiTheme::default(), 100));
    assert!(text.contains("Loading process insights"), "{text}");

    // 2. Unavailable environment renders honest permission denied
    projection.environment = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection.clone()))),
    );
    let text = render_text(insights_lines(&app, TuiTheme::default(), 100));
    assert!(text.contains("Permission denied"), "{text}");

    // 3. Current environment (empty) renders "No environment variables observed"
    projection.environment = ProcessInsightFacetState::Current(ProcessEnvironment::default());
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection.clone()))),
    );
    let text = render_text(insights_lines(&app, TuiTheme::default(), 100));
    assert!(text.contains("No environment variables observed"), "{text}");

    // 4. Current environment (populated) and enhanced GPU with device_id and engines
    projection.environment = ProcessInsightFacetState::Current(ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: None,
        entries: vec![
            ProcessEnvironmentEntry {
                key: "VAR_A".into(),
                value: "val_a".into(),
            },
            ProcessEnvironmentEntry {
                key: "VAR_B".into(),
                value: "val_b".into(),
            },
        ],
        truncated_count: 0,
    });
    projection.gpu = ProcessInsightFacetState::Current(ProcessGpuSnapshot {
        state: DeviceState::healthy(1),
        devices: vec![ProcessGpuDevice {
            device_id: "0".into(),
            memory_bytes: Some(2 * 1024 * 1024 * 1024),
            utilization_pct: Some(55.0),
            engine_time_ns: None,
        }],
        engines: ProcessGpuEngines {
            state: DeviceState::healthy(1),
            engines: vec![ProcessGpuEngineUsage {
                name: "3d".into(),
                usage_pct: ScalarObservation::available(22.0, 1),
                engine_time_ns: ScalarObservation::available(1_500_000_000, 1),
                engine_cycles: ScalarObservation::default(),
            }],
        },
    });
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );
    let text = render_text(insights_lines(&app, TuiTheme::default(), 100));
    assert!(text.contains("Environment variables 2"), "{text}");
    assert!(text.contains("VAR_A=val_a"), "{text}");
    assert!(text.contains("VAR_B=val_b"), "{text}");
    assert!(text.contains("GPU #0"), "{text}");
    assert!(text.contains("55.0%"), "{text}");
    assert!(text.contains("2.0 GiB"), "{text}");
    assert!(text.contains("3d"), "{text}");
    assert!(text.contains("22.0%"), "{text}");
    assert!(text.contains("1.5s"), "{text}");
}

/// Environment preview escapes newlines, carriage returns, and formats empty/spaced values honestly.
#[test]
fn environment_preview_escapes_newlines_and_formats_empty_values() {
    let _guard = en();
    let env = ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: None,
        entries: vec![
            ProcessEnvironmentEntry {
                key: "MULTILINE".into(),
                value: "line1\nline2".into(),
            },
            ProcessEnvironmentEntry {
                key: "CRLF".into(),
                value: "val1\r\nval2".into(),
            },
            ProcessEnvironmentEntry {
                key: "EMPTY".into(),
                value: "".into(),
            },
        ],
        truncated_count: 0,
    };
    let text = render_text(environment_preview_lines(&env, TuiTheme::default()));
    assert!(text.contains("Environment variables 3"), "{text}");
    assert!(
        text.contains("MULTILINE=line1\\nline2"),
        "newlines must be escaped: {text}"
    );
    assert!(
        text.contains("CRLF=val1\\r\\nval2"),
        "CRLF must be escaped: {text}"
    );
    assert!(
        text.contains("EMPTY="),
        "empty values must format without space: {text}"
    );
    assert!(
        !text.contains('…'),
        "no ellipsis when exactly at preview bound and untruncated"
    );
}

/// Truncation ellipsis renders when truncated_count > 0 even if entry count is under preview bound.
#[test]
fn environment_preview_ellipsis_when_truncated_count_positive() {
    let _guard = en();
    let env = ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: None,
        entries: vec![ProcessEnvironmentEntry {
            key: "ONLY_ONE".into(),
            value: "value".into(),
        }],
        truncated_count: 10,
    };
    let text = render_text(environment_preview_lines(&env, TuiTheme::default()));
    assert!(text.contains("Environment variables 1"), "{text}");
    assert!(text.contains("ONLY_ONE=value"), "{text}");
    assert!(
        text.contains('…'),
        "ellipsis must render for positive truncated_count"
    );
}

/// GPU device row handles partial observations: missing VRAM with present utilization,
/// and missing utilization with present VRAM.
#[test]
fn format_gpu_device_row_partial_observations_render_dashes_honestly() {
    let _guard = en();
    let util_only = ProcessGpuDevice {
        device_id: "card0".into(),
        memory_bytes: None,
        utilization_pct: Some(78.4),
        engine_time_ns: None,
    };
    let line1 = format_gpu_device_row(&util_only);
    assert!(line1.contains("GPU #card0"), "{line1}");
    assert!(line1.contains("78.4%"), "{line1}");
    assert!(
        line1.contains("· VRAM in use —"),
        "missing VRAM must show dash: {line1}"
    );

    let vram_only = ProcessGpuDevice {
        device_id: "card1".into(),
        memory_bytes: Some(512 * 1024 * 1024),
        utilization_pct: None,
        engine_time_ns: None,
    };
    let line2 = format_gpu_device_row(&vram_only);
    assert!(line2.contains("GPU #card1"), "{line2}");
    assert!(
        line2.contains("— ·"),
        "missing util must show dash: {line2}"
    );
    assert!(line2.contains("512.0 MiB"), "{line2}");
}

/// format_engine_time formats nanoseconds as seconds with one decimal precision.
#[test]
fn format_engine_time_scales_correctly() {
    assert_eq!(format_engine_time(0), "0.0s");
    assert_eq!(format_engine_time(500_000_000), "0.5s");
    assert_eq!(format_engine_time(1_000_000_000), "1.0s");
    assert_eq!(format_engine_time(12_345_000_000), "12.3s");
}

/// format_engine_cycles formats cycle counts across scale boundaries.
#[test]
fn format_engine_cycles_scales_across_ranges() {
    assert_eq!(format_engine_cycles(0), "0 cycles");
    assert_eq!(format_engine_cycles(999_999), "999999 cycles");
    assert_eq!(format_engine_cycles(1_000_000), "1.0M cycles");
    assert_eq!(format_engine_cycles(12_340_000), "12.3M cycles");
    assert_eq!(format_engine_cycles(1_000_000_000), "1.00G cycles");
    assert_eq!(format_engine_cycles(5_432_100_000), "5.43G cycles");
}

/// format_engine_usage_line handles time-over-cycles precedence and missing-counter dashes.
#[test]
fn format_engine_usage_line_precedence_and_missing_counters() {
    // 1. When engine_time_ns is present, it takes precedence over engine_cycles
    let both_present = ProcessGpuEngineUsage {
        name: "compute".into(),
        usage_pct: ScalarObservation::available(50.0, 1),
        engine_time_ns: ScalarObservation::available(3_000_000_000, 1),
        engine_cycles: ScalarObservation::available(999_999_999, 1),
    };
    let line = format_engine_usage_line(&both_present);
    assert!(line.contains("compute"), "{line}");
    assert!(line.contains("50.0%"), "{line}");
    assert!(
        line.contains("3.0s"),
        "engine_time_ns must take precedence: {line}"
    );
    assert!(
        !line.contains("cycles"),
        "cycles must be superseded by time: {line}"
    );

    // 2. When usage is present but neither time nor cycles is observed, cumulative shows dash
    let usage_only = ProcessGpuEngineUsage {
        name: "copy".into(),
        usage_pct: ScalarObservation::available(10.0, 1),
        engine_time_ns: ScalarObservation::default(),
        engine_cycles: ScalarObservation::default(),
    };
    let line2 = format_engine_usage_line(&usage_only);
    assert!(line2.contains("copy  10.0%  —"), "{line2}");

    // 3. When cold-start and cycles in millions are observed
    let cycles_m = ProcessGpuEngineUsage {
        name: "blit".into(),
        usage_pct: ScalarObservation::default(),
        engine_time_ns: ScalarObservation::default(),
        engine_cycles: ScalarObservation::available(42_500_000, 1),
    };
    let line3 = format_engine_usage_line(&cycles_m);
    assert!(line3.contains("blit  —  42.5M cycles"), "{line3}");
}

/// Dangerous capabilities (CAP_SYS_ADMIN, CAP_SYS_PTRACE, etc.) are correctly identified
/// regardless of case and CAP_ prefix.
#[test]
fn is_dangerous_capability_identifies_high_privilege_caps() {
    // CAP_SYS_ADMIN variants
    assert!(is_dangerous_capability("CAP_SYS_ADMIN"));
    assert!(is_dangerous_capability("cap_sys_admin"));
    assert!(is_dangerous_capability("SYS_ADMIN"));
    assert!(is_dangerous_capability("sys_admin"));

    // CAP_SYS_PTRACE variants
    assert!(is_dangerous_capability("CAP_SYS_PTRACE"));
    assert!(is_dangerous_capability("cap_sys_ptrace"));
    assert!(is_dangerous_capability("SYS_PTRACE"));
    assert!(is_dangerous_capability("sys_ptrace"));

    // Other dangerous capabilities
    assert!(is_dangerous_capability("CAP_SYS_RAWIO"));
    assert!(is_dangerous_capability("CAP_SYS_MODULE"));
    assert!(is_dangerous_capability("CAP_NET_ADMIN"));
    assert!(is_dangerous_capability("CAP_DAC_OVERRIDE"));

    // Safe cgroup controllers / capabilities
    assert!(!is_dangerous_capability("cpu"));
    assert!(!is_dangerous_capability("memory"));
    assert!(!is_dangerous_capability("pids"));
    assert!(!is_dangerous_capability("io"));
    assert!(!is_dangerous_capability("CAP_AUDIT_WRITE"));
}

/// format_capability_row adds visible warning markers for dangerous capabilities.
#[test]
fn format_capability_row_highlights_dangerous_capabilities() {
    let dangerous_admin = format_capability_row("CAP_SYS_ADMIN");
    assert!(
        dangerous_admin.contains("[!]") && dangerous_admin.contains('\u{26a0}'),
        "dangerous capability must contain visible warning marker: {dangerous_admin}"
    );
    assert!(dangerous_admin.contains("CAP_SYS_ADMIN"));

    let dangerous_ptrace = format_capability_row("CAP_SYS_PTRACE");
    assert!(
        dangerous_ptrace.contains("[!]") && dangerous_ptrace.contains('\u{26a0}'),
        "dangerous capability must contain visible warning marker: {dangerous_ptrace}"
    );
    assert!(dangerous_ptrace.contains("CAP_SYS_PTRACE"));

    let safe = format_capability_row("cpu");
    assert!(
        !safe.contains("[!]") && !safe.contains('\u{26a0}'),
        "safe capability must not contain warning marker: {safe}"
    );
    assert_eq!(safe, "cpu");
}

/// format_capability_line styles dangerous capabilities with theme.warn and Modifier::BOLD.
#[test]
fn format_capability_line_styles_dangerous_with_warning() {
    let theme = TuiTheme::default();
    let line = format_capability_line("CAP_SYS_ADMIN", theme);
    let rendered = render_text(vec![line]);
    assert!(
        rendered.contains("[!]")
            && rendered.contains('\u{26a0}')
            && rendered.contains("CAP_SYS_ADMIN"),
        "rendered line must contain marker and cap: {rendered}"
    );

    let safe_line = format_capability_line("memory", theme);
    let safe_rendered = render_text(vec![safe_line]);
    assert!(
        !safe_rendered.contains("[!]") && !safe_rendered.contains('\u{26a0}'),
        "safe line must not have marker: {safe_rendered}"
    );
    assert!(safe_rendered.contains("memory"));
}

/// Capabilities preview renders the count title, highlights dangerous capabilities,
/// and truncates with ellipsis beyond preview limit.
#[test]
fn capabilities_preview_renders_header_rows_and_ellipsis() {
    let _guard = en();
    let caps = vec![
        "cpu".to_string(),
        "CAP_SYS_ADMIN".to_string(),
        "CAP_SYS_PTRACE".to_string(),
        "extra_cap".to_string(),
    ];
    let text = render_text(capabilities_preview_lines_with_limit(
        &caps,
        TuiTheme::default(),
        3,
    ));
    assert!(
        text.contains("Capabilities 4"),
        "count title must render, got:\n{text}"
    );
    assert!(text.contains("cpu"), "safe capability must render: {text}");
    assert!(
        text.contains("[!]") && text.contains("CAP_SYS_ADMIN"),
        "CAP_SYS_ADMIN must render with warning marker: {text}"
    );
    assert!(
        text.contains("[!]") && text.contains("CAP_SYS_PTRACE"),
        "CAP_SYS_PTRACE must render with warning marker: {text}"
    );
    assert!(
        text.contains('…'),
        "ellipsis must render when truncated: {text}"
    );
    assert!(
        !text.contains("extra_cap"),
        "truncated item beyond bound must not render: {text}"
    );
}

/// Full `insights_lines` pipeline verifies that dangerous capabilities in resource groups
/// are highlighted with visible warning markers.
#[test]
fn insights_lines_renders_capabilities_with_warning_markers() {
    use taskmanager_core::core::identity::ProviderId;
    use taskmanager_core::core::process_telemetry::{
        ProcessResourceObservations, ProcessResourceSnapshot, ResourceGroupMembership,
        ResourceObservation,
    };

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(200, "priv-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("snapshot exists");

    let groups = vec![ResourceGroupMembership {
        provider: ProviderId::borrowed("fixture"),
        native_hierarchy_id: Some(1),
        capabilities: vec![
            "cpu".into(),
            "CAP_SYS_ADMIN".into(),
            "CAP_SYS_PTRACE".into(),
        ],
        native_locator: "cgroup:/system.slice/test.service".into(),
    }];
    let resource_snapshot = ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(1),
        ProcessResourceObservations {
            resource_groups: ResourceObservation::current(groups, 1),
            ..ProcessResourceObservations::default()
        },
        Vec::new(),
    );
    projection.resources = ProcessInsightFacetState::Current(resource_snapshot);

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );
    let text = render_text(insights_lines(&app, TuiTheme::default(), 200));

    assert!(
        text.contains("Resource group"),
        "resource group locator must render: {text}"
    );
    assert!(
        text.contains("Capabilities 3"),
        "capabilities count must render: {text}"
    );
    assert!(
        text.contains("[!]") && text.contains("CAP_SYS_ADMIN"),
        "CAP_SYS_ADMIN must render with warning marker: {text}"
    );
    assert!(
        text.contains("[!]") && text.contains("CAP_SYS_PTRACE"),
        "CAP_SYS_PTRACE must render with warning marker: {text}"
    );
    assert!(text.contains("cpu"), "safe controller must render: {text}");
}

#[test]
fn insights_lines_renders_cpu_affinity_when_observed() {
    use taskmanager_application::ProcessAffinityReady;

    let _guard = en();
    let target = FrozenProcessIdentity::from_authoritative_parts(300, "aff-proc", 1000, 1000)
        .expect("valid target");
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    let projection = tracker.snapshot().expect("snapshot exists");

    let mut app = crate::demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );

    // Seed affinity ready state
    let ready = ProcessAffinityReady {
        target,
        cpus: vec![0, 1, 2, 3],
        request_id: RequestId::new(1).expect("valid request id"),
    };
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessAffinity(Some(ready)),
    );

    let text = render_text(insights_lines(&app, TuiTheme::default(), 300));
    assert!(
        text.contains("Affinity"),
        "must render Affinity label: {text}"
    );
    assert!(
        text.contains("CPUs 0, 1, 2, 3") || text.contains("4 /") || text.contains("All"),
        "must render CPUs list: {text}"
    );
}
