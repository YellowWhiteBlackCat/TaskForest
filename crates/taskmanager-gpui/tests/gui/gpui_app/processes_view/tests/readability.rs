//! Positive geometry contracts for the Apps page at capture-matrix sizes.
//!
//! These assertions deliberately avoid pixel snapshots. They measure the
//! rendered semantic surfaces and preserve the mature-layout invariants that
//! screenshot provenance/marker validation cannot see: the table remains the
//! dominant content surface, headers and rows stay readable, and compact mode
//! still exposes useful data rather than only chrome.

use gpui::{VisualTestContext, px, size};
use taskmanager_application::{
    CapabilityId, CorrelatedEvent, EventSequence, PlatformEventBatch, PlatformEventContext,
    ProcessEvent, ProviderId, RequestId,
};

use crate::gpui_app::root::TopPage;

fn process_batch(count: u32) -> PlatformEventBatch {
    let processes = (1..=count)
        .map(|pid| {
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(pid)
                .name(format!("readability-worker-{pid:02}"))
                .current_cpu_percentage(pid as f32)
                .current_memory_bytes(u64::from(pid) * 1024 * 1024)
                .status("Running".to_owned())
                .build()
        })
        .collect();
    PlatformEventBatch {
        process_events: vec![CorrelatedEvent::new(
            PlatformEventContext {
                request_id: RequestId::new(91).expect("fixture request id"),
                capability: CapabilityId::PROCESS_LIST,
                provider: Some(ProviderId::borrowed("fixture.readability")),
                sequence: EventSequence::new(1),
                observed_at_ms: 1_000,
            },
            ProcessEvent::Snapshot(processes),
        )],
        ..PlatformEventBatch::default()
    }
}

#[gpui::test]
async fn mc07_compact_matrix_case_apps_table_remains_the_dominant_readable_surface_at_capture_sizes(
    cx: &mut gpui::TestAppContext,
) {
    let (win, view) = super::wrapped_root(cx);
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.processes_state
            .expanded_apps
            .insert("category:uncategorized".to_owned());
        let _ = view.apply_platform_event_batch(process_batch(48), cx);
        cx.notify();
    });

    for (width, height, minimum_visible_rows) in [
        (720.0_f32, 480.0_f32, 4_usize),
        (1180.0_f32, 780.0_f32, 8_usize),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        super::draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let body = vcx
            .debug_bounds("tm-telemetry-ready-body")
            .expect("the committed application body must render");
        let table = vcx
            .debug_bounds("tm-procs-table-scroll")
            .expect("the Apps table viewport must render");
        let header = vcx
            .debug_bounds("tm-proc-hdr-row")
            .expect("the Apps table header must remain visible");
        let status = vcx
            .debug_bounds("tm-status-bar")
            .expect("the Apps status bar must remain visible");

        let body_height = f32::from(body.size.height);
        let table_height = f32::from(table.size.height);
        assert!(
            table_height >= body_height * 0.45,
            "the process table must remain the dominant body surface at {width}x{height}: body={body:?}, table={table:?}"
        );
        assert!(
            header.size.height >= px(28.0),
            "the table header must retain a readable hit/label height at {width}x{height}: {header:?}"
        );
        assert!(
            table.origin.y + table.size.height <= status.origin.y + px(0.5),
            "the process table must stop before the fixed status bar: table={table:?}, status={status:?}"
        );

        let visible_rows = [
            "tm-proc-row-root:0",
            "tm-proc-row-root:1",
            "tm-proc-row-root:2",
            "tm-proc-row-root:3",
            "tm-proc-row-root:4",
            "tm-proc-row-root:5",
            "tm-proc-row-root:6",
            "tm-proc-row-root:7",
            "tm-proc-row-root:8",
            "tm-proc-row-root:9",
            "tm-proc-row-root:10",
            "tm-proc-row-root:11",
        ]
        .into_iter()
        .filter_map(|selector| vcx.debug_bounds(selector))
        .filter(|row| {
            row.size.height >= px(28.0)
                && row.origin.y >= table.origin.y - px(0.5)
                && row.bottom() <= table.bottom() + px(0.5)
        })
        .count();
        assert!(
            visible_rows >= minimum_visible_rows,
            "{width}x{height} must expose at least {minimum_visible_rows} readable process rows; got {visible_rows}, table={table:?}"
        );
    }
}

/// A wide viewport is horizontal capacity, not permission to keep stacking
/// toolbar rows. The overview uses its trailing area for search and the typed
/// Wide presentation combines primary commands, hierarchy and status filters
/// into one bounded control band. This measures the real GPUI layout at both a
/// short and generous height so width and height cannot collapse into one
/// implicit compact flag again.
#[gpui::test]
async fn wide_apps_chrome_has_a_bounded_share_and_returns_height_to_the_table(
    cx: &mut gpui::TestAppContext,
) {
    let (win, view) = super::wrapped_root(cx);
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.processes_state
            .expanded_apps
            .insert("category:uncategorized".to_owned());
        let _ = view.apply_platform_event_batch(process_batch(48), cx);
        cx.notify();
    });

    for (width, height, minimum_table_share) in [
        (1920.0_f32, 540.0_f32, 0.60_f32),
        (2048.0_f32, 1080.0_f32, 0.72_f32),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        super::draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let body = vcx
            .debug_bounds("tm-telemetry-ready-body")
            .expect("the committed application body must render");
        let overview = vcx
            .debug_bounds("tm-proc-overview")
            .expect("wide Apps must expose its overview band");
        let search = vcx
            .debug_bounds("tm-search-box")
            .expect("wide Apps must keep search in the overview band");
        let controls = vcx
            .debug_bounds("tm-proc-unified-controls")
            .expect("wide Apps must use one typed unified control band");
        assert!(
            vcx.debug_bounds("tm-proc-stacked-controls").is_none(),
            "Wide must not retain the retired stacked control path"
        );
        let actions = vcx
            .debug_bounds("tm-proc-action-bar")
            .expect("wide Apps must keep primary commands visible");
        let overflow = vcx
            .debug_bounds("tm-proc-actions-trigger")
            .expect("wide Apps must keep secondary commands in the accessible menu");
        let hierarchy = vcx
            .debug_bounds("tm-proc-mode-switcher")
            .expect("wide Apps must keep hierarchy mode visible");
        let filters = vcx
            .debug_bounds("tm-proc-status-filter")
            .expect("wide Apps must keep every status filter visible");
        let table = vcx
            .debug_bounds("tm-procs-table-scroll")
            .expect("wide Apps must allocate the remaining height to its table");

        assert!(
            search.origin.y >= overview.origin.y - px(0.5)
                && search.bottom() <= overview.bottom() + px(0.5),
            "search must consume the overview's trailing slot: overview={overview:?}, search={search:?}"
        );
        for (label, bounds) in [
            ("primary commands", actions),
            ("secondary-command trigger", overflow),
            ("hierarchy", hierarchy),
            ("status filters", filters),
        ] {
            assert!(
                bounds.origin.y >= controls.origin.y - px(0.5)
                    && bounds.bottom() <= controls.bottom() + px(0.5),
                "{label} must belong to the unified control band: controls={controls:?}, child={bounds:?}"
            );
        }

        let body_height = f32::from(body.size.height);
        let chrome_height = f32::from(controls.bottom() - overview.origin.y);
        let table_height = f32::from(table.size.height);
        assert!(
            chrome_height <= body_height * 0.30,
            "wide chrome must remain a bounded minority of the page at {width}x{height}: body={body:?}, overview={overview:?}, controls={controls:?}"
        );
        assert!(
            table_height >= body_height * minimum_table_share,
            "wide table must receive at least {:.0}% of the body at {width}x{height}: body={body:?}, table={table:?}",
            minimum_table_share * 100.0,
        );
    }
}
