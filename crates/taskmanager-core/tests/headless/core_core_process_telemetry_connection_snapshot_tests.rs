use super::*;

#[test]
fn traffic_provenance_roundtrips_and_old_snapshots_default_safely() {
    let snapshot = ProcessNetworkSnapshot {
        traffic_failure: Some(FailureKind::Rejected),
        traffic_provider: Some(ProviderId::borrowed("linux.ebpf.aya")),
        ..Default::default()
    };
    let json = serde_json::to_string(&snapshot).expect("serialize network snapshot");
    let decoded: ProcessNetworkSnapshot =
        serde_json::from_str(&json).expect("deserialize network snapshot");
    assert_eq!(decoded.traffic_failure, Some(FailureKind::Rejected));
    assert_eq!(
        decoded.traffic_provider.as_ref().map(ProviderId::as_str),
        Some("linux.ebpf.aya")
    );

    let mut old_json =
        serde_json::to_value(ProcessNetworkSnapshot::default()).expect("serialize old fixture");
    let object = old_json
        .as_object_mut()
        .expect("network snapshot serializes as an object");
    object.remove("traffic_failure");
    object.remove("traffic_provider");
    let old: ProcessNetworkSnapshot =
        serde_json::from_value(old_json).expect("deserialize pre-provenance snapshot");
    assert_eq!(old.traffic_failure, None);
    assert_eq!(old.traffic_provider, None);
}
