use serde_json::json;
use taskmanager_core::{
    ServiceDeps, ServiceId, ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind,
};

#[test]
fn every_known_relation_kind_has_a_stable_wire_name() {
    for (kind, wire_name) in [
        (ServiceRelationKind::Requires, "requires"),
        (ServiceRelationKind::Wants, "wants"),
        (ServiceRelationKind::Requisite, "requisite"),
        (ServiceRelationKind::BindsTo, "binds_to"),
        (ServiceRelationKind::PartOf, "part_of"),
        (ServiceRelationKind::Conflicts, "conflicts"),
        (ServiceRelationKind::Before, "before"),
        (ServiceRelationKind::After, "after"),
        (ServiceRelationKind::WantedBy, "wanted_by"),
        (ServiceRelationKind::RequiredBy, "required_by"),
        (ServiceRelationKind::UpheldBy, "upheld_by"),
    ] {
        // The wire name is sealed serde-ingress vocabulary: pin the stable
        // spelling through the serde boundary the ingress owns.
        assert_eq!(
            serde_json::from_value::<ServiceRelationKind>(json!(wire_name))
                .expect("deserialize known relation"),
            kind
        );
        assert_eq!(
            serde_json::to_value(&kind).expect("serialize known relation"),
            json!(wire_name)
        );
    }
}

#[test]
fn unknown_relation_kind_round_trips_without_losing_its_name() {
    let kind = serde_json::from_value::<ServiceRelationKind>(json!("propagates_reload_to"))
        .expect("deserialize future relation");

    assert_eq!(
        kind,
        ServiceRelationKind::Unknown("propagates_reload_to".into())
    );
    assert_eq!(
        serde_json::to_value(&kind).expect("serialize future relation"),
        json!("propagates_reload_to")
    );
}

#[test]
fn typed_graph_uses_service_ids_and_projects_legacy_wire_fields() {
    let graph = ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "network.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "local-fs.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Wants, "audit.service"),
        ServiceRelationEdge::new(ServiceRelationKind::After, "network.target"),
        ServiceRelationEdge::new(ServiceRelationKind::WantedBy, "multi-user.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Conflicts, "shutdown.target"),
    ]);

    let deps = ServiceDeps::from_relations(graph);

    assert_eq!(
        deps.relation_projection(&ServiceRelationKind::Requires),
        "network.target local-fs.target"
    );
    assert_eq!(
        deps.relation_projection(&ServiceRelationKind::Wants),
        "audit.service"
    );
    assert_eq!(
        deps.relation_projection(&ServiceRelationKind::After),
        "network.target"
    );
    assert_eq!(
        deps.relation_projection(&ServiceRelationKind::WantedBy),
        "multi-user.target"
    );
    assert_eq!(
        deps.relations()
            .targets(&ServiceRelationKind::Conflicts)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>(),
        ["shutdown.target"]
    );
}

#[test]
fn legacy_wire_shape_still_reads_and_hydrates_typed_edges() {
    let legacy = json!({
        "requires": "network.target local-fs.target",
        "wants": "audit.service",
        "wanted_by": "multi-user.target",
        "after": "basic.target"
    });

    let deps: ServiceDeps = serde_json::from_value(legacy.clone()).expect("read legacy payload");

    assert_eq!(deps.relations().len(), 5);
    assert_eq!(
        deps.relation_targets(&ServiceRelationKind::Requires)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>(),
        ["network.target", "local-fs.target"]
    );
    let encoded = serde_json::to_value(&deps).expect("serialize relation payload");
    for field in ["requires", "wants", "wanted_by", "after"] {
        assert_eq!(encoded.get(field), legacy.get(field));
    }
    assert_eq!(
        encoded
            .pointer("/relations/edges/0")
            .expect("serialized typed edge"),
        &json!({"kind": "requires", "target": "network.target"})
    );
}

#[test]
fn typed_only_wire_payload_round_trips_with_legacy_projection() {
    let typed_only = json!({
        "relations": {
            "edges": [
                {"kind": "requires", "target": "network.target"},
                {"kind": "conflicts", "target": "shutdown.target"}
            ]
        }
    });

    let deps: ServiceDeps = serde_json::from_value(typed_only).expect("read typed-only payload");
    let encoded = serde_json::to_value(&deps).expect("serialize typed dependencies");
    let decoded: ServiceDeps =
        serde_json::from_value(encoded.clone()).expect("round-trip typed dependencies");

    assert_eq!(decoded, deps);
    assert_eq!(encoded["requires"], "network.target");
    assert_eq!(encoded["wants"], "");
    assert!(
        decoded
            .relations()
            .edges()
            .contains(&ServiceRelationEdge::new(
                ServiceRelationKind::Conflicts,
                "shutdown.target"
            ))
    );
}

#[test]
fn typed_relation_wins_conflict_and_legacy_fills_only_missing_kinds() {
    let deps: ServiceDeps = serde_json::from_value(json!({
        "requires": "legacy.target",
        "wants": "legacy-want.target",
        "wanted_by": "",
        "after": "",
        "relations": {
            "edges": [
                {"kind": "requires", "target": "typed.target"}
            ]
        }
    }))
    .expect("read mixed dependency payload");

    assert_eq!(
        deps.relation_targets(&ServiceRelationKind::Requires)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>(),
        ["typed.target"]
    );
    assert_eq!(
        deps.relation_targets(&ServiceRelationKind::Wants)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>(),
        ["legacy-want.target"]
    );
    assert_eq!(
        serde_json::to_value(&deps).expect("serialize merged dependency payload")["requires"],
        "typed.target"
    );
}

#[test]
fn replacing_typed_targets_immediately_updates_wire_projection() {
    let mut deps = ServiceDeps::from_relations(ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "old.target"),
    ]));

    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("new.target")],
    );

    let encoded = serde_json::to_value(&deps).expect("serialize replaced dependencies");
    assert_eq!(encoded["requires"], "new.target");
    assert_eq!(
        encoded["relations"]["edges"].as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn unknown_relation_survives_the_service_deps_wire_boundary() {
    let deps: ServiceDeps = serde_json::from_value(json!({
        "relations": {
            "edges": [
                {"kind": "propagates_reload_to", "target": "future.target"}
            ]
        }
    }))
    .expect("read future dependency relation");

    let encoded = serde_json::to_value(&deps).expect("serialize future dependency relation");
    let decoded: ServiceDeps = serde_json::from_value(encoded).expect("round-trip future relation");
    assert!(
        decoded
            .relations()
            .edges()
            .contains(&ServiceRelationEdge::new(
                ServiceRelationKind::Unknown("propagates_reload_to".into()),
                "future.target"
            ))
    );
}

#[test]
fn empty_dependencies_keep_the_exact_legacy_wire_projection() {
    assert_eq!(
        serde_json::to_value(ServiceDeps::default()).expect("serialize empty dependencies"),
        json!({
            "requires": "",
            "wants": "",
            "wanted_by": "",
            "after": ""
        })
    );
}
