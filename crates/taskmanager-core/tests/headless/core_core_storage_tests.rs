use super::*;

#[test]
fn storage_axes_keep_usb_bridge_protocol_independent() {
    let sat = StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let uas = StorageConnection::new(
        StorageProtocol::Scsi,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );

    assert_ne!(sat.protocol, uas.protocol);
    assert_eq!(sat.interconnect, StorageInterconnect::Usb);
    assert_eq!(uas.interconnect, StorageInterconnect::Usb);
}

#[test]
fn extensible_connections_keep_each_typed_axis_without_guessing() {
    let future = StorageConnection::new(
        StorageProtocol::Other,
        StorageInterconnect::Other,
        StorageDeviceKind::Physical,
    );
    assert_eq!(future.protocol, StorageProtocol::Other);
    assert_eq!(future.interconnect, StorageInterconnect::Other);

    let fibre_channel = StorageConnection::new(
        StorageProtocol::Scsi,
        StorageInterconnect::FibreChannel,
        StorageDeviceKind::Physical,
    );
    assert_eq!(fibre_channel.protocol, StorageProtocol::Scsi);
    assert_eq!(
        fibre_channel.interconnect,
        StorageInterconnect::FibreChannel
    );

    let tunneled_nvme = StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::PcieTunnel,
        StorageDeviceKind::Physical,
    );
    assert_eq!(tunneled_nvme.protocol, StorageProtocol::Nvme);
    assert_eq!(tunneled_nvme.interconnect, StorageInterconnect::PcieTunnel);
}

#[test]
fn storage_target_keeps_legacy_device_key_wire_shape() {
    let target = StorageDeviceTarget {
        device_id: DeviceId::new("disk:wwid:fixture"),
        device_generation: DeviceGeneration::INITIAL,
        locator: StorageDeviceKey::new("native-locator"),
    };
    let value = serde_json::to_value(&target).expect("serialize target");
    assert_eq!(value["device_key"], "native-locator");
    assert!(value.get("locator").is_none());

    let decoded: StorageDeviceTarget = serde_json::from_value(serde_json::json!({
        "device_id": "disk:wwid:fixture",
        "device_generation": 2,
        "locator": "new-wire-alias"
    }))
    .expect("deserialize alias");
    assert_eq!(decoded.locator.as_str(), "new-wire-alias");
}
