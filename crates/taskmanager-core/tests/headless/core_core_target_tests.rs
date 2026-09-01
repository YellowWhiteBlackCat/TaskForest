use super::*;

#[test]
fn opaque_targets_keep_semantics_but_serialize_as_strings() {
    let service = ServiceId::new("fixture.service");
    let session = SessionId::new("7");
    let storage = StorageDeviceKey::new("nvme0n1");

    assert_eq!(
        serde_json::to_string(&service).expect("serialize service"),
        "\"fixture.service\""
    );
    assert_eq!(
        serde_json::to_string(&session).expect("serialize session"),
        "\"7\""
    );
    assert_eq!(session.as_str(), "7");
    assert_eq!(storage.into_string(), "nvme0n1");
}
