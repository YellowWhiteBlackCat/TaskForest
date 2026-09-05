//! Crossterm key-normalization, page/search/quit routing, and the Performance
//! resource-digit selector.

use super::super::*;

#[test]
fn crossterm_keys_normalize_into_shared_command_vocabulary() {
    let alt_two = KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('2'),
        KeyModifiers::ALT,
    );
    assert_eq!(
        key_to_terminal(alt_two),
        Some(taskmanager_shell::ShellKeyEvent::new(
            KeyCode::Digit2,
            Modifiers::ALT
        ))
    );
    let back_tab = KeyEvent::new(
        ratatui::crossterm::event::KeyCode::BackTab,
        KeyModifiers::SHIFT,
    );
    assert_eq!(
        key_to_terminal(back_tab),
        Some(taskmanager_shell::ShellKeyEvent::new(
            KeyCode::Tab,
            Modifiers::SHIFT
        ))
    );
    // Home / End reach the shared vocabulary so the router's jump bindings
    // fire from the terminal.
    assert_eq!(
        key_to_terminal(KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Home,
            KeyModifiers::NONE
        )),
        Some(taskmanager_shell::ShellKeyEvent::new(
            KeyCode::Home,
            Modifiers::NONE
        ))
    );
    assert_eq!(
        key_to_terminal(KeyEvent::new(
            ratatui::crossterm::event::KeyCode::End,
            KeyModifiers::NONE
        )),
        Some(taskmanager_shell::ShellKeyEvent::new(
            KeyCode::End,
            Modifiers::NONE
        ))
    );
    // The full fixed-key surface normalizes onto the shared vocabulary: page
    // keys, refresh, navigation, search/quit/sort chords, dialog keys and the
    // F1 help alias.
    let cases = [
        (
            ratatui::crossterm::event::KeyCode::F(1),
            KeyCode::F1,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::F(5),
            KeyCode::F5,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::PageUp,
            KeyCode::PageUp,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyCode::PageDown,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::Delete,
            KeyCode::Delete,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::Enter,
            KeyCode::Enter,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::Esc,
            KeyCode::Escape,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::Tab,
            KeyCode::Tab,
            Modifiers::NONE,
            KeyModifiers::NONE,
        ),
        (
            ratatui::crossterm::event::KeyCode::Char(' '),
            KeyCode::Space,
            Modifiers::CONTROL,
            KeyModifiers::CONTROL,
        ),
        (
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyCode::F,
            Modifiers::CONTROL,
            KeyModifiers::CONTROL,
        ),
        (
            ratatui::crossterm::event::KeyCode::Char('a'),
            KeyCode::A,
            Modifiers::CONTROL,
            KeyModifiers::CONTROL,
        ),
        (
            ratatui::crossterm::event::KeyCode::Char('1'),
            KeyCode::Digit1,
            Modifiers::ALT,
            KeyModifiers::ALT,
        ),
        (
            ratatui::crossterm::event::KeyCode::Char('7'),
            KeyCode::Digit7,
            Modifiers::ALT,
            KeyModifiers::ALT,
        ),
    ];
    for (crossterm, key, shared, crossterm_modifiers) in cases {
        assert_eq!(
            key_to_terminal(KeyEvent::new(crossterm, crossterm_modifiers)),
            Some(taskmanager_shell::ShellKeyEvent::new(key, shared)),
            "crossterm key {crossterm:?} must normalize to {key:?}"
        );
    }
    // Unmapped keys (a mouse-independent terminal can still emit Insert) yield
    // None and never panic.
    assert_eq!(
        key_to_terminal(KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Insert,
            KeyModifiers::NONE
        )),
        None
    );
}

#[test]
fn alt_8_opens_health_and_alerts_overlay() {
    let mut app = crate::demo_app();
    assert!(!app.health_open());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('8'),
            KeyModifiers::ALT,
        ),
    );
    assert!(
        app.health_open(),
        "Alt+8 opens the health and alerts overlay in the TUI"
    );
}

#[test]
fn alt_page_chords_cover_the_four_middle_routes() {
    let mut app = crate::demo_app();
    // Alt+4/5/6 reach System / Startup / Users through the shared router
    // (Alt+1/2/3/7 already covered elsewhere).
    for (character, expected) in [
        ('4', AppPage::System),
        ('5', AppPage::Startup),
        ('6', AppPage::Users),
    ] {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char(character),
                KeyModifiers::ALT,
            ),
        );
        assert_eq!(
            app.page(),
            expected,
            "Alt+{character} must select {expected:?}"
        );
    }
}

#[test]
fn prefix_jump_moves_the_canonical_cursor_to_the_first_name_match_and_extends_within_the_window() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);
    // The demo is sorted by CPU descending; `d` must land on the first name
    // starting with 'd' (dbus-broker; systemd-journald sorts after it on the
    // pid tiebreaker? no — resolve the expected row dynamically so the test
    // never hardcodes a sorted index). The detail scroll resets for the new
    // row, mirroring the arrow paths.
    let first_d = app
        .process_rows_snapshot()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::process_view::ProcessRow::TreeNode { process, .. }
                    if process.name.starts_with('d')
            )
        })
        .expect("demo must contain a 'd' process");
    app.detail_scroll_by(3);
    assert!(app.detail_scroll > 0);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('d'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, first_d);
    assert!(
        app.selected_detail_process()
            .is_some_and(|process| process.name.starts_with('d'))
    );
    assert_eq!(app.detail_scroll, 0);

    // A second key within the window extends the prefix: "db" still matches
    // dbus-broker (the only 'db' name), so the cursor stays put.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('b'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, first_d);
    assert_eq!(app.prefix_jump, "db");
    assert!(app.feedback_text().contains("Jump: db"));

    // "dbz" matches nothing: the cursor stays put, the prefix is retained for
    // the next key in the window.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('z'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.selected, first_d);
    assert_eq!(app.prefix_jump, "dbz");
}

#[test]
fn prefix_jump_is_case_insensitive_and_resets_after_inactivity() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);

    // 'N' matches NetworkManager case-insensitively.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('N'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.name.as_str()),
        Some("NetworkManager")
    );
    assert_eq!(app.prefix_jump, "N");

    // A pause past the window resets the prefix: 'z' then starts a FRESH
    // prefix and jumps to zed instead of extending to "Nz" (no match).
    app.service_log_now_micros = crate::PREFIX_JUMP_WINDOW_MICROS.saturating_add(1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('z'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.name.as_str()),
        Some("zed")
    );
    assert_eq!(app.prefix_jump, "z");
    // 'k' immediately after: still inside the window, "zk" matches nothing,
    // so the cursor stays put and the prefix is retained.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('k'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.selected_detail_process()
            .as_ref()
            .map(|process| process.name.as_str()),
        Some("zed")
    );
    assert_eq!(app.prefix_jump, "zk");
    // Another pause past the window (clock moved at least one full window
    // past the last key), then 'k' alone jumps to kworker.
    app.service_log_now_micros = crate::PREFIX_JUMP_WINDOW_MICROS
        .saturating_mul(2)
        .saturating_add(2);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('k'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.prefix_jump, "k");
    assert!(
        app.selected_detail_process()
            .is_some_and(|process| process.name.starts_with("kworker"))
    );
}

#[test]
fn prefix_jump_lands_on_category_headers() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    app.expanded_groups.clear();

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('u'),
            KeyModifiers::NONE,
        ),
    );
    let rows = app.process_rows_snapshot();
    let label = rows.get(app.selected).and_then(|row| match row {
        crate::process_view::ProcessRow::Group { label, .. } => Some(label.as_str()),
        crate::process_view::ProcessRow::TreeNode { .. } => None,
    });
    assert!(
        matches!(label, Some(value) if value.eq_ignore_ascii_case("Uncategorized")),
        "prefix jump must land on the Uncategorized header, got {label:?}"
    );
    // A group header has no single process, so no insights target is set.
    assert_eq!(app.application.selected_process, None);
}

#[test]
fn prefix_jump_ignores_control_chords_and_non_alphanumeric_characters() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    let selected_before = app.selected;
    // Ctrl+c is consumed by the local containers binding, not the prefix
    // jump — the prefix stays empty and the cursor never moves for a chord.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(app.prefix_jump, "");
    assert_eq!(app.selected, selected_before);
    assert!(app.containers_open(), "Ctrl+c opens the containers overlay");
    app.close_local_overlays();

    // A bare space is not alphanumeric, so it must not start a jump.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(' '),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.prefix_jump, "");
}

#[test]
fn f1_toggles_the_help_overlay_like_the_question_mark_binding() {
    let mut app = crate::demo_app();
    assert!(!app.help_open(), "help starts closed");

    // F1 opens the keyboard-reference overlay (mirrors `?`, which routes
    // through the shell's local bindings).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::F(1), KeyModifiers::NONE),
    );
    assert!(app.help_open(), "F1 must open the help overlay");

    // A second F1 closes it (toggle).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::F(1), KeyModifiers::NONE),
    );
    assert!(!app.help_open(), "F1 must close the help overlay");

    // The normalization path reaches the shared vocabulary so the fixed-key
    // handler can act on it.
    assert_eq!(
        key_to_terminal(KeyEvent::new(
            ratatui::crossterm::event::KeyCode::F(1),
            KeyModifiers::NONE
        )),
        Some(taskmanager_shell::ShellKeyEvent::new(
            KeyCode::F1,
            Modifiers::NONE
        ))
    );
}

#[test]
fn exclusive_input_scopes_swallow_table_navigation_before_content_systems() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Applications;
    app.selected = 1;
    let anchor = app.selected;

    app.open_search();
    for code in [
        ratatui::crossterm::event::KeyCode::Down,
        ratatui::crossterm::event::KeyCode::Home,
        ratatui::crossterm::event::KeyCode::End,
        ratatui::crossterm::event::KeyCode::PageDown,
    ] {
        let _ = handle_key(&mut app, KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(
            app.selected, anchor,
            "search must own {code:?} before table navigation"
        );
    }

    app.close_search();
    app.toggle_help();
    let scroll_before = app.help_scroll;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(
        app.selected, anchor,
        "help scrolling must not move the table"
    );
    assert_eq!(app.help_scroll, scroll_before + 1);
    for code in [
        ratatui::crossterm::event::KeyCode::Home,
        ratatui::crossterm::event::KeyCode::End,
    ] {
        let _ = handle_key(&mut app, KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(
            app.selected, anchor,
            "help must own {code:?} before table navigation"
        );
    }
}

#[test]
fn home_and_end_jump_to_the_category_tree_visual_bounds() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);
    let visible = app.visible_processes().len();
    assert!(visible >= 2, "demo process table must cover a jump");

    let visual = app.visual_row_count();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::End, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, visual - 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Home, KeyModifiers::NONE),
    );
    assert_eq!(app.selected, 0);
}

#[test]
fn demo_runtime_keys_change_pages_search_and_quit_without_platform_work() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    assert!(app.search_active());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.query, "q");
    app.close_search();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('q'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.should_quit());
}

#[test]
fn performance_digit_keys_select_a_resource_without_colliding_with_pages() {
    let mut app = crate::demo_app();
    // The digit rail follows the VISIBLE devices: seed the demo with a
    // battery and a fan so the full seven-resource rail renders (the same
    // fixture enrichment iced's capture demo does).
    let mut battery = taskmanager_core::core::power::BatteryInfo::new(
        "battery:demo:BAT0",
        taskmanager_core::core::device_state::DeviceState::healthy(1),
    );
    battery.status = "Discharging".into();
    battery.apply_scalar_observations(taskmanager_core::core::power::BatteryScalarObservations {
        capacity_pct: taskmanager_core::core::metrics::ScalarObservation::available(80, 1),
        voltage_uv: taskmanager_core::core::metrics::ScalarObservation::available(12_000_000, 1),
        ..Default::default()
    });
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(
            taskmanager_core::core::power::PowerSupplySnapshot {
                state: taskmanager_core::core::device_state::DeviceState::healthy(1),
                timestamp_ms: 1,
                batteries: vec![battery],
                ..Default::default()
            },
        )),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(
            taskmanager_core::core::sensors::SensorCenterSnapshot {
                state: taskmanager_core::core::device_state::DeviceState::healthy(1),
                timestamp_ms: 1,
                readings: vec![
                    taskmanager_core::core::sensors::SensorReading::from_measurement_observation(
                        "hwmon:demo:cpu".into(),
                        "fan1".into(),
                        "CPU Fan".into(),
                        taskmanager_core::core::sensors::SensorMeasurementObservation::available(
                            taskmanager_core::core::sensors::SensorDescriptor::fan_speed(
                                taskmanager_core::core::sensors::SensorScale::IDENTITY,
                            ),
                            taskmanager_core::core::sensors::SensorMagnitude::Unsigned(1200),
                            1,
                        )
                        .expect("valid fan fixture"),
                    ),
                ],
                ..Default::default()
            },
        )),
    );
    assert_eq!(app.page(), AppPage::Performance);
    assert_eq!(app.perf_device, crate::PerfDevice::Cpu);

    // Bare digits 1..=7 select the matching resource on the Performance page.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('3'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Disk);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Gpu);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('6'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Battery);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('7'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Fan);

    // Out-of-range digits ('0', '8') leave the selection untouched.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('0'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Fan);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('8'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Fan);

    // The selector is Performance-scoped: Alt+2 still switches page through
    // the shared router, and a bare digit off the Performance page must not
    // mutate the resource selection.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::Applications);
    let before = app.perf_device;
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('1'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.perf_device, before,
        "digits must not select a resource off the Performance page"
    );
}

#[test]
fn cpu_core_viewport_keys_are_scoped_to_the_cpu_device() {
    let mut app = crate::demo_app();
    assert_eq!(app.page(), AppPage::Performance);
    assert_eq!(app.perf_device, crate::PerfDevice::Cpu);
    assert_eq!(app.cpu_core_scroll, 0);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(app.cpu_core_scroll, 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.cpu_core_scroll > 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Up, KeyModifiers::NONE),
    );
    assert!(app.cpu_core_scroll > 0);

    // Changing the resource resets the resource-local viewport, and the same
    // keys are left to the selected resource instead of mutating hidden CPU
    // state.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Memory);
    assert_eq!(app.cpu_core_scroll, 0);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(app.cpu_core_scroll, 0);
}

#[test]
fn gpu_and_system_viewports_own_unmodified_scroll_keys() {
    let mut app = crate::demo_app();
    app.select_perf_device(crate::PerfDevice::Gpu);
    assert_eq!(app.gpu_engine_scroll, 0);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(app.gpu_engine_scroll, 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.gpu_engine_scroll > 1);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('4'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(app.page(), AppPage::System);
    assert_eq!(app.system_scroll, 0, "page entry resets the viewport");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(app.system_scroll, 1);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::PageDown,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.system_scroll > 1);
}

#[test]
fn e_key_toggles_the_per_engine_gpu_session_on_the_gpu_device() {
    let mut app = crate::demo_app();
    assert_eq!(app.page(), AppPage::Performance);
    assert_eq!(app.perf_device, crate::PerfDevice::Cpu);
    assert!(matches!(
        app.shell.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Closed
    ));

    // Select the Gpu resource (digit 5), then `e` enables the session and
    // produces the typed engine-rows request for the demo GPU.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Gpu);
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        matches!(&effect, Some(PlatformEffect::GpuEngineRows(_))),
        "`e` must request the engine rows for the demo GPU, got {effect:?}"
    );
    let requested_device = match effect {
        Some(PlatformEffect::GpuEngineRows(request)) => request.device_id,
        _ => unreachable!("asserted GPU engine request above"),
    };
    let _ = app.shell.begin_gpu_engine_rows_request(requested_device);
    assert!(matches!(
        app.shell.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Loading { .. }
    ));

    // A second `e` stops the session and produces no request.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none());
    assert!(matches!(
        app.shell.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Closed
    ));

    // Off the Gpu device the toggle is a no-op.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert!(matches!(
        app.shell.gpu_engine_rows_state(),
        taskmanager_application::GpuEngineRowsState::Closed
    ));
}

#[test]
fn backspace_edits_the_search_field_when_active() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ),
    );
    assert!(app.search_active());

    // Type two characters, then Backspace pops the last one.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('z'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('e'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.query, "ze");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Backspace,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.query, "z",
        "Backspace must pop the last search character"
    );
}

#[test]
fn ctrl_arrows_scroll_the_inline_detail_panel_without_moving_the_cursor() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('2'),
            KeyModifiers::ALT,
        ),
    );
    let selected_before = app.selected;

    // Ctrl+Down scrolls the inline detail/insights panel; the table cursor
    // never moves.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::CONTROL,
        ),
    );
    assert!(
        app.detail_scroll > 0,
        "Ctrl+Down must scroll the detail panel"
    );
    assert_eq!(
        app.selected, selected_before,
        "the table cursor must not move"
    );

    // Ctrl+Up scrolls back.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Up,
            KeyModifiers::CONTROL,
        ),
    );
    assert_eq!(
        app.detail_scroll, 0,
        "Ctrl+Up must scroll the detail panel back"
    );
    assert_eq!(app.selected, selected_before);
}

#[test]
fn alt_7_reaches_the_app_history_page_through_the_shared_router() {
    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('7'),
            KeyModifiers::ALT,
        ),
    );
    assert_eq!(
        app.page(),
        AppPage::AppHistory,
        "Alt+7 must reach the seventh page"
    );
}

#[test]
fn control_hold_pauses_telemetry_through_the_shared_policy() {
    let mut app = crate::demo_app();
    assert!(!app.paused(), "not paused by default");
    assert!(!app.shell.control_held());

    // The event loop mirrors the live Ctrl state into the shared policy on
    // every key event (Press and Release alike); the policy then pauses
    // telemetry refresh while held — the same authority iced drives.
    app.shell.set_control_held(true);
    assert!(app.shell.control_held());
    assert!(app.paused(), "held control pauses telemetry refresh");

    // A Release-style sync (Ctrl no longer in the modifiers) clears the hold.
    app.shell.set_control_held(false);
    assert!(!app.shell.control_held());
    assert!(!app.paused());
}

/// The GPU-page `g` chord cycles the shared shell chart-metric selection
/// (ADR-034 stage 2): availability-gated through the demo GPU's typed
/// facts (power is unobserved, so the cycle skips it), reported in the
/// status bar, and a no-op off the GPU device.
#[test]
fn gpu_page_g_cycles_the_shared_chart_metric_selection() {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Gpu);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "the default selection is Utilization"
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "the cycle skips the unobserved power family"
    );
    let notice = app
        .shell
        .feedback_notice()
        .map(|notice| notice.text().to_owned())
        .unwrap_or_default();
    assert!(
        notice.contains(taskmanager_application::i18n::t("gpu.graph_temperature")),
        "the cycle must report the family it landed on: {notice}"
    );

    // Off the GPU device the same chord changes nothing.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('1'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "the chord is scoped to the GPU device"
    );
}

/// The same-wave fold (ADR-034 stage 2): when the next applied batch carries
/// a viewed GPU whose device generation advanced (a confirmed hot-plug), the
/// TUI's per-batch fold resets the shared selection to the Utilization
/// default before the next paint. The store edit stands in for the provider
/// fact; the fold itself is the production `apply_platform_batch` path every
/// live batch drives.
#[test]
fn gpu_chart_metric_selection_resets_when_the_generation_advances() {
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    // Bind the selection to the demo GPU's generation first — the production
    // fold does this on the very first batch, long before a user can press g.
    app.apply_platform_batch(taskmanager_application::PlatformEventBatch::default());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        if let Some(snapshot) = snapshot.as_mut()
            && let Some(gpu) = snapshot.gpu.first_mut()
        {
            gpu.device_generation =
                DeviceGeneration::new(gpu.device_generation.get().saturating_add(1));
        }
    });
    app.apply_platform_batch(taskmanager_application::PlatformEventBatch::default());

    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "a generation change must reset the selection to the ADR default"
    );
    let notice = app
        .shell
        .feedback_notice()
        .map(|notice| notice.text().to_owned())
        .unwrap_or_default();
    assert!(
        notice.contains(taskmanager_application::i18n::t("gpu.graph_utilization")),
        "the reset must land in the same wave the fact did: {notice}"
    );
}
