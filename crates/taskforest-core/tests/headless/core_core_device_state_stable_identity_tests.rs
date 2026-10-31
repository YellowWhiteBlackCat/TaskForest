use super::{
    StableDeviceSelection, clean_identity, stable_disk_id, stable_gpu_id, stable_network_id,
};

#[test]
fn clean_identity_strips_to_lowercase_alphanumerics_and_separators() {
    assert_eq!(
        clean_identity("  NVMe-SN 1234  "),
        Some("nvme-sn1234".into()),
        "whitespace is filtered out, not converted"
    );
    assert_eq!(clean_identity("sda"), Some("sda".into()));
    assert_eq!(
        clean_identity(""),
        None,
        "empty input has no stable identity"
    );
    assert_eq!(
        clean_identity("!!! ???"),
        None,
        "identity with no usable characters is not stable"
    );
}

#[test]
fn disk_id_prefers_wwid_then_serial_then_path_and_never_empty() {
    assert_eq!(
        stable_disk_id("sda", Some("WWN-0x1234"), Some("SER-9")),
        "disk:wwid:wwn-0x1234"
    );
    assert_eq!(
        stable_disk_id("sda", None, Some("SERIAL-42")),
        "disk:serial:serial-42"
    );
    assert_eq!(
        stable_disk_id("sda", None, None),
        "disk:path:sda",
        "name-only disk falls back to the path identity"
    );
    assert_eq!(
        stable_disk_id("!!", None, None),
        "disk:path:unknown",
        "an unusable name still yields a stable (unknown) identity"
    );
}

#[test]
fn network_id_prefers_mac_then_name() {
    assert_eq!(
        stable_network_id("enp0s3", Some("AA:BB:CC:00:11:22")),
        "net:mac:aa:bb:cc:00:11:22"
    );
    assert_eq!(stable_network_id("enp0s3", None), "net:name:enp0s3");
    assert_eq!(stable_network_id("!!", None), "net:name:unknown");
}

#[test]
fn gpu_id_prefers_pci_then_card_name() {
    assert_eq!(
        stable_gpu_id("Intel Arc A770", Some("0000:03:00.0")),
        "gpu:pci:0000:03:00.0"
    );
    assert_eq!(
        stable_gpu_id("Intel Arc A770", None),
        "gpu:drm:intelarca770",
        "spaces are filtered out of the card name"
    );
    assert_eq!(stable_gpu_id("!!", None), "gpu:drm:unknown");
}

#[test]
fn selection_keeps_the_selected_id_until_cleared() {
    let mut selection = StableDeviceSelection::default();
    assert_eq!(selection.selected_id(), None);
    selection.select("disk:wwid:abc");
    assert_eq!(selection.selected_id(), Some("disk:wwid:abc"));
    selection.select("net:mac:aa:bb");
    assert_eq!(selection.selected_id(), Some("net:mac:aa:bb"));
    selection.clear();
    assert_eq!(
        selection.selected_id(),
        None,
        "clear must drop the selection (a `→ None` mutation is caught)"
    );
}
