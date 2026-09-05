//! TUI-003 binding matrix: every entry of the TUI-local command registry
//! ([`crate::command_palette::TUI_LOCAL_COMMANDS`]) is driven through the real
//! `handle_key` path with its declared chord and proven to produce its
//! observable effect, and the help/palette surfaces are pinned to the same
//! declaration. The registry is the single authority for these chords —
//! a hand-written `match` on one of them in `runtime/keys.rs` is drift.
//!
//! Acceptance clauses deliberately NOT duplicated here (already covered by
//! dedicated suites): the gated Confirm `y` / `n` alternative paths
//! (`control_feedback.rs`, `process_batch.rs`, `service_control.rs`,
//! `session_control.rs`, `startup_control.rs`, `seam_tests.rs`) and the
//! structured no-sidebar / no-alerts / no-Enter-confirm reasons
//! (`bindings_tests.rs`, `ui/help_tests.rs`,
//! `page_navigation.rs::f9_is_consumed_without_creating_a_terminal_sidebar`).

use super::super::*;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::process::ProcessLiveKey;

use crate::command_palette::{TUI_LOCAL_COMMANDS, TuiDirectScope};

fn press_char(app: &mut crate::TuiApp, character: char) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(character),
            KeyModifiers::NONE,
        ),
    )
}

fn press_enter(app: &mut crate::TuiApp) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    )
}

fn select_page(app: &mut crate::TuiApp, page: AppPage) {
    let _ = app.apply_action(AppAction::SelectPage(page));
}

fn app_on_processes() -> crate::TuiApp {
    let mut app = crate::demo_app();
    select_page(&mut app, AppPage::Applications);
    // The demo frame ships a "Demo snapshot" banner notice; state assertions
    // below observe their own command's notice, so start from a clean slate.
    app.shell.clear_feedback_notice();
    app
}

fn app_on_device(page: AppPage, device: crate::PerfDevice) -> crate::TuiApp {
    let mut app = crate::demo_app();
    select_page(&mut app, page);
    app.perf_device = device;
    // The demo frame ships a "Demo snapshot" banner notice; state assertions
    // below observe their own command's notice, so start from a clean slate.
    app.shell.clear_feedback_notice();
    app
}

// ── registry integrity ────────────────────────────────────────────────────

/// One chord, one declaring layer: no registry chord may duplicate another
/// registry entry or a shell-owned character.
#[test]
fn registry_shortcuts_are_unique_and_disjoint_from_the_shell_layer() {
    let mut seen: Vec<&str> = Vec::new();
    for command in TUI_LOCAL_COMMANDS {
        assert!(
            !command.binding.shortcut.is_empty(),
            "an empty chord advertises nothing"
        );
        assert!(
            !seen.contains(&command.binding.shortcut),
            "chord {:?} is declared twice inside the TUI registry",
            command.binding.shortcut
        );
        seen.push(command.binding.shortcut);
        assert!(
            !taskmanager_shell::shell_local_bindings()
                .iter()
                .any(|binding| binding.shortcut == command.binding.shortcut),
            "chord {:?} is declared by BOTH the shell layer and the TUI registry",
            command.binding.shortcut
        );
    }
}

/// Every advertised chord executes: an entry with no direct arm would render
/// a help/palette row for a key that does nothing (the exact drift this
/// registry exists to prevent).
#[test]
fn every_registry_entry_wires_at_least_one_direct_arm() {
    for command in TUI_LOCAL_COMMANDS {
        assert!(
            !command.direct.is_empty(),
            "advertised chord {:?} has no direct dispatch arm",
            command.binding.shortcut
        );
    }
}

/// The two token rows (range digits, `Enter`) declare token scopes; every
/// other row is a single literal character chord.
#[test]
fn token_rows_declare_token_scopes_and_literal_rows_declare_char_chords() {
    for command in TUI_LOCAL_COMMANDS {
        match command.binding.shortcut {
            crate::command_palette::ROW_TARGET_SHORTCUT => assert!(
                command
                    .direct
                    .iter()
                    .all(|arm| matches!(arm.scope, TuiDirectScope::RowTarget(_))),
                "the Enter row must declare only row-target arms"
            ),
            crate::command_palette::RESOURCE_DIGITS_SHORTCUT => assert!(
                command
                    .direct
                    .iter()
                    .all(|arm| arm.scope == TuiDirectScope::PerformanceResourceDigit),
                "the digit-range row must declare only resource-digit arms"
            ),
            literal => assert_eq!(
                literal.chars().count(),
                1,
                "unexpected non-char token row {literal:?}"
            ),
        }
    }
}

// ── global overlay commands ───────────────────────────────────────────────

#[test]
fn p_opens_the_settings_surface() {
    let mut app = crate::demo_app();
    assert!(!app.settings_open());
    let effect = press_char(&mut app, 'p');
    assert!(effect.is_none(), "settings opens without platform work");
    assert!(
        app.settings_open(),
        "the declared p chord must open settings"
    );
}

#[test]
fn i_opens_the_about_overlay() {
    let mut app = crate::demo_app();
    assert!(!app.about_open());
    let _ = press_char(&mut app, 'i');
    assert!(app.about_open(), "the declared i chord must open about");
}

#[test]
fn h_opens_the_health_overlay() {
    let mut app = crate::demo_app();
    assert!(!app.health_open());
    let _ = press_char(&mut app, 'h');
    assert!(app.health_open(), "the declared h chord must open health");
}

#[test]
fn c_opens_the_containers_overlay() {
    let mut app = crate::demo_app();
    assert!(!app.containers_open());
    let _ = press_char(&mut app, 'c');
    assert!(
        app.containers_open(),
        "the declared c chord must open containers"
    );
}

#[test]
fn x_reports_the_snapshot_export_feedback() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    app.shell.clear_feedback_notice();
    let effect = press_char(&mut app, 'x');
    assert!(
        effect.is_none(),
        "export routes through the app-host worker"
    );
    let feedback = app.feedback_notice().expect("export feedback");
    assert_eq!(
        feedback.source(),
        taskmanager_shell::FeedbackSource::Persistence,
        "the declared x chord must reach the export path"
    );
}

// ── the row-target Enter ─────────────────────────────────────────────────

#[test]
fn enter_opens_the_declared_row_target_on_each_page() {
    let mut services = crate::demo_app();
    select_page(&mut services, AppPage::Services);
    let _ = press_enter(&mut services);
    assert_eq!(
        services.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ServiceMenu),
        "Enter on Services must open the service-action menu"
    );

    let mut users = crate::demo_app();
    select_page(&mut users, AppPage::Users);
    let _ = press_enter(&mut users);
    assert_eq!(
        users.local_surface_kind(),
        Some(crate::TuiSurfaceKind::SessionMenu),
        "Enter on Users must open the session-action menu"
    );

    let mut startup = crate::demo_app();
    select_page(&mut startup, AppPage::Startup);
    let _ = press_enter(&mut startup);
    assert_eq!(
        startup.local_surface_kind(),
        Some(crate::TuiSurfaceKind::StartupMenu),
        "Enter on Startup must open the startup-action menu"
    );

    let mut processes = app_on_processes();
    let _ = press_enter(&mut processes);
    assert!(
        processes.process_properties().is_some(),
        "Enter on Applications must open process properties"
    );
}

#[test]
fn enter_is_inert_on_pages_without_a_row_target() {
    let mut app = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let effect = press_enter(&mut app);
    assert!(effect.is_none());
    assert!(app.local_surface_kind().is_none(), "no target, no surface");
}

// ── the Performance resource digits ──────────────────────────────────────

#[test]
fn digits_select_the_declared_performance_resource() {
    let mut app = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let effect = press_char(&mut app, '3');
    assert!(effect.is_none(), "resource selection is local state only");
    assert_eq!(
        app.perf_device,
        crate::PerfDevice::Disk,
        "the declared digit chord must select the matching resource"
    );
    // A digit beyond the visible resource list is honestly inert.
    let device = app.perf_device;
    let _ = press_char(&mut app, '9');
    assert_eq!(app.perf_device, device, "an unlisted digit changes nothing");
}

#[test]
fn digits_do_not_switch_resources_off_the_performance_page() {
    let mut app = app_on_processes();
    let _ = press_char(&mut app, '3');
    assert_eq!(
        app.page(),
        AppPage::Applications,
        "the Applications page keeps the digit for the prefix jump"
    );
    let on_performance = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    assert_eq!(
        on_performance.select_perf_device_digit('3'),
        Some(crate::PerfDevice::Disk),
        "the same chord on Performance does switch — the scope, not the digit, decides"
    );
}

// ── Applications-page commands ───────────────────────────────────────────

#[test]
fn capital_c_opens_the_column_menu_only_on_applications() {
    let mut app = app_on_processes();
    let _ = press_char(&mut app, 'C');
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ColumnMenu),
        "the declared C chord must open the column menu"
    );

    let mut services = crate::demo_app();
    select_page(&mut services, AppPage::Services);
    let _ = press_char(&mut services, 'C');
    assert_ne!(
        services.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ColumnMenu),
        "the column menu is Applications-scoped"
    );
}

#[test]
fn m_toggles_the_marked_batch_set_of_the_selected_row() {
    let mut app = app_on_processes();
    let selected = app.selected_detail_process().expect("a selected row");
    let expected = ProcessLiveKey::from_process(&selected).expect("live identity");
    assert!(app.shell.selected_identities().is_empty());
    let _ = press_char(&mut app, 'm');
    assert!(
        app.shell.selected_identities().contains(&expected),
        "the declared m chord must mark the selected process"
    );
    assert!(
        app.feedback_text().contains("marked for batch control"),
        "the notice names the batch intent: {}",
        app.feedback_text()
    );
    let _ = press_char(&mut app, 'm');
    assert!(
        app.shell.selected_identities().is_empty(),
        "the same chord toggles the mark off"
    );
    assert!(app.feedback_text().contains("Selection cleared"));
}

#[test]
fn capital_b_opens_the_batch_menu_only_with_marked_rows() {
    let mut app = app_on_processes();
    app.shell.clear_feedback_notice();
    let _ = press_char(&mut app, 'B');
    assert_ne!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::BatchMenu),
        "an empty marked set must not open a dead-end menu"
    );
    assert!(
        app.feedback_text().contains("No processes marked"),
        "the honest reason is reported: {}",
        app.feedback_text()
    );

    let _ = press_char(&mut app, 'm');
    let _ = press_char(&mut app, 'B');
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::BatchMenu),
        "with a marked row the declared B chord opens the batch menu"
    );
}

#[test]
fn a_opens_the_process_action_menu_only_on_applications() {
    let mut app = app_on_processes();
    let _ = press_char(&mut app, 'a');
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ProcessMenu),
        "the declared a chord must open the process-action menu"
    );

    let mut services = crate::demo_app();
    select_page(&mut services, AppPage::Services);
    let _ = press_char(&mut services, 'a');
    assert_ne!(
        services.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ProcessMenu),
        "the process menu is Applications-scoped"
    );
}

#[test]
fn y_copies_the_selected_row_through_the_declared_chord() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = app_on_processes();
    let pid = app.selected_detail_process().expect("a selected row").pid;
    let effect = press_char(&mut app, 'y');
    assert!(effect.is_none(), "the clipboard write is a local effect");
    assert!(
        app.feedback_text().contains(&pid.to_string()),
        "the declared y chord must copy the selected identity: {}",
        app.feedback_text()
    );
}

// ── Services-page commands ───────────────────────────────────────────────

#[test]
fn o_opens_the_service_log_only_on_services_and_only_once() {
    let mut app = crate::demo_app();
    app.shell.clear_feedback_notice();
    select_page(&mut app, AppPage::Services);
    let _ = press_char(&mut app, 'o');
    assert!(
        app.shell.service_log.is_some(),
        "the declared o chord must open the service log"
    );
    let opened = app.shell.service_log.is_some();
    let _ = press_char(&mut app, 'o');
    assert_eq!(
        app.shell.service_log.is_some(),
        opened,
        "with the panel up, the chord is inert (the panel owns its keys)"
    );

    let mut processes = app_on_processes();
    let _ = press_char(&mut processes, 'o');
    assert!(
        processes.shell.service_log.is_none(),
        "the service log is Services-scoped"
    );
}

// ── Performance·GPU commands ─────────────────────────────────────────────

#[test]
fn e_requests_the_gpu_engine_rows_on_the_gpu_device() {
    let mut app = app_on_device(AppPage::Performance, crate::PerfDevice::Gpu);
    let effect = press_char(&mut app, 'e');
    assert!(
        effect.is_some(),
        "the declared e chord must submit the typed engine-rows request"
    );

    let mut cpu = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let effect = press_char(&mut cpu, 'e');
    assert!(effect.is_none());
    assert!(
        cpu.feedback_notice().is_none(),
        "the engine-rows arm is GPU-scoped and must stay silent on the CPU device"
    );
}

#[test]
fn e_exports_service_log_on_services_page_when_log_open() {
    let mut app = crate::demo_app();
    app.shell.clear_feedback_notice();
    select_page(&mut app, AppPage::Services);
    let scratch = crate::ui::test_support::repo_temp_dir().join(format!(
        "svc-log-matrix-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    app.export_dir = Some(scratch.clone());

    // When closed, e does not export
    let effect = press_char(&mut app, 'e');
    assert!(effect.is_none());
    assert!(app.feedback_notice().is_none());

    // Open service log
    let _ = press_char(&mut app, 'o');
    assert!(app.shell.service_log.is_some());

    // When open, e triggers export
    let effect = press_char(&mut app, 'e');
    assert!(effect.is_none());
    assert!(app.feedback_notice().is_some());

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn g_cycles_the_gpu_chart_metric_only_on_the_gpu_device() {
    let mut app = app_on_device(AppPage::Performance, crate::PerfDevice::Gpu);
    let before = app.shell.gpu_chart_metric_selected();
    let effect = press_char(&mut app, 'g');
    assert!(effect.is_none(), "the metric cycle is local shell state");
    assert_ne!(
        app.shell.gpu_chart_metric_selected(),
        before,
        "the declared g chord must advance the chart metric"
    );
    let _ = press_char(&mut app, 'g');
    assert_ne!(
        app.shell.gpu_chart_metric_selected(),
        before,
        "a second press keeps cycling through the available families"
    );

    let mut cpu = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let before = cpu.shell.gpu_chart_metric_selected();
    let _ = press_char(&mut cpu, 'g');
    assert_eq!(
        cpu.shell.gpu_chart_metric_selected(),
        before,
        "the metric cycle is GPU-scoped"
    );
}

// ── Performance·Disk command ─────────────────────────────────────────────

#[test]
fn d_toggles_the_directory_scan_only_on_the_disk_device() {
    let mut app = app_on_device(AppPage::Performance, crate::PerfDevice::Disk);
    let effect = press_char(&mut app, 'd');
    assert!(
        effect.is_some(),
        "the declared d chord must submit the typed directory-usage request"
    );

    let mut cpu = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let effect = press_char(&mut cpu, 'd');
    assert!(effect.is_none(), "the scan trigger is Disk-scoped");
}

// ── help / palette parity with the registry ──────────────────────────────

/// Matrix clauses (b) and (c): every registry row appears in the palette with
/// exactly the declared executability, and in the help overlay with the
/// declared label (the English catalog's localized copy equals the registry's
/// const label — or falls back to it verbatim).
#[test]
fn palette_and_help_carry_every_registry_row_with_declared_executability() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let palette = crate::TuiApp::palette_rows();
    let help = crate::ui::help::help_rows();
    for command in TUI_LOCAL_COMMANDS {
        let shortcut = command.binding.shortcut;
        // The shared router's `Enter` (Properties) legitimately coexists with
        // the registry's row-target `Enter`, so a row is identified by its
        // full declared (shortcut, label) pair, not by the chord alone.
        let matches_declaration = |row: &&crate::CommandPaletteRow| {
            row.shortcut == shortcut && row.label == command.binding.label
        };
        let row = palette
            .iter()
            .find(matches_declaration)
            .unwrap_or_else(|| panic!("the palette must list the declared row {shortcut:?}"));
        assert_eq!(
            row.local_action, command.palette_action,
            "palette executability must equal the registry declaration for {shortcut:?}"
        );
        help.iter()
            .find(|row| row.shortcut == shortcut && row.label == command.binding.label)
            .unwrap_or_else(|| panic!("help must advertise the declared row {shortcut:?}"));
    }
}

/// The palette lane respects the same context guards as the direct keys: a
/// declared action executed from the palette never succeeds where the chord
/// itself would refuse.
#[test]
fn palette_local_actions_respect_the_declared_scopes() {
    use crate::PaletteLocalAction;
    let mut services = crate::demo_app();
    services.run_palette_local_action(Some(PaletteLocalAction::OpenServiceLog));
    assert!(
        services.shell.service_log.is_none(),
        "off the Services page the palette log action must stay inert"
    );
    select_page(&mut services, AppPage::Services);
    services.run_palette_local_action(Some(PaletteLocalAction::OpenServiceLog));
    assert!(
        services.shell.service_log.is_some(),
        "on Services the palette runs the same log action the o key does"
    );
    services.run_palette_local_action(Some(PaletteLocalAction::ExportServiceLog));
    assert!(
        services.feedback_notice().is_some(),
        "with service log open, palette export action triggers export"
    );

    let mut gpu = app_on_device(AppPage::Performance, crate::PerfDevice::Gpu);
    let before = gpu.shell.gpu_chart_metric_selected();
    gpu.run_palette_local_action(Some(PaletteLocalAction::ToggleGpuChartMetric));
    assert_ne!(
        gpu.shell.gpu_chart_metric_selected(),
        before,
        "the palette must cycle the same metric the g key cycles"
    );

    let mut cpu = app_on_device(AppPage::Performance, crate::PerfDevice::Cpu);
    let before = cpu.shell.gpu_chart_metric_selected();
    cpu.run_palette_local_action(Some(PaletteLocalAction::ToggleGpuChartMetric));
    assert_eq!(
        cpu.shell.gpu_chart_metric_selected(),
        before,
        "the palette metric action is GPU-scoped like the direct key"
    );
}
