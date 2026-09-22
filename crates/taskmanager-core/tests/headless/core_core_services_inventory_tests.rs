use super::*;
use crate::core::ServiceRelationEdge;

#[test]
fn legacy_snapshot_without_provider_target_is_read_only_compatible() {
    let item: ServiceItem = serde_json::from_str(
        r#"{
                "name":"demo",
                "status":"Active",
                "description":"",
                "load_state":"loaded",
                "active_state":"active",
                "sub_state":"running",
                "requires":"",
                "wants":"",
                "wanted_by":"",
                "after":""
            }"#,
    )
    .expect("legacy service snapshot remains readable");

    assert_eq!(item.name, "demo");
    assert!(item.id.as_str().is_empty());
    assert!(item.relations().is_empty());
}

#[test]
fn legacy_inventory_relations_hydrate_the_typed_graph() {
    let item: ServiceItem = serde_json::from_value(serde_json::json!({
        "id": "linux.service.systemd:demo.service",
        "name": "demo",
        "status": "Active",
        "description": "",
        "load_state": "loaded",
        "active_state": "active",
        "sub_state": "running",
        "requires": "linux.service.systemd:network.target linux.service.systemd:dbus.service",
        "wants": "",
        "wanted_by": "",
        "after": "linux.service.systemd:basic.target"
    }))
    .expect("legacy inventory payload remains readable");

    assert_eq!(
        item.relation_targets(&ServiceRelationKind::Requires)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>(),
        [
            "linux.service.systemd:network.target",
            "linux.service.systemd:dbus.service"
        ]
    );
    assert_eq!(
        item.relation_projection(&ServiceRelationKind::After),
        "linux.service.systemd:basic.target"
    );
}

#[test]
fn typed_only_inventory_relations_round_trip_with_legacy_projection() {
    let item: ServiceItem = serde_json::from_value(serde_json::json!({
        "id": "linux.service.systemd:demo.service",
        "name": "demo",
        "status": "Active",
        "description": "",
        "load_state": "loaded",
        "active_state": "active",
        "sub_state": "running",
        "relations": {
            "edges": [
                {"kind": "requires", "target": "linux.service.systemd:network.target"},
                {"kind": "conflicts", "target": "linux.service.systemd:shutdown.target"}
            ]
        }
    }))
    .expect("typed-only inventory payload remains readable");

    let encoded = serde_json::to_value(&item).expect("serialize canonical inventory item");
    assert_eq!(encoded["requires"], "linux.service.systemd:network.target");
    assert_eq!(encoded["wants"], "");
    assert_eq!(
        serde_json::from_value::<ServiceItem>(encoded).expect("round-trip inventory item"),
        item
    );
}

#[test]
fn typed_inventory_kind_wins_legacy_conflict_and_unknown_relations_survive() {
    let item: ServiceItem = serde_json::from_value(serde_json::json!({
        "id": "linux.service.systemd:demo.service",
        "name": "demo",
        "status": "Active",
        "description": "",
        "load_state": "loaded",
        "active_state": "active",
        "sub_state": "running",
        "requires": "legacy.service",
        "wants": "legacy-want.service",
        "wanted_by": "",
        "after": "",
        "relations": {
            "edges": [
                {"kind": "requires", "target": "typed.service"},
                {"kind": "propagates_reload_to", "target": "future.service"}
            ]
        }
    }))
    .expect("mixed inventory payload remains readable");

    assert_eq!(
        item.relation_projection(&ServiceRelationKind::Requires),
        "typed.service"
    );
    assert_eq!(
        item.relation_projection(&ServiceRelationKind::Wants),
        "legacy-want.service"
    );
    assert!(item.relations().edges().contains(&ServiceRelationEdge::new(
        ServiceRelationKind::Unknown("propagates_reload_to".into()),
        "future.service"
    )));

    let encoded = serde_json::to_value(&item).expect("serialize merged inventory item");
    assert_eq!(encoded["requires"], "typed.service");
    let decoded: ServiceItem =
        serde_json::from_value(encoded).expect("round-trip unknown inventory relation");
    assert_eq!(decoded, item);
}

#[test]
fn empty_inventory_relations_keep_the_legacy_wire_shape_without_typed_noise() {
    let item = ServiceItem::from_inventory(
        "fixture.service:demo",
        "demo",
        ServiceStatus::Inactive,
        "",
        "loaded",
        "inactive",
        "dead",
    );
    let encoded = serde_json::to_value(item).expect("serialize empty inventory relations");

    assert_eq!(encoded["requires"], "");
    assert_eq!(encoded["wants"], "");
    assert_eq!(encoded["wanted_by"], "");
    assert_eq!(encoded["after"], "");
    assert!(encoded.get("relations").is_none());
}

#[test]
fn service_diagnostics_round_trip_and_failure_precedence_are_explicit() {
    let diagnostics = ServiceDiagnostics {
        result: Some("start-limit-hit".into()),
        exec_main_status: Some(78),
        oom_killed: Some(true),
        memory_max_bytes: Some(1024),
        memory_current_bytes: Some(1024),
        start_limit_hit: Some(true),
        triggers: vec!["demo.socket".into()],
        ..ServiceDiagnostics::default()
    };
    let item = ServiceItem::from_inventory(
        "linux.service.systemd:demo.service",
        "demo",
        ServiceStatus::Failed,
        "",
        "loaded",
        "failed",
        "failed",
    )
    .with_diagnostics(diagnostics.clone());
    let encoded = serde_json::to_value(&item).expect("diagnostics should serialize");
    assert_eq!(encoded["diagnostics"]["oom_killed"], true);
    let decoded: ServiceItem = serde_json::from_value(encoded).expect("diagnostics should decode");
    assert_eq!(decoded.diagnostics(), &diagnostics);
    assert_eq!(
        decoded.diagnostics().failure_cause(),
        Some(ServiceFailureCause::OomKilled)
    );
    assert_eq!(
        decoded.diagnostics().oom_cause(),
        Some(ServiceOomCause::UnitLimit)
    );
    assert_eq!(service_exit_status_label(78), Some("EX_CONFIG"));
    assert_eq!(service_exit_status_label(127), None);
}
