use std::time::Duration;

use crate::core::alerts::{export_alert_rules_json, import_alert_rules_json};
use crate::core::{AlertMetric, AlertRule, AlertSeverity};
use taskmanager_application::{
    AlertCenter, AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit,
};

use super::{managed_rules, transfer_entries};

fn managed(id: &str, threshold: f32, enabled: bool) -> ManagedAlertRule {
    ManagedAlertRule::new(
        AlertRule::new(
            id,
            AlertMetric::CpuUsagePercent,
            AlertSeverity::Warning,
            threshold,
            Duration::ZERO,
            5.0,
        ),
        enabled,
    )
}

#[test]
fn clipboard_adapter_round_trips_enabled_rules_and_replaces_id_conflicts() {
    let exported = [managed("cpu", 95.0, false), managed("new", 60.0, true)];
    let json = export_alert_rules_json(&transfer_entries(&exported)).unwrap();
    let imported = import_alert_rules_json(&json).unwrap();
    let mut center = AlertCenter::new([managed("cpu", 80.0, true).rule]);
    center
        .edit_rules(ManagedAlertRuleEdit::Import {
            rules: managed_rules(imported),
            mode: AlertRuleImportMode::Merge(
                crate::core::alerts::AlertRuleConflictPolicy::ReplaceExisting,
            ),
        })
        .unwrap();

    assert_eq!(center.managed_rules()[0], exported[0]);
    assert_eq!(center.managed_rules()[1], exported[1]);
}

#[test]
fn clipboard_adapter_leaves_current_rules_untouched_when_json_is_bad() {
    let current = vec![managed("cpu", 80.0, true)];
    let imported = import_alert_rules_json("not-json");
    assert!(imported.is_err());
    assert_eq!(current, vec![managed("cpu", 80.0, true)]);
}
