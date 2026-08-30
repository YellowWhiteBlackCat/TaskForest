//! TUI render unit tests, split by topic.
//!
//! `frame_text` is shared across the topic submodules ([`device_render`],
//! [`process_render`]); the projection group stays inline because it
//! carries its own self-contained helpers.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::ui::render;
use crate::{TuiApp, TuiTheme};

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test.
    // render() resolves chrome through the process-global t(), which
    // otherwise auto-seeds from the host locale (a zh host would translate
    // these assertions) and a concurrent set_language(Zh) would leak
    // translated text mid-render.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

#[path = "tests/acceptance_locale_frames.rs"]
mod acceptance_locale_frames;
#[path = "tests/acceptance_long_labels.rs"]
mod acceptance_long_labels;
#[path = "tests/acceptance_raw_keys.rs"]
mod acceptance_raw_keys;
#[path = "tests/acceptance_size_matrix.rs"]
mod acceptance_size_matrix;
#[path = "tests/acceptance_support.rs"]
mod acceptance_support;
#[path = "tests/chipset_render.rs"]
mod chipset_render;
#[path = "tests/cpu_metrics_render.rs"]
mod cpu_metrics_render;
#[path = "tests/cpu_right_rail.rs"]
mod cpu_right_rail;
#[path = "tests/detail_scroll_render.rs"]
mod detail_scroll_render;
#[path = "tests/device_history_render.rs"]
mod device_history_render;
#[path = "tests/device_render.rs"]
mod device_render;
#[path = "tests/directory_usage_render.rs"]
mod directory_usage_render;
#[path = "tests/focus_control.rs"]
mod focus_control;
#[path = "tests/focus_paint.rs"]
mod focus_paint;
#[path = "tests/memory_selector_parity.rs"]
mod memory_selector_parity;
#[path = "tests/overlay_hit.rs"]
mod overlay_hit;
#[path = "tests/process_render.rs"]
mod process_render;
#[path = "tests/row_alignment.rs"]
mod row_alignment;
#[path = "tests/service_details_render.rs"]
mod service_details_render;
#[path = "tests/session_render.rs"]
mod session_render;
#[path = "tests/source_render.rs"]
mod source_render;
#[path = "tests/startup_render.rs"]
mod startup_render;
#[path = "tests/unit_prefs_render.rs"]
mod unit_prefs_render;
#[path = "tests/visibility.rs"]
mod visibility;

// ── Projection render: partial telemetry must keep last complete values ─────
// The shell state machine folds projections into the render model; this test
// renders the TUI frame to prove a partial projection never zeroes the gauge.
mod projection_render_tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use taskmanager_application::{
        PlatformEventBatch, SystemTelemetryDomainEvent, SystemTelemetryProjection,
        SystemTelemetryProjectionApplyResult, SystemTelemetryRevision,
    };
    use taskmanager_core::core::metrics::{CpuMetrics, CpuTelemetryObservation};

    use crate::TuiTheme;
    use crate::render;

    fn projection_from(
        revision: SystemTelemetryRevision,
        events: impl IntoIterator<Item = SystemTelemetryDomainEvent>,
    ) -> taskmanager_application::ProjectedSystemTelemetry {
        let mut reducer = SystemTelemetryProjection::default();
        reducer.begin(revision);
        let mut latest = None;
        for event in events {
            latest = match reducer.apply(&event) {
                SystemTelemetryProjectionApplyResult::AppliedPartial(projection)
                | SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } => {
                    Some(*projection)
                }
                SystemTelemetryProjectionApplyResult::Ignored(reason) => {
                    panic!("fixture projection rejected: {reason:?}")
                }
            };
        }
        latest.expect("fixture should contain an event")
    }

    fn partial_projection(
        revision: u64,
        observed_at_ms: u64,
    ) -> taskmanager_application::ProjectedSystemTelemetry {
        let revision = SystemTelemetryRevision::new(revision);
        projection_from(
            revision,
            [SystemTelemetryDomainEvent::Cpu {
                revision,
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    observed_at_ms,
                    Vec::new(),
                )),
            }],
        )
    }

    #[test]
    fn partial_projection_render_keeps_last_complete_values_not_zeroes() {
        let mut app = crate::TuiApp::from_shell(taskmanager_shell::demo_app());
        let mut batch = PlatformEventBatch::default();
        batch
            .system_telemetry_projections
            .push(partial_projection(1, 20));
        app.apply_platform_batch(batch);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &app, TuiTheme::default()))
            .expect("draw");
        let text = terminal.backend().to_string();

        assert!(text.contains("37.4%"));
        assert!(!text.contains("CPU 0.0%"));
    }
}

#[test]
fn collecting_frame_masks_page_data_until_the_shared_commit_is_ready() {
    use taskmanager_application::AppPage;

    let mut app = crate::demo_app();
    let committed = app
        .projection()
        .snapshot
        .clone()
        .expect("demo fixture starts with a committed frame");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    app.application.active_page = AppPage::Applications;

    assert!(app.telemetry_frame_state().is_collecting());
    let collecting = frame_text(&app, 120, 36);
    assert!(collecting.contains("Initializing system telemetry"));
    assert!(
        !collecting.contains("zed"),
        "partial process facts must stay behind the first-frame mask"
    );

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(committed))),
    );
    assert!(app.telemetry_frame_state().is_ready());
    let ready = frame_text(&app, 120, 36);
    assert!(
        ready.contains("zed"),
        "the committed frame must reveal page data"
    );
    assert!(!ready.contains("Initializing system telemetry"));
}

mod table_window_tests {
    use crate::ui::frame_plan::{TableWindow, centered_popup, table_window};
    use ratatui::layout::Rect;

    #[test]
    fn table_window_tracks_the_global_cursor_without_exceeding_the_viewport() {
        let area = Rect::new(0, 0, 120, 20);
        // Four rows are table chrome (borders, header, header margin), so a
        // 20-line table materializes at most sixteen body rows.
        assert_eq!(
            table_window(10_000, 0, area),
            TableWindow {
                start: 0,
                end: 16,
                selected: 0,
            }
        );
        assert_eq!(
            table_window(10_000, 5_000, area),
            TableWindow {
                start: 4_992,
                end: 5_008,
                selected: 8,
            }
        );
        assert_eq!(
            table_window(10_000, usize::MAX, area),
            TableWindow {
                start: 9_984,
                end: 10_000,
                selected: 15,
            }
        );
    }

    #[test]
    fn table_window_clamps_small_tables_and_empty_inputs() {
        let area = Rect::new(0, 0, 80, 10);
        assert_eq!(
            table_window(2, 99, area),
            TableWindow {
                start: 0,
                end: 2,
                selected: 1,
            }
        );
        assert_eq!(
            table_window(0, 0, area),
            TableWindow {
                start: 0,
                end: 0,
                selected: 0,
            }
        );
    }

    #[test]
    fn centered_popup_is_the_single_clamped_overlay_geometry_rule() {
        assert_eq!(
            centered_popup(Rect::new(10, 5, 100, 40), 20, 10),
            Rect::new(50, 20, 20, 10)
        );
        let tiny = centered_popup(Rect::new(3, 7, 2, 1), 80, 20);
        assert_eq!(tiny.width, 0);
        assert_eq!(tiny.height, 0);
        assert!(tiny.x >= 3 && tiny.y >= 7);
    }
}

mod visual_row_count_tests {
    use crate::demo_app;

    #[test]
    fn visual_row_count_cache_recomputes_when_the_filter_changes() {
        let mut app = demo_app();
        let all = app.visual_row_count();
        assert_eq!(app.visual_row_count(), all);

        app.query = "__definitely_no_matching_process__".to_owned();
        assert_eq!(app.visual_row_count(), 0);

        app.query.clear();
        assert_eq!(app.visual_row_count(), all);
    }
}

mod process_frame_layout_tests {
    use ratatui::layout::Rect;
    use taskmanager_application::{AppAction, AppPage};

    use crate::demo_app;
    use crate::ui::frame_plan::{
        TuiFocusControl, TuiFocusOrder, TuiFocusTarget, TuiFramePlan, TuiHitTarget,
        frame_chrome_layout,
    };
    use crate::ui::process_table::process_table_layout;
    use crate::ui::table_hit::table_hit_support::table_panel_projection;

    #[test]
    fn applications_hit_projection_uses_the_painted_frame_bands() {
        let frame = Rect::new(0, 0, 120, 40);
        let chrome = frame_chrome_layout(frame);
        let page = process_table_layout(chrome.body);

        assert_eq!(page.search.y + page.search.height, page.table.y);
        assert_eq!(page.table.y + page.table.height, page.details.y);
        assert_eq!(
            page.details.y + page.details.height,
            chrome.body.y + chrome.body.height
        );

        let mut app = demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
        let hit = table_panel_projection(&app, frame).expect("Applications has a table");
        assert_eq!(hit.area, page.table);
        assert_eq!(
            TuiFramePlan::build(&app, frame).hit_target(page.table.x + 1, page.table.y + 3),
            Some(TuiHitTarget::TableRow {
                page: AppPage::Applications,
                index: 0,
            })
        );

        // Keep the render entry in this contract's test path: the helper is
        // not merely geometrically plausible; the page still paints through
        // the same body area after the extraction. frame_text pins English so
        // the title assertion cannot depend on the host locale.
        assert!(super::frame_text(&app, frame.width, frame.height).contains("Processes"));
    }

    #[test]
    fn frame_plan_is_exhaustive_and_focus_follows_the_active_scope() {
        for page in AppPage::ALL {
            let mut app = demo_app();
            let _ = app.apply_action(AppAction::SelectPage(page));
            let plan = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
            assert!(plan.page_matches(page), "plan page mismatch for {page:?}");
            if page == AppPage::Applications {
                assert!(plan.overlay().is_none());
            }
            if let Some(table) = plan.table_panel() {
                assert!(table.window.start <= table.window.end);
                assert!(table.window.end <= table.total);
            }
        }

        let mut app = demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
        app.focus_panel = crate::FocusPanel::Details;
        let details = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(details.focus.target, TuiFocusTarget::ApplicationsDetails);
        assert_eq!(details.focus.order, TuiFocusOrder::ApplicationsPanels);
        assert_eq!(details.focus.control, TuiFocusControl::DetailsViewport);

        app.focus_panel = crate::FocusPanel::Table;
        let _ = app.apply_action(AppAction::FocusSearch);
        let search = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(search.focus.target, TuiFocusTarget::Search);
        assert_eq!(search.focus.order, TuiFocusOrder::None);
        assert_eq!(search.focus.control, TuiFocusControl::SearchField);

        app.close_local_overlays();
        app.toggle_help();
        let help = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(help.focus.target, TuiFocusTarget::Help);
        assert_eq!(help.focus.control, TuiFocusControl::Viewport);
        let help_overlay = help.overlay().expect("help owns an overlay rectangle");
        assert_eq!(help_overlay.scope, crate::TuiInputScope::Help);
        assert_eq!(
            help_overlay.popup,
            crate::ui::frame_plan::centered_popup(Rect::new(0, 0, 120, 40), 68, 24,)
        );

        app.close_local_overlays();
        app.toggle_settings();
        let settings = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(
            settings.focus.target,
            TuiFocusTarget::LocalSurface(crate::TuiSurfaceKind::Settings)
        );
        assert_eq!(settings.focus.control, TuiFocusControl::SettingsField(0));
        let settings_overlay = settings
            .overlay()
            .expect("settings owns an overlay rectangle");
        assert_eq!(
            settings_overlay.scope,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Settings)
        );
        assert_eq!(
            settings_overlay.popup,
            crate::ui::frame_plan::centered_popup(Rect::new(0, 0, 120, 40), 68, 32)
        );
        assert_eq!(
            settings.hit_target(settings_overlay.popup.x, settings_overlay.popup.y),
            Some(TuiHitTarget::Overlay {
                scope: crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Settings),
            })
        );

        assert_eq!(
            settings.hit_target(0, 0),
            None,
            "non-table pages and overlay-owned cells stay outside the typed HitMap"
        );

        app.close_local_overlays();
        app.open_command_palette();
        let palette = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(
            palette.focus.control,
            TuiFocusControl::PaletteItem { index: 0 }
        );

        app.close_local_overlays();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
        assert!(app.open_service_menu(), "the demo exposes a service menu");
        let service_menu = TuiFramePlan::build(&app, Rect::new(0, 0, 120, 40));
        assert_eq!(
            service_menu.focus.control,
            TuiFocusControl::MenuItem {
                surface: crate::TuiSurfaceKind::ServiceMenu,
                index: 0,
            }
        );
    }
}
