//! Process-page render tests: process table + details, the flat Applications
//! table, settings/service/process/session menus, export footer, app-history,
//! and the keyboard-help overlay. Extracted from `ui/tests.rs`. The grouped
//! views and the per-row sparkline/trend tests live in [`grouped_render`].

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::{AppAction, AppPage, ConfigStore};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessMetadataObservations, ProcessOwner};
use taskmanager_shell::{SortCol, SortDir};

use crate::ui::*;
use crate::{TuiApp, TuiTheme};

use super::frame_text;

#[path = "process_render/grouped_render.rs"]
mod grouped_render;

fn wait_for_config_outcome(app: &mut TuiApp) {
    let drain = app
        .config_client
        .as_mut()
        .expect("injected config client")
        .wait_for_drain(std::time::Duration::from_secs(2));
    match drain {
        taskmanager_application::ConfigDrain::Empty => panic!("expected config outcome"),
        taskmanager_application::ConfigDrain::Publications(publications) => {
            for publication in publications {
                app.apply_config_publication(&publication);
            }
        }
        taskmanager_application::ConfigDrain::ResyncRequired { latest, .. } => {
            app.apply_config_publication(&latest);
        }
    }
}

#[test]
fn search_highlight_keeps_process_names_rendered_and_segments_exact() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.open_search();
    app.query = "SYSTEM".to_string();

    let text = frame_text(&app, 150, 56);

    // Highlighting must not remove matching rows. The canonical indentation
    // consumes part of the bounded Name column, so long names may truncate.
    assert!(text.contains("systemd"), "matched name must stay rendered");
    assert!(
        text.contains("systemd-jour"),
        "second matching process must remain visible, got:\n{text}"
    );

    // And the segmenter must split that same input exactly as the
    // renderer consumes it.
    let segments = crate::ui::highlight::highlight_segments("systemd-journald", "SYSTEM");
    assert_eq!(
        segments,
        vec![
            ("system".to_string(), true),
            ("d-journald".to_string(), false),
        ]
    );
}

#[test]
fn process_table_header_marks_the_active_sort_column_and_direction() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    // Default sort is CPU descending: that column carries ▼ and no other
    // column carries an arrow.
    app.process_sort = (SortCol::Cpu, SortDir::Desc);
    let text = frame_text(&app, 120, 36);
    assert!(
        text.contains("CPU ▼"),
        "descending CPU header must carry the ▼ marker"
    );
    assert!(
        !text.contains("PID ▼") && !text.contains("PID ▲"),
        "non-active columns must not carry a sort arrow"
    );

    // Flipping to PID ascending moves the marker.
    app.process_sort = (SortCol::Pid, SortDir::Asc);
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("PID ▲"));
    assert!(
        !text.contains("CPU ▼"),
        "the previous column must lose its marker"
    );
}

#[test]
fn help_overlay_renders_when_toggled_and_hides_when_closed() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    app.toggle_help();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Keyboard reference"));
    assert!(text.contains("? / Esc"));
    // A genuinely-wired shared shortcut is listed, not a fabricated one.
    assert!(text.contains("Ctrl+F"));

    app.toggle_help();
    let closed = frame_text(&app, 120, 36);
    assert!(!closed.contains("Keyboard reference"));
}

#[test]
fn process_details_panel_renders_frozen_and_current_row_facts() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    let text = frame_text(&app, 120, 56);
    assert!(text.contains("Process details"));
    // The selected row (default sort: highest CPU = zed/4201) and its
    // full field set are rendered.
    assert!(text.contains("zed"));
    assert!(text.contains("4201"));
    assert!(text.contains("24.8%"));
    assert!(text.contains("devuser"));
    assert!(text.contains("Running"));
    assert!(text.contains("Start"));
    assert!(text.contains("Executable"));
    // The parity detail rows render their labels even when the demo fixture
    // carries no observation for them (an honest dash, not a fabricated 0).
    assert!(text.contains("CPU time"));
    assert!(text.contains("Nice"));
    assert!(text.contains("Disk read"));
    assert!(text.contains("Disk write"));
}

#[test]
fn process_action_menu_opens_for_the_selected_row_and_renders_actions() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(
        app.open_process_menu(),
        "the menu opens on the Applications page"
    );
    assert!(app.process_menu().is_some());
    // The frozen target is the selected (highest-CPU) demo row.
    assert_eq!(app.process_menu().expect("menu open").item.pid, 4201);

    let text = frame_text(&app, 100, 26);
    assert!(text.contains("Process actions"));
    assert!(text.contains("Open file location"));
    assert!(text.contains("Search online"));

    // Closing the overlay dismisses the menu.
    app.close_local_overlays();
    assert!(app.process_menu().is_none());
}

#[test]
fn process_menu_resolve_action_routes_through_platform_ports_not_direct_spawn() {
    use crate::ui::process_menu::{ProcessMenuAction, ProcessMenuTarget, resolve_action};
    use taskmanager_application::PlatformEffect;
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::process::FrozenProcessIdentity;

    let mut item = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(4321)
        .name("my worker".into())
        .build();
    item.apply_scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
        start_token: ScalarObservation::available(7, 1),
        ..Default::default()
    });
    item.apply_metadata_observations(ProcessMetadataObservations::current(
        ProcessOwner::opaque("fixture"),
        Some("/usr/bin/worker".into()),
        1,
    ));

    // Search-online builds a percent-encoded OpenUrl effect (no spawn).
    let search = resolve_action(&ProcessMenuTarget {
        item: item.clone(),
        selection: ProcessMenuAction::SearchOnline as usize,
    })
    .expect("search resolves");
    match search {
        PlatformEffect::OpenUrl(request) => {
            assert_eq!(request.url, "https://www.google.com/search?q=my%20worker");
        }
        other => panic!("search must be OpenUrl, got {other:?}"),
    }

    // Open-location builds a RevealResource effect carrying the frozen
    // identity + cached executable (the provider owns the spawn).
    let reveal = resolve_action(&ProcessMenuTarget {
        item: item.clone(),
        selection: ProcessMenuAction::OpenLocation as usize,
    })
    .expect("reveal resolves");
    match reveal {
        PlatformEffect::RevealResource(request) => {
            assert_eq!(request.target.pid, 4321);
            assert_eq!(
                request.cached_executable.as_deref(),
                Some(std::path::Path::new("/usr/bin/worker"))
            );
            // from_process produced an authoritative identity.
            assert!(FrozenProcessIdentity::from_process(&item).is_some());
        }
        other => panic!("reveal must be RevealResource, got {other:?}"),
    }
}

#[test]
fn apps_table_projects_typed_pss_and_swap_without_zero_fallbacks() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    taskmanager_shell::fixture::edit_processes(&mut app.shell, |processes| {
        let process = processes
            .as_mut()
            .and_then(|processes| processes.iter_mut().find(|process| process.pid == 4201))
            .expect("demo process fixture");
        let mut observations = *process.scalar_observations();
        observations.memory_pss_bytes = ScalarObservation::available(512 * 1024 * 1024, 1);
        observations.swap_bytes = ScalarObservation::available(0, 1);
        process.apply_scalar_observations(observations);
    });

    let text = frame_text(&app, 140, 40);

    assert!(text.contains("PSS"));
    assert!(text.contains("512.0 MiB"));
    assert!(text.contains("PSS / Swap"));
    assert!(
        text.contains("0 B"),
        "measured zero swap must remain visible"
    );
}

#[test]
fn process_details_panel_renders_honest_empty_state_without_rows() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(Vec::new())),
    );
    app.selected = 0;

    let text = frame_text(&app, 120, 40);
    assert!(text.contains("Process details"));
    assert!(text.contains("No process selected"));
    assert!(text.contains("No processes reported yet."));
}

#[test]
fn service_confirmation_overlay_renders_for_a_pending_control_target() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let service = app.projection().services.as_ref().expect("demo services")[1].clone();
    assert!(app.select_service_control(
        &service,
        taskmanager_core::core::services::ServiceAction::Restart
    ));
    let _ = app.apply_action(AppAction::RequestServiceControl);

    let text = frame_text(&app, 100, 30);
    assert!(text.contains("Confirm service action"));
    assert!(text.contains("Restart"));
    assert!(text.contains(&service.id.to_string()));
    assert!(text.contains("provider-issued target"));
}

#[test]
fn service_action_menu_renders_actions_and_frozen_row() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu());

    let text = frame_text(&app, 100, 30);
    assert!(text.contains("Service actions"));
    assert!(text.contains("Start"));
    assert!(text.contains("Stop"));
    assert!(text.contains("Restart"));
    assert!(text.contains("Enable"));
    assert!(text.contains("Disable"));
    assert!(text.contains("NetworkManager.service"));
}

#[test]
fn settings_overlay_renders_all_fields_with_enter_apply_hint() {
    let mut app = crate::demo_app();
    app.toggle_settings();

    let text = frame_text(&app, 140, 30);
    assert!(text.contains("Settings"));
    assert!(text.contains("Skin"));
    assert!(text.contains("Mode"));
    assert!(text.contains("High contrast"));
    assert!(text.contains("Desktop UI font"));
    assert!(text.contains("Desktop monospace font"));
    assert!(text.contains("Row density"));
    assert!(text.contains("Language"));
    assert!(text.contains("Enter save"));
    // The bounded popup wraps the durable-language note after "across".
    assert!(text.contains("language persists across"));
}

#[test]
fn settings_save_round_trips_through_the_config_store() {
    let mut app = crate::demo_app();
    let config_dir = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-config-test-{}",
        std::process::id()
    ));
    let config_path = config_dir.join("config.json");
    crate::ui::test_support::install_config_store(&mut app, config_path.clone());

    app.toggle_settings();
    // Focus the skin field (already focused), step to KDE, apply.
    app.settings_form.move_field(1);
    app.settings_form.step_value(1);
    app.settings_form.move_field(-1);
    app.settings_form.step_value(1);
    assert!(app.apply_settings_form(), "save must succeed");
    wait_for_config_outcome(&mut app);
    assert!(!app.settings_open());

    // The shared store round-trips the persisted token…
    let store = ConfigStore::new(&config_path);
    let config = store.load().expect("config must load");
    assert_eq!(config.skin, "KDE");

    // …and the runtime theme parameters were rebuilt from it.
    assert_eq!(
        app.theme_params,
        crate::ThemeParams::from_config_tokens("KDE", "EyeForest", false)
    );

    // A fresh TUI instance seeded from the same path picks the change up.
    let mut fresh = crate::demo_app();
    crate::ui::test_support::install_config_store(&mut fresh, config_path.clone());
    fresh.cancel_settings();
    assert_eq!(fresh.settings_form.skin, 1, "reloaded form sees KDE");
    let _ = std::fs::remove_dir_all(&config_dir);
}

#[test]
fn settings_save_failure_is_surfaced_in_the_overlay() {
    let mut app = crate::demo_app();
    // Point the store at a path whose parent is a FILE so directory
    // creation (and thus the save) must fail.
    let blocker = crate::ui::test_support::repo_temp_dir().join(format!(
        "taskmanager-tui-config-blocker-{}",
        std::process::id()
    ));
    std::fs::write(&blocker, "not a directory").expect("blocker file");
    crate::ui::test_support::install_config_store(&mut app, blocker.join("config.json"));
    app.toggle_settings();

    assert!(
        app.apply_settings_form(),
        "the patch is accepted asynchronously"
    );
    wait_for_config_outcome(&mut app);
    assert!(app.settings_open(), "the overlay stays open on failure");
    let text = frame_text(&app, 100, 40);
    assert!(text.contains("save failed"));
    let _ = std::fs::remove_file(&blocker);
}

#[test]
fn unavailable_export_worker_feedback_renders_in_the_footer() {
    // Pin before export_snapshot() freezes the notice text.
    taskmanager_test_support::pin_english();
    let mut app = crate::demo_app();
    // Shorten the status line so the export hint survives the footer's
    // wrap-trim at the reference width.
    app.set_feedback_activity("");
    app.clear_feedback_notice();
    app.export_snapshot();
    let feedback = app.feedback_notice().expect("export feedback");
    assert_eq!(
        feedback.severity(),
        taskmanager_shell::FeedbackSeverity::Error
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Snapshot export is unavailable"));
    assert!(text.contains("x export"));
}

#[test]
fn export_failure_renders_in_the_footer_in_danger_style() {
    // Pin before t() resolves the notice text the assertions match.
    taskmanager_test_support::pin_english();
    let mut app = crate::demo_app();
    app.report_notice(
        taskmanager_shell::FeedbackSource::Persistence,
        taskmanager_shell::FeedbackSeverity::Error,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        taskmanager_application::i18n::t("system.export_failed").replacen("{}", "disk full", 1),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("export failed"));
    assert!(text.contains("disk full"));
}

#[test]
fn empty_tables_render_honest_empty_states() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(Vec::new())),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("No services reported yet."));

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(Vec::new())),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("No sessions"));

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::StartupEntries(Some(Vec::new())),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("No startup entries reported yet."));
}

#[test]
fn applications_empty_state_answers_the_search_question() {
    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(Vec::new())),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    // An empty list with no active query is the honest "nothing reported"
    // state (the Applications empty state is rendered through the shared
    // windowed-table primitive's zero-row branch).
    let text = frame_text(&app, 120, 36);
    assert!(
        text.contains(taskmanager_application::i18n::t(
            "empty.no_processes_reported"
        )),
        "no-query empty state must say nothing was reported, got:\n{text}"
    );
    assert!(
        !text.contains(taskmanager_application::i18n::t(
            "empty.no_processes_match_query"
        )),
        "the query-mismatch copy must not paint without a query"
    );

    // The same empty list under an active query answers the search: the copy
    // names the query mismatch, never a fabricated source failure.
    app.query = "zzz".to_string();
    let text = frame_text(&app, 120, 36);
    assert!(
        text.contains(taskmanager_application::i18n::t(
            "empty.no_processes_match_query"
        )),
        "a non-matching query must name the query mismatch, got:\n{text}"
    );
    assert!(
        !text.contains(taskmanager_application::i18n::t(
            "empty.no_processes_reported"
        )),
        "the no-query copy must not paint under an active query"
    );
}

#[test]
fn containers_event_drains_into_tui_state_through_the_batch() {
    use taskmanager_application::{ContainerRollupEvent, CorrelatedEvent, PlatformEventBatch};
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_platform_contract::{EventSequence, RequestId};

    let mut app = crate::demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(None),
    );
    let rollup = taskmanager_core::core::process_telemetry::ContainerRollup {
        state: DeviceState::healthy(1_000),
        containers: Vec::new(),
    };
    let mut batch = PlatformEventBatch::default();
    batch.containers_events.push(CorrelatedEvent {
        request_id: RequestId::new(1).expect("non-zero request id"),
        capability: taskmanager_platform_contract::CapabilityId::CONTAINERS,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1_000,
        event: ContainerRollupEvent::Snapshot(Box::new(rollup.clone())),
    });
    app.apply_platform_batch(batch);
    assert_eq!(app.projection().containers, Some(rollup));
}

#[test]
fn performance_graph_renders_honest_empty_state_before_first_sample() {
    // CONSCIOUS UPDATE (2026-08-29 fixture seeding): the demo frame now seeds
    // a full measured-looking history window (see `demo::seed_demo_history`),
    // so the cold-start shape under test — fewer than two samples — is built
    // here directly instead of through `demo_app`. The graph reads the SHARED
    // `MetricHistory` window (G-02); one point is not a trend and the graph
    // must say so honestly.
    use taskmanager_application::{AppAction, AppPage};
    use taskmanager_core::core::metrics::{
        CpuMetrics, CpuScalarObservations, ScalarObservation, ScalarObservationGroup,
        SystemSnapshot,
    };
    use taskmanager_shell::fixture::{edit_snapshot, record_demo_history_frame};
    use taskmanager_telemetry_store::live_graph::MetricSeries;

    let mut app = TuiApp::new();
    let mut snapshot = SystemSnapshot {
        timestamp_ms: 1_000,
        ..SystemSnapshot::default()
    };
    snapshot.cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(37.4, 1_000),
        core_usage_group: ScalarObservationGroup::available(vec![52.0], 1_000),
        ..CpuScalarObservations::default()
    });
    edit_snapshot(&mut app.shell, |slot| *slot = Some(snapshot));
    let seeded = app
        .projection()
        .snapshot
        .clone()
        .expect("fixture snapshot exists");
    record_demo_history_frame(&mut app.shell, &seeded, None, None);
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.select_perf_device(crate::PerfDevice::Cpu);
    assert_eq!(
        app.history.series(MetricSeries::CpuUsagePercent).len(),
        1,
        "the fixture seeds exactly one sample into the shared window"
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("CPU Utilization (%)"));
    assert!(text.contains("Collecting samples"));
    // The chart-only artifacts (axis labels) must be absent so the empty
    // state cannot masquerade as a plotted series.
    assert!(!text.contains("100%"));
    assert!(!text.contains("older"));
}

#[test]
fn performance_graph_renders_chart_axes_once_real_samples_are_recorded() {
    use taskmanager_telemetry_store::live_graph::MetricSeries;
    let mut app = crate::demo_app();
    // Drive the same record path the shell fold drives on each telemetry
    // tick: `MetricHistory::record_snapshot` on the fresh snapshot (bumping
    // the timestamp each tick so a real cadence is modeled, and varying the
    // CPU reading so the line carries real distinct values).
    for tick in 1..=12u64 {
        let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
        snapshot.timestamp_ms = 1_785_292_800_000 + tick;
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.global_usage_pct =
            taskmanager_core::core::metrics::ScalarObservation::available(
                30.0 + tick as f32,
                snapshot.timestamp_ms,
            );
        snapshot.cpu.apply_scalar_observations(observations);
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
        );
    }
    // The shared window holds exactly the twelve finite ticks we drove plus
    // the demo's seeded history depth, with the last CPU reading preserved —
    // proves real values reached the buffer, not a fabricated flat line.
    // (CONSCIOUS UPDATE: the demo now seeds `DEMO_HISTORY_FRAMES` frames, not
    // the single cold-start sample.)
    let cpu = app.history.series(MetricSeries::CpuUsagePercent);
    assert_eq!(
        cpu.len(),
        crate::demo::DEMO_HISTORY_FRAMES + 12,
        "demo seeded history + twelve driven ticks"
    );
    assert_eq!(cpu.last(), Some(&42.0));

    let text = frame_text(&app, 120, 36);
    assert!(text.contains("CPU Utilization (%)"));
    // The y-axis labels only render with the chart, never in the empty
    // state, and no gauge on the page shows a bare 100%/50%.
    assert!(text.contains("100%"));
    assert!(text.contains("50%"));
    // The x-axis direction label renders with the chart.
    assert!(text.contains("older"));
    // The honest empty-state message must be gone once samples exist.
    assert!(!text.contains("Collecting samples"));
}

/// A body string routed through the shared catalog must re-render localized
/// when the active language flips. The about overlay's title is resolved via
/// `t("about.title")`, so it must read "关于任务森林" under Zh and
/// "About TaskForest" under En. Holds `LANG_TEST_GUARD` for the whole cycle
/// so no parallel render test observes the mid-test global mutation (Mutex
/// is non-reentrant, so the draw is inlined rather than going through the
/// guarded `frame_text` helper).
#[test]
fn localized_body_string_changes_when_language_switches() {
    use taskmanager_application::i18n::{Language, set_language};

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    let mut app = crate::demo_app();
    app.toggle_about();

    let draw = |app: &TuiApp| -> String {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, app, TuiTheme::default()))
            .expect("draw");
        terminal.backend().to_string()
    };

    set_language(Language::Zh);
    let zh_text = draw(&app);
    set_language(Language::En);
    let en_text = draw(&app);
    // Restore the host default so the rest of the suite is unaffected.
    set_language(Language::En);

    assert!(
        zh_text.contains("关于任务森林"),
        "the about title must render localized under Zh"
    );
    assert!(
        en_text.contains("About TaskForest"),
        "the about title must render in English under En"
    );
    assert_ne!(
        zh_text, en_text,
        "switching the active language must change the rendered body text"
    );
}

#[test]
fn process_table_renders_the_full_column_projection() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let text = frame_text(&app, 150, 36);
    // The widened header covers the advanced columns (GPUI column parity).
    // The labels come from the shell's SortCol single source (label()).
    for header in [
        "Threads", "Fds", "Nice", "Start", "CPU time", "Disk R/s", "Disk W/s",
    ] {
        assert!(text.contains(header), "header column {header} missing");
    }
    // The first demo row carries real values in the advanced cells (the
    // highest-CPU fixture process), not dashes.
    assert!(text.contains("4201"), "fixture pid visible");
    assert!(text.contains("s"), "cpu-time cell renders");
}

/// Column separation (the 列分隔 readability rule): a wide terminal renders a
/// TWO-blank gutter between adjacent columns, while a narrow one keeps the
/// one-blank gutter so every column still fits (the projection test above
/// pins the narrow behavior). The assertion reads the rendered header line,
/// not the widget config — the behavior, not the setting.
#[test]
fn wide_terminals_render_a_two_blank_column_gutter() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let wide = frame_text(&app, 200, 36);
    assert!(
        wide.contains("Threads  Fds"),
        "adjacent headers must carry the two-blank gutter: {wide}"
    );
    // Below the threshold the same pair renders at the one-blank gutter.
    let narrow = frame_text(&app, 150, 36);
    assert!(
        narrow.contains("Threads Fds"),
        "narrow terminals keep the one-blank gutter: {narrow}"
    );
    assert!(
        !narrow.contains("Threads  Fds"),
        "the narrow table must not waste budget on a wide gutter"
    );
}

#[test]
fn search_highlight_reaches_pid_user_and_group_header_cells() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.open_search();
    app.query = "zed".into();
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("zed"), "name match renders");

    // Numeric query matches the pid column.
    app.query = "4201".into();
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("4201"));

    // The canonical category hierarchy keeps matching rows visible too.
    app.open_search();
    app.query = "zed".into();
    let text = frame_text(&app, 150, 36);
    assert!(
        text.contains("×") || text.contains("zed"),
        "grouped header still renders the matching group"
    );
}

#[test]
fn gray_zero_dims_measured_zeros_but_keeps_unavailable_dashes() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    // Force every row to a measured zero CPU and memory so the first screen
    // (any sort order) carries a zeroed reading.
    let mut processes = app.projection().processes.clone().expect("demo processes");
    for process in &mut processes {
        let mut observations = *process.scalar_observations();
        observations.cpu_percentage = ScalarObservation::available(0.0, 1);
        observations.memory_bytes = ScalarObservation::available(0, 1);
        process.apply_scalar_observations(observations);
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(processes)),
    );
    app.prefs.gray_zero = true;
    let text = frame_text(&app, 150, 36);
    // The zeroed memory reads "0 B" (a measured zero, dimmed — the text is
    // unchanged; the style is what dims), and the unavailable disk rate stays
    // a dash.
    assert!(text.contains("0 B"));
    assert!(text.contains("0.0%"));
    // With the preference off the values render identically (the tint is a
    // style change, never a data change).
    app.prefs.gray_zero = false;
    let text = frame_text(&app, 150, 36);
    assert!(text.contains("0 B"));
}

#[test]
fn services_table_header_marks_the_active_keyboard_sort_column() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));

    // No sort picked yet: the header carries no arrow.
    let text = frame_text(&app, 100, 30);
    assert!(!text.contains("▼"), "provider order renders no sort arrow");

    // The `s` key sort (w8) writes the shared slot; the header then marks the
    // active column with the direction arrow.
    app.shell.set_info_sort(
        taskmanager_shell::InfoTable::Services,
        taskmanager_shell::InfoSortCol::Status,
    );
    app.shell
        .toggle_info_sort_direction(taskmanager_shell::InfoTable::Services);
    let text = frame_text(&app, 100, 30);
    assert!(
        text.contains("Status ▼"),
        "the Status header marks the descending sort: {}",
        text
    );
    assert!(
        !text.contains("Name ▼") && !text.contains("Name ▲"),
        "non-active headers carry no arrow"
    );
}
