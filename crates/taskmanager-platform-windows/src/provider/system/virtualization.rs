//! Windows hypervisor detection.
//!
//! The firmware "VT-x enabled" bit (`IsProcessorFeaturePresent`) is NOT a
//! hypervisor fact: it only says hardware virtualization is enabled in
//! firmware, which is true on most bare-metal machines. `HardwareInfo::virt`
//! means "this host runs inside a VM". Windows adds one wrinkle: with VBS /
//! Core Isolation enabled, a physical machine also runs under the Microsoft
//! Hyper-V root partition, so CPUID alone reports "Microsoft Hv". We therefore
//! treat CPUID identity as authoritative for non-Microsoft hypervisors, but
//! require a SMBIOS guest marker (manufacturer/product) before reporting
//! Hyper-V on a machine that may just have VBS enabled.

// Consumed by the `#[cfg(windows)]` SMBIOS firmware query and by the mounted
// headless guest-classification tests; dormant elsewhere.
#[cfg(any(windows, test))]
use taskmanager_core::classify_hypervisor_vendor;

/// Detect whether the Windows host runs under a hypervisor. Returns the
/// hypervisor label when detected, `None` on bare metal (including
/// VBS-enabled physical hosts).
#[cfg(any(windows, test))]
pub(super) fn detect_virtualization(
    smbios_manufacturer: Option<&str>,
    smbios_product: Option<&str>,
) -> Option<String> {
    #[cfg(all(
        windows,
        any(target_arch = "x86", target_arch = "x86_64"),
        not(target_env = "sgx")
    ))]
    {
        let cpuid = raw_cpuid::CpuId::new();
        if let Some(info) = cpuid.get_hypervisor_info() {
            let label = match info.identify() {
                raw_cpuid::Hypervisor::Xen => "Xen",
                raw_cpuid::Hypervisor::VMware => "VMware",
                // VBS-enabled physical hosts also report "Microsoft Hv";
                // only label it a guest when SMBIOS says so.
                raw_cpuid::Hypervisor::HyperV => {
                    return smbios_guest_label(smbios_manufacturer, smbios_product);
                }
                raw_cpuid::Hypervisor::KVM => "KVM",
                raw_cpuid::Hypervisor::QEMU => "QEMU",
                raw_cpuid::Hypervisor::Bhyve => "Bhyve",
                raw_cpuid::Hypervisor::QNX => "QNX",
                raw_cpuid::Hypervisor::ACRN => "ACRN",
                raw_cpuid::Hypervisor::Unknown(..) => {
                    return smbios_guest_label(smbios_manufacturer, smbios_product);
                }
            };
            return Some(label.to_string());
        }
        if cpuid
            .get_feature_info()
            .is_some_and(|features| features.has_hypervisor())
        {
            return smbios_guest_label(smbios_manufacturer, smbios_product);
        }
    }
    smbios_guest_label(smbios_manufacturer, smbios_product)
}

/// SMBIOS-only guest detection: known VMM vendors (VMware/QEMU/VirtualBox/Xen)
/// classify directly; "Microsoft Corporation" alone is NOT enough (Surface and
/// VBS hosts both carry it) — the product must look like a virtual machine.
#[cfg(any(windows, test))]
fn smbios_guest_label(manufacturer: Option<&str>, product: Option<&str>) -> Option<String> {
    if !smbios_guest_marker(manufacturer, product) {
        return None;
    }
    if let Some(label) = manufacturer.and_then(classify_hypervisor_vendor) {
        return Some(label);
    }
    if product
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("virtual machine")
    {
        return Some("Hyper-V".to_string());
    }
    None
}

/// Whether the SMBIOS manufacturer/product combination points at a real guest
/// VM rather than a VBS-enabled physical host.
#[cfg(any(windows, test))]
fn smbios_guest_marker(manufacturer: Option<&str>, product: Option<&str>) -> bool {
    let haystack = format!(
        "{} {}",
        manufacturer.unwrap_or_default(),
        product.unwrap_or_default()
    )
    .to_ascii_lowercase();
    [
        "vmware",
        "qemu",
        "kvm",
        "virtualbox",
        "innotek",
        "xen",
        "bochs",
        "virtual machine",
    ]
    .iter()
    .any(|marker| haystack.contains(marker))
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_virtualization.rs"]
mod tests;
