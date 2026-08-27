use super::classify_hypervisor_vendor;

#[test]
fn qemu_and_kvm_map_to_kvm() {
    assert_eq!(
        classify_hypervisor_vendor("QEMU Standard PC"),
        Some("KVM".to_string())
    );
    assert_eq!(
        classify_hypervisor_vendor("Genuine KVM"),
        Some("KVM".to_string())
    );
}

#[test]
fn vmware_virtualbox_hyperv_xen_recognised() {
    assert_eq!(
        classify_hypervisor_vendor("VMware, Inc."),
        Some("VMware".to_string())
    );
    assert_eq!(
        classify_hypervisor_vendor("innotek GmbH"),
        Some("VirtualBox".to_string())
    );
    assert_eq!(
        classify_hypervisor_vendor("Microsoft Corporation"),
        Some("Hyper-V".to_string())
    );
    assert_eq!(classify_hypervisor_vendor("Xen"), Some("Xen".to_string()));
}

#[test]
fn bare_metal_vendors_are_none() {
    assert_eq!(classify_hypervisor_vendor("ASUSTeK COMPUTER INC."), None);
    assert_eq!(classify_hypervisor_vendor("LENOVO"), None);
    assert_eq!(classify_hypervisor_vendor("Dell Inc."), None);
    assert_eq!(classify_hypervisor_vendor(""), None);
}
