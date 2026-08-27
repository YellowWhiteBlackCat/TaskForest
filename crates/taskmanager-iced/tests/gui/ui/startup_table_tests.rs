use super::{BootEvidenceStrip, StartupRow, boot_evidence_strip_data, startup_control_actionable};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    StartupBootEvidenceSnapshot, StartupControlPolicy, StartupCriticalChainNode,
    StartupEvidenceFailure, StartupFailedUnit, StartupImpact, StartupImpactEvidence, StartupScope,
    StartupSource,
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

fn row(control_policy: StartupControlPolicy) -> StartupRow {
    StartupRow {
        id: "startup:test-entry".into(),
        name: "Test Entry".into(),
        enabled: true,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy,
        impact: StartupImpact::Low,
        impact_evidence: StartupImpactEvidence::Measured { duration_ms: 40 },
        exec: "/usr/bin/test-entry".into(),
    }
}

#[test]
fn boot_evidence_strip_projects_every_typed_state_honestly() {
    set_language(Language::En);

    // No snapshot yet: the strip stays silent, never a fabricated zero.
    assert!(boot_evidence_strip_data(None).is_none());

    // Healthy boot: a TRUE empty failed set reads "0" (muted, not danger)
    // and the measured chain totals with its head unit.
    let healthy = StartupBootEvidenceSnapshot {
        critical_chain: vec![
            chain("systemd-journald.service", Some(120)),
            chain("graphical.target", None),
            chain("foo.service", Some(30)),
        ],
        ..StartupBootEvidenceSnapshot::default()
    };
    assert_eq!(
        boot_evidence_strip_data(Some(&healthy)),
        Some(BootEvidenceStrip {
            failed_units: "0".to_string(),
            failed_units_danger: false,
            critical_chain: "150 ms · systemd-journald.service".to_string(),
        })
    );

    // Failed boot: the count plus the first two unit names, danger-flagged.
    let failing_boot = StartupBootEvidenceSnapshot {
        failed_units: vec![failed("acpid.service"), failed("bluetooth.service")],
        critical_chain: vec![chain("multi-user.target", Some(2_500))],
        ..StartupBootEvidenceSnapshot::default()
    };
    assert_eq!(
        boot_evidence_strip_data(Some(&failing_boot)),
        Some(BootEvidenceStrip {
            failed_units: "2 · acpid.service · bluetooth.service".to_string(),
            failed_units_danger: true,
            critical_chain: "2500 ms · multi-user.target".to_string(),
        })
    );

    // Typed failures render "boot evidence unavailable" — the failure on
    // one lane never poisons the other lane's honest value.
    let failing_evidence = StartupBootEvidenceSnapshot {
        failed_units_failure: Some(StartupEvidenceFailure::MissingTool),
        critical_chain_failure: Some(StartupEvidenceFailure::PermissionDenied),
        ..StartupBootEvidenceSnapshot::default()
    };
    let projected =
        boot_evidence_strip_data(Some(&failing_evidence)).expect("typed failure projects");
    assert_eq!(projected.failed_units, "Boot evidence unavailable");
    assert_eq!(projected.critical_chain, "Boot evidence unavailable");
    assert!(!projected.failed_units_danger);

    set_language(Language::En);
}

#[test]
fn boot_evidence_strip_truncates_names_and_keeps_all_untimed_honest() {
    set_language(Language::En);

    // More than two failed units: the count stays exact, the name list is
    // bounded to the first two names.
    let mut many = StartupBootEvidenceSnapshot::default();
    for index in 0..5 {
        many.failed_units
            .push(failed(&format!("unit{index}.service")));
    }
    let projected = boot_evidence_strip_data(Some(&many)).expect("populated summary");
    assert!(projected.failed_units_danger);
    assert!(
        projected
            .failed_units
            .starts_with("5 · unit0.service · unit1.service"),
        "{}",
        projected.failed_units
    );
    assert!(!projected.failed_units.contains("unit2.service"));

    // An all-untimed chain is an honest dash, never a fabricated 0 ms.
    let untimed = StartupBootEvidenceSnapshot {
        critical_chain: vec![chain("graphical.target", None)],
        ..StartupBootEvidenceSnapshot::default()
    };
    let projected = boot_evidence_strip_data(Some(&untimed)).expect("untimed chain");
    assert_eq!(projected.critical_chain, "—");

    // A measured zero duration stays a real value with its head unit.
    let zero = StartupBootEvidenceSnapshot {
        critical_chain: vec![chain("foo.service", Some(0))],
        ..StartupBootEvidenceSnapshot::default()
    };
    let projected = boot_evidence_strip_data(Some(&zero)).expect("measured zero");
    assert_eq!(projected.critical_chain, "0 ms · foo.service");

    set_language(Language::En);
}

#[test]
fn unsupported_policy_rows_never_project_an_actionable_control() {
    // Only an Unsupported policy disarms the toggle; the other two
    // policies stay actionable regardless of the enabled state.
    for (policy, actionable) in [
        (StartupControlPolicy::Direct, true),
        (StartupControlPolicy::UserOverride, true),
        (StartupControlPolicy::Unsupported, false),
    ] {
        assert_eq!(startup_control_actionable(&row(policy)), actionable);
    }
}
