use super::{boot_timeline_rows, critical_chain_summary, failed_units_summary};
use taskmanager_application::i18n;
use taskmanager_core::core::startup::{
    BootTimeline, StartupBootEvidenceSnapshot, StartupCriticalChainNode, StartupEvidenceFailure,
    StartupFailedUnit,
};

fn failed(unit: &str) -> StartupFailedUnit {
    StartupFailedUnit {
        unit: unit.to_string(),
        load_state: "loaded".into(),
        active_state: "failed".into(),
        sub_state: "failed".into(),
        description: String::new(),
    }
}

fn chain(unit: &str, duration_ms: Option<u64>) -> StartupCriticalChainNode {
    StartupCriticalChainNode {
        unit: unit.to_string(),
        activated_at_ms: None,
        duration_ms,
    }
}

#[test]
fn failed_units_summary_stays_honest_for_empty_failure_and_populated_states() {
    let empty = StartupBootEvidenceSnapshot::default();
    assert_eq!(failed_units_summary(&empty).as_deref(), Some("0"));

    let failing = StartupBootEvidenceSnapshot {
        failed_units_failure: Some(StartupEvidenceFailure::MissingTool),
        ..Default::default()
    };
    assert_eq!(
        failed_units_summary(&failing).as_deref(),
        Some(i18n::t("startup.evidence_unavailable"))
    );

    let populated = StartupBootEvidenceSnapshot {
        failed_units: vec![failed("acpid.service"), failed("bluetooth.service")],
        ..Default::default()
    };
    assert_eq!(
        failed_units_summary(&populated).as_deref(),
        Some("2 · acpid.service · bluetooth.service")
    );
}

#[test]
fn failed_units_summary_truncates_long_name_lists() {
    let mut populated = StartupBootEvidenceSnapshot::default();
    for index in 0..5 {
        populated
            .failed_units
            .push(failed(&format!("unit{index}.service")));
    }
    let summary = failed_units_summary(&populated).expect("populated summary");
    assert!(summary.contains("unit0.service"), "{summary}");
    assert!(summary.contains("unit1.service"), "{summary}");
    assert!(!summary.contains("unit2.service"), "{summary}");
}

#[test]
fn critical_chain_summary_totals_measured_time_and_drops_gaps() {
    let mut snapshot = StartupBootEvidenceSnapshot {
        critical_chain: vec![
            chain("systemd-journald.service", Some(120)),
            chain("graphical.target", None),
            chain("foo.service", Some(30)),
        ],
        ..Default::default()
    };
    let summary = critical_chain_summary(&snapshot).expect("measured chain");
    assert_eq!(summary, "150 ms · systemd-journald.service");

    snapshot.critical_chain_failure = Some(StartupEvidenceFailure::PermissionDenied);
    assert_eq!(
        critical_chain_summary(&snapshot).as_deref(),
        Some(i18n::t("startup.evidence_unavailable"))
    );

    snapshot.critical_chain_failure = None;
    snapshot.critical_chain = vec![chain("graphical.target", None)];
    assert_eq!(critical_chain_summary(&snapshot).as_deref(), Some("—"));
}

#[test]
fn boot_timeline_rows_stay_silent_until_typed_evidence_arrives() {
    assert!(boot_timeline_rows(&StartupBootEvidenceSnapshot::default()).is_none());

    let failing = StartupBootEvidenceSnapshot {
        critical_chain_failure: Some(StartupEvidenceFailure::MissingTool),
        ..Default::default()
    };
    assert!(
        boot_timeline_rows(&failing).is_none(),
        "typed failure must suppress the waterfall, never render stale bars"
    );
}

#[test]
fn boot_timeline_rows_project_measured_chain_without_placing_untimed() {
    let mut snapshot = StartupBootEvidenceSnapshot {
        critical_chain: vec![
            chain("dbus.service", Some(1200)),
            chain("graphical.target", None),
            chain("multi-user.target", Some(2500)),
        ],
        ..Default::default()
    };
    snapshot.critical_chain[0].activated_at_ms = Some(500);
    snapshot.critical_chain[2].activated_at_ms = Some(3000);

    let timeline: BootTimeline = boot_timeline_rows(&snapshot).expect("measured chain");
    assert_eq!(timeline.total_ms, 5_500);
    assert_eq!(timeline.segments.len(), 2);
    assert_eq!(timeline.segments[0].unit, "dbus.service");
    assert_eq!(timeline.segments[1].unit, "multi-user.target");
    assert_eq!(timeline.untimed_count, 1);
    assert_eq!(timeline.untimed_units, ["graphical.target"]);
}
