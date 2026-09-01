//! Dashboard capture-evidence tests.
//!
//! These exercise the dashboard-side readiness gate of `CaptureEvidence`
//! (`on_dashboard_state`, panel routing, history-window anchoring) — the
//! process/snapshot/scenario-token cases live in the parent module.

use super::super::{
    CaptureEvidence, CaptureScenario, DashboardPanel, DashboardState, HistoryWindow, SystemSection,
    SystemSnapshot,
};
use super::PROCESSES_OBSERVED_AT_MS;
use std::sync::Arc;
use taskmanager_telemetry_store::{CorrelatedSystemTelemetryIngestor, TelemetryStore};

fn history_pair() -> (Arc<TelemetryStore>, CorrelatedSystemTelemetryIngestor) {
    TelemetryStore::shared_with_correlated_ingestion(
        taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY,
    )
}

#[test]
fn dashboard_marker_waits_for_live_readiness_and_prepares_exact_target() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::EventCenter));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    assert_eq!(
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000,),
        None
    );
    assert!(!evidence.scenario_ready());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert_eq!(
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000,),
        None
    );
    assert!(!evidence.scenario_ready());
    let mut processes = Vec::new();
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    let panel =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000);
    assert!(evidence.scenario_ready());
    assert_eq!(panel, Some(DashboardPanel::Events));
    let events = evidence
        .take_event_history_fixture()
        .expect("event-center capture supplies shared history");
    assert_eq!(dashboard.events.unread_count(&events), 2);
}

#[test]
fn every_dashboard_capture_token_reaches_its_exact_root_state() {
    fn prepared(
        scenario: CaptureScenario,
    ) -> (DashboardState, Arc<TelemetryStore>, Option<DashboardPanel>) {
        let mut evidence = CaptureEvidence::for_test(Some(scenario));
        let mut dashboard = DashboardState::new();
        let (store, ingestor) = history_pair();
        let mut snapshot = SystemSnapshot::default();
        let mut processes = Vec::new();
        evidence.on_snapshot(&mut snapshot);
        evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);
        let panel = evidence.on_dashboard_state(
            &mut dashboard,
            &store.system_history,
            &ingestor,
            7_200_000,
        );
        assert!(evidence.scenario_ready());
        (dashboard, store, panel)
    }
    let (overview, _, _) = prepared(CaptureScenario::SystemDashboard);
    assert_eq!(overview.section, SystemSection::Dashboard);
    assert_eq!(overview.history_window, HistoryWindow::FifteenMinutes);
    let (hardware, _, _) = prepared(CaptureScenario::SystemHardware);
    assert_eq!(hardware.section, SystemSection::Hardware);
    let (history, store, _) = prepared(CaptureScenario::HistorySixtyMinutes);
    assert_eq!(history.history_window, HistoryWindow::SixtyMinutes);
    assert_eq!(
        history
            .timeline
            .series(&store.system_history, HistoryWindow::SixtyMinutes)
            .covered_ms,
        3_600_000
    );
    assert_eq!(
        prepared(CaptureScenario::AlertRulesManager).2,
        Some(DashboardPanel::AlertRules)
    );
    assert_eq!(
        prepared(CaptureScenario::EventCenter).2,
        Some(DashboardPanel::Events)
    );
    let (saved, _, panel) = prepared(CaptureScenario::SavedViewPresets);
    assert_eq!(panel, Some(DashboardPanel::SavedViews));
    assert!(saved.saved_views.iter().any(|preset| preset.id == 90_000));
}

#[test]
fn system_npu_capture_selects_hardware_but_defers_marker_to_post_layout_scroll() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SystemNpu));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    let mut snapshot = SystemSnapshot::default();
    let mut processes = Vec::new();
    evidence.on_snapshot(&mut snapshot);
    evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);

    let panel =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_200_000);
    assert_eq!(dashboard.section, SystemSection::Hardware);
    assert_eq!(panel, None);
    assert!(
        !evidence.scenario_ready(),
        "the marker belongs to the later visible-scroll terminal"
    );
}

#[test]
fn dashboard_capture_history_stays_anchored_when_a_new_live_frame_arrives() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::HistorySixtyMinutes));
    let mut dashboard = DashboardState::new();
    let (store, ingestor) = history_pair();
    let mut snapshot = SystemSnapshot {
        timestamp_ms: 7_200_000,
        ..Default::default()
    };
    let mut processes = Vec::new();
    evidence.on_snapshot(&mut snapshot);
    evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes);
    let _ = evidence.on_dashboard_state(
        &mut dashboard,
        &store.system_history,
        &ingestor,
        snapshot.timestamp_ms,
    );

    let _ =
        evidence.on_dashboard_state(&mut dashboard, &store.system_history, &ingestor, 7_201_000);

    assert_eq!(
        dashboard
            .timeline
            .series(&store.system_history, HistoryWindow::SixtyMinutes)
            .covered_ms,
        3_600_000
    );
}
