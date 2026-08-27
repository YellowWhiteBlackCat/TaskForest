//! Geometry and interaction contracts for the Startup page's primary table.

use crate::core::{
    FailureKind, ProviderId, SourceOutcome, SourceStatus, StartupBootEvidenceSnapshot,
    StartupCriticalChainNode,
};
use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::theme::Theme;
use gpui::{AppContext, Modifiers, TestAppContext, VisualTestContext, WindowHandle, px, size};
use taskmanager_application::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence, StartupScope,
    StartupSource,
};

fn draw(cx: &mut TestAppContext, window: WindowHandle<RootView>) {
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("startup test window must draw");
}

fn entry(index: usize) -> StartupEntry {
    StartupEntry {
        id: format!("desktop:startup-layout-{index}.desktop").into(),
        name: format!("Startup layout fixture {index:02}"),
        exec: format!("/usr/bin/startup-layout-{index}"),
        enabled: index.is_multiple_of(2),
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: format!("startup-layout-{index}.desktop").into(),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 24 },
    }
}

fn evidence() -> StartupBootEvidenceSnapshot {
    StartupBootEvidenceSnapshot {
        critical_chain: (0..20)
            .map(|index| StartupCriticalChainNode {
                unit: format!("startup-layout-{index:02}.service"),
                activated_at_ms: Some(index * 100),
                duration_ms: Some(80),
            })
            .collect(),
        ..StartupBootEvidenceSnapshot::default()
    }
}

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("linux.startup.systemd-blame"),
        outcome,
        item_count: 40,
    }
}

fn install_fixture(view: &gpui::Entity<RootView>, cx: &mut TestAppContext, outcome: SourceOutcome) {
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Startup;
        view.replace_startup_for_test((0..40).map(entry).collect(), vec![source(outcome)]);
        view.replace_startup_evidence_for_test(Some(evidence()), None);
        cx.notify();
    });
}

#[gpui::test]
async fn constrained_partial_and_failure_keep_the_table_readable_and_interactive(
    cx: &mut TestAppContext,
) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("RootView entity");
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));

    for outcome in [
        SourceOutcome::Partial(FailureKind::TimedOut),
        SourceOutcome::Unavailable(FailureKind::ProviderFault),
    ] {
        install_fixture(&view, cx, outcome);
        draw(cx, window);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let body = visual
            .debug_bounds("tm-telemetry-ready-body")
            .expect("startup body must render");
        let table = visual
            .debug_bounds("tm-startup-primary-table")
            .expect("startup table must remain mounted");
        let notice = visual
            .debug_bounds("tm-startup-source-notice")
            .expect("degraded startup source must remain visible");
        assert!(
            visual.debug_bounds("boot-timeline").is_none(),
            "constrained height must collapse secondary timeline evidence"
        );
        assert!(
            table.size.height >= px(168.0),
            "partial/failure notice must not squeeze the primary table below its readable minimum: body={body:?}, table={table:?}, notice={notice:?}"
        );
        assert!(
            notice.size.height <= px(64.0),
            "constrained notice must consume its compact allocation: {notice:?}"
        );

        let first_row = visual
            .debug_bounds("tm-startup-row:0")
            .expect("the first startup row must be reachable");
        visual.simulate_click(first_row.center(), Modifiers::none());
        assert!(
            view.read_with(cx, |view, _cx| view.selected_startup.is_some()),
            "automatic evidence collapse must leave real table interaction intact"
        );
    }
}

#[gpui::test]
async fn standard_and_wide_viewports_allocate_timeline_without_displacing_the_table(
    cx: &mut TestAppContext,
) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("RootView entity");
    install_fixture(&view, cx, SourceOutcome::Available);

    cx.simulate_window_resize(window.into(), size(px(1180.0), px(780.0)));
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual
        .debug_bounds("tm-telemetry-ready-body")
        .expect("standard startup body must render");
    let table = visual
        .debug_bounds("tm-startup-primary-table")
        .expect("standard startup table must render");
    let timeline = visual
        .debug_bounds("tm-startup-timeline-expanded")
        .expect("standard space should expose bounded timeline detail");
    assert!(
        table.origin.y + table.size.height <= timeline.origin.y + px(0.5),
        "bounded timeline belongs after, not inside, the table allocation: table={table:?}, timeline={timeline:?}"
    );
    assert!(
        f32::from(table.size.height) >= f32::from(body.size.height) * 0.45,
        "the table must remain the dominant standard-height surface: body={body:?}, table={table:?}"
    );

    cx.simulate_window_resize(window.into(), size(px(1920.0), px(1080.0)));
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let body = visual
        .debug_bounds("tm-telemetry-ready-body")
        .expect("wide startup body must render");
    let table = visual
        .debug_bounds("tm-startup-primary-table")
        .expect("wide startup table must render");
    let side = visual
        .debug_bounds("tm-startup-timeline-side-panel")
        .expect("wide space should move timeline to a side panel");
    assert!(
        table.origin.x + table.size.width <= side.origin.x + px(0.5),
        "the secondary timeline must not overlay the primary table: table={table:?}, side={side:?}"
    );
    assert!(
        f32::from(table.size.height) >= f32::from(body.size.height) * 0.65,
        "a side timeline must preserve nearly all vertical space for the table: body={body:?}, table={table:?}"
    );
}
