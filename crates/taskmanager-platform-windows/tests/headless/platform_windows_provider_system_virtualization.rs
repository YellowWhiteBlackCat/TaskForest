use super::{smbios_guest_label, smbios_guest_marker};

#[test]
fn smbios_guest_marker_recognises_real_vms_but_not_vbs_hosts() {
    assert!(
        smbios_guest_marker(Some("Microsoft Corporation"), Some("Virtual Machine")),
        "Hyper-V guests carry the Virtual Machine product name"
    );
    assert!(
        smbios_guest_marker(Some("VMware, Inc."), Some("VMware Virtual Platform")),
        "VMware guests carry a VMM vendor marker"
    );
    assert!(
        !smbios_guest_marker(Some("LENOVO"), Some("ThinkBook 16 G8+ IPH")),
        "a physical OEM must not look like a guest"
    );
    assert!(
        !smbios_guest_marker(Some("Microsoft Corporation"), Some("Surface Pro 9")),
        "a physical Surface must not look like a Hyper-V guest"
    );
}

#[test]
fn smbios_guest_label_maps_hyperv_guests_and_bare_metal() {
    assert_eq!(
        smbios_guest_label(Some("Microsoft Corporation"), Some("Virtual Machine")),
        Some("Hyper-V".to_string())
    );
    assert_eq!(smbios_guest_label(Some("LENOVO"), Some("ThinkBook")), None);
    assert_eq!(smbios_guest_label(None, None), None);
}

#[cfg(not(windows))]
#[test]
fn off_windows_detection_uses_smbios_only() {
    use super::detect_virtualization;

    assert_eq!(
        detect_virtualization(Some("LENOVO"), Some("ThinkBook")),
        None
    );
    assert_eq!(
        detect_virtualization(Some("VMware, Inc."), Some("VMware Virtual Platform")),
        Some("VMware".to_string())
    );
}
