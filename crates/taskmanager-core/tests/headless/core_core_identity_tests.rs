use super::DeviceId;

#[test]
fn string_representation_round_trips_verbatim() {
    let id = DeviceId::new("nvme0n1");
    assert_eq!(id.as_str(), "nvme0n1");
    assert_eq!(id.to_string(), "nvme0n1", "Display must render the raw id");
    assert_eq!(id.clone().into_string(), "nvme0n1");
    assert_eq!(
        <DeviceId as AsRef<str>>::as_ref(&id),
        "nvme0n1",
        "AsRef must expose the raw id"
    );
}

#[test]
fn empty_device_id_is_valid_and_serializes_transparently() {
    let id = DeviceId::new("");
    assert_eq!(id.as_str(), "");
    let json = serde_json::to_string(&id).expect("serialize DeviceId");
    assert_eq!(json, r#""""#, "transparent serialization of the raw string");
    let decoded: DeviceId = serde_json::from_str(&json).expect("deserialize DeviceId");
    assert_eq!(decoded, id);
}
