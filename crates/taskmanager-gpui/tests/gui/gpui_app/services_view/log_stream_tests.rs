//! Painted service-log stream contract for the service details dialog.
//!
//! `services.log-stream` delivers the sd-journal stream with level filtering.
//! These tests drive the production dialog (`RootView::open_service_details`
//! → `render_details` → `render_service_log_section`): every projected entry
//! paints its own row, the painted level-filter control narrows the drawn rows
//! to the matching severity, and a stream with no usable entries paints no
//! fabricated row at all.

use crate::gpui_app::root::{RootView, TopPage};
use gpui::{AppContext, TestAppContext, VisualTestContext, px};
use taskmanager_application::ServiceUpdate;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceItem, ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogStreamSnapshot,
    ServiceLogStreamState, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;
use taskmanager_theme::Theme;

const SERVICE: &str = "fixture.service:log-stream";

fn item() -> ServiceItem {
    ServiceItem::from_inventory(
        SERVICE,
        "log-stream",
        ServiceStatus::Active,
        "journal fixture",
        "",
        "",
        "",
    )
}

fn entry(cursor: &str, priority: u8, level: ServiceLogLevel, message: &str) -> ServiceLogEntry {
    ServiceLogEntry {
        cursor: cursor.to_owned(),
        realtime_timestamp_micros: None,
        priority: Some(priority),
        level,
        message: message.to_owned(),
    }
}

fn open_details(cx: &mut TestAppContext) -> (gpui::WindowHandle<RootView>, gpui::Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Services;
        v.replace_services_for_test(vec![item()], Vec::new());
        v.open_service_details(ServiceId::new(SERVICE));
        cx.notify();
    });
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// `debug_bounds` takes a `&'static str`; the stream rows are indexed.
fn selector(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// Seed the open dialog with one accepted stream batch through the production
/// state path (`next_follow_request` → `begin_stream_attempt` →
/// `accept_stream` → `apply`), the same sequence the runtime drives.
fn seed_stream(
    view: &gpui::Entity<RootView>,
    cx: &mut TestAppContext,
    entries: Vec<ServiceLogEntry>,
) {
    view.update(cx, |v, _| {
        let service_id = ServiceId::new(SERVICE);
        let query = v
            .service_details
            .next_follow_request(&service_id, 1_000)
            .expect("the open dialog must have an initial follow query");
        let attempt = v
            .service_details
            .begin_stream_attempt(query.clone())
            .expect("the follow query targets the open dialog");
        let request_id = RequestId::new(1).expect("non-zero request id");
        v.service_details.accept_stream(attempt, request_id);
        let snapshot = ServiceLogStreamSnapshot {
            state: ServiceLogStreamState::from_query_entries(&query, entries),
            query,
        };
        v.service_details.apply(ServiceUpdate::LogStream {
            request_id,
            observed_at_ms: 1_000,
            snapshot,
        });
    });
}

/// The painted stream, end to end: the same four-severity batch paints four
/// rows at the default `All` level, while a dialog whose painted level control
/// was advanced to `Errors` before the batch arrives paints only the error
/// row. The absence half uses a fresh window because `debug_bounds` keeps the
/// last frame that painted a selector, so a disappeared element is only
/// observable as "never painted in this window".
#[gpui::test]
async fn service_details_paints_the_stream_rows_and_the_level_filter_drops_lower_rows(
    cx: &mut TestAppContext,
) {
    let entries = || {
        vec![
            entry("c1", 3, ServiceLogLevel::Error, "collector failed"),
            entry("c2", 4, ServiceLogLevel::Warning, "spare nearly exhausted"),
            entry("c3", 6, ServiceLogLevel::Info, "collectors ready"),
            entry("c4", 7, ServiceLogLevel::Debug, "probe tick"),
        ]
    };

    // Window 1: the unfiltered stream paints one row per projected entry.
    let (win, view) = open_details(cx);
    seed_stream(&view, cx, entries());
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for index in 0..4 {
        let sel = selector(format!("tm-svc-log-line:{index}"));
        let line = vcx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{sel} must paint its stream line"));
        assert!(
            line.size.height > px(0.0) && line.size.width > px(0.0),
            "stream line {index} collapsed: {line:?}"
        );
    }
    assert!(
        vcx.debug_bounds("tm-svc-log-line:4").is_none(),
        "only the four projected entries may paint"
    );
    drop(vcx);

    // Window 2: the user-facing control is the production one — click the
    // painted level pill (All → Errors) before the batch arrives; the drawn
    // rows must follow the shared filter.
    let (win, view) = open_details(cx);
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let pill = vcx
        .debug_bounds("service-logs-level")
        .expect("the level filter control must be painted");
    vcx.simulate_click(pill.center(), Default::default());
    drop(vcx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.service_details
                .snapshot(&taskmanager_application::ServiceDependenciesLifecycle::default())
                .feed
                .level,
            ServiceLogLevelFilter::Errors,
            "the painted level control must advance the shared filter"
        );
    });
    seed_stream(&view, cx, entries());
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-svc-log-line:0").is_some(),
        "the error entry must stay painted under the Errors filter"
    );
    for index in 1..4 {
        let sel = selector(format!("tm-svc-log-line:{index}"));
        assert!(
            vcx.debug_bounds(sel).is_none(),
            "{sel}: a lower-severity row must not be painted under the Errors filter"
        );
    }
}

/// Honest absence: a stream with no accepted entries (and a rejected provider
/// attempt) paints the typed state instead of a fabricated log row — the panel
/// stays mounted, but no `tm-svc-log-line:*` selector exists.
#[gpui::test]
async fn service_details_paints_no_log_row_without_entries(cx: &mut TestAppContext) {
    let (win, view) = open_details(cx);
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("service-logs-level").is_some(),
        "the log panel must stay mounted while the stream has no entries"
    );
    assert!(
        vcx.debug_bounds("tm-svc-log-line:0").is_none(),
        "a cold stream must not paint a fabricated log row"
    );
    drop(vcx);

    // A rejected attempt is a typed failure, not an empty successful stream.
    view.update(cx, |v, _| {
        let service_id = ServiceId::new(SERVICE);
        let query = v
            .service_details
            .next_follow_request(&service_id, 2_000)
            .expect("the dialog still has a follow query");
        let attempt = v
            .service_details
            .begin_stream_attempt(query)
            .expect("the follow query targets the open dialog");
        v.service_details
            .reject_stream(attempt, FailureKind::TimedOut);
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("service-logs-level").is_some(),
        "a rejected stream keeps the panel mounted with its typed failure"
    );
    assert!(
        vcx.debug_bounds("tm-svc-log-line:0").is_none(),
        "a rejected stream must not paint a fabricated log row"
    );
}
