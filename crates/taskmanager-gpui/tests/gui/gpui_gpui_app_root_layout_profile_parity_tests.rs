//! Cross-page geometry proof for the shared frame layout budget.

use gpui::{AppContext, TestAppContext, VisualTestContext, WindowHandle, px, size};
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence, StartupScope,
    StartupSource,
};

use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::sidebar::SelectedDevice;
use taskmanager_core::core::{StartupBootEvidenceSnapshot, StartupCriticalChainNode};
use taskmanager_theme::Theme;

fn draw(cx: &mut TestAppContext, window: WindowHandle<RootView>) {
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .expect("profile-parity window must draw");
}

fn startup_entry() -> StartupEntry {
    StartupEntry {
        id: "desktop:profile-parity.desktop".into(),
        name: "Profile parity fixture".into(),
        exec: "/usr/bin/profile-parity".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: "profile-parity.desktop".into(),
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 24 },
    }
}

fn startup_evidence() -> StartupBootEvidenceSnapshot {
    StartupBootEvidenceSnapshot {
        critical_chain: vec![StartupCriticalChainNode {
            unit: "profile-parity.service".into(),
            activated_at_ms: Some(100),
            duration_ms: Some(80),
        }],
        ..StartupBootEvidenceSnapshot::default()
    }
}

fn install_shared_fixture(view: &gpui::Entity<RootView>, cx: &mut TestAppContext) {
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.replace_startup_for_test(vec![startup_entry()], Vec::new());
        view.replace_startup_evidence_for_test(Some(startup_evidence()), None);
        cx.notify();
    });
}

fn select_page(view: &gpui::Entity<RootView>, page: TopPage, cx: &mut TestAppContext) {
    view.update(cx, |view, cx| {
        view.page = page;
        if page == TopPage::Performance {
            view.selected = SelectedDevice::Cpu;
        }
        cx.notify();
    });
}

/// One wide-but-short viewport must remain Wide horizontally on every page,
/// while all three page-specific projections independently honor the shared
/// Constrained vertical axis.
#[gpui::test]
async fn wide_short_profile_is_consistent_across_cpu_apps_and_startup(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("RootView entity");
    install_shared_fixture(&view, cx);
    cx.simulate_window_resize(window.into(), size(px(1920.0), px(540.0)));

    select_page(&view, TopPage::Performance, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(visual.debug_bounds("tm-perf-stats-surface").is_some());
    assert!(visual.debug_bounds("tm-cpu-per-core-matrix").is_none());

    select_page(&view, TopPage::Apps, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(visual.debug_bounds("tm-proc-unified-controls").is_some());
    assert!(visual.debug_bounds("tm-proc-stacked-controls").is_none());
    assert!(visual.debug_bounds("tm-proc-overview-title").is_some());
    assert!(visual.debug_bounds("tm-proc-overview-subtitle").is_none());

    select_page(&view, TopPage::Startup, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let table = visual
        .debug_bounds("tm-startup-primary-table")
        .expect("constrained Startup keeps its primary table");
    assert!(table.size.height >= px(168.0));
    assert!(visual.debug_bounds("boot-timeline").is_none());
}

/// The standard reference viewport must likewise map once and consistently:
/// full CPU inventory, stacked Apps chrome, and bounded expanded Startup
/// evidence all consume the same Standard horizontal/vertical facts.
#[gpui::test]
async fn standard_profile_is_consistent_across_cpu_apps_and_startup(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("RootView entity");
    install_shared_fixture(&view, cx);
    cx.simulate_window_resize(window.into(), size(px(1180.0), px(780.0)));

    select_page(&view, TopPage::Performance, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(visual.debug_bounds("tm-cpu-per-core-matrix").is_some());

    select_page(&view, TopPage::Apps, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(visual.debug_bounds("tm-proc-stacked-controls").is_some());
    assert!(visual.debug_bounds("tm-proc-unified-controls").is_none());
    assert!(visual.debug_bounds("tm-proc-overview-subtitle").is_some());

    select_page(&view, TopPage::Startup, cx);
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(visual.debug_bounds("tm-startup-primary-table").is_some());
    assert!(
        visual
            .debug_bounds("tm-startup-timeline-expanded")
            .is_some()
    );
    assert!(
        visual
            .debug_bounds("tm-startup-timeline-side-panel")
            .is_none()
    );
}
