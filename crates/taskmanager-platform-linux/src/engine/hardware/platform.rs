//! Linux platform topology and virtualization detection.
//!
//! Counts CPU sockets from `physical_package_id` and detects the hypervisor
//! via `/sys/hypervisor/type`, the `hypervisor` CPUID flag, and DMI vendor
//! strings.
use std::fs;

#[cfg(target_os = "linux")]
use taskmanager_core::classify_hypervisor_vendor;

use super::num_cpu_dirs;
use super::read_sysfs_string;

/// Number of CPU sockets (physical packages) by counting distinct
/// `physical_package_id` values. Missing topology nodes stay `None`; a
/// container must not be reported as one socket merely because one is common.
#[cfg(target_os = "linux")]
pub fn detect_socket_count() -> Option<u16> {
    let mut seen: std::collections::HashSet<u16> = std::collections::HashSet::new();
    for cpu in 0..num_cpu_dirs() {
        let p = format!("/sys/devices/system/cpu/cpu{cpu}/topology/physical_package_id");
        if let Some(s) = read_sysfs_string(&p)
            && let Ok(n) = s.trim().parse::<u16>()
        {
            seen.insert(n);
        }
    }
    (!seen.is_empty()).then(|| u16::try_from(seen.len()).unwrap_or(u16::MAX))
}

/// Read the DMI `sys_vendor` string, trying the canonical
/// `/sys/class/dmi/id` path first and the virtual-DMI fallback second. Returns
/// `None` when DMI is unavailable (non-x86, firmware that hides it, or a
/// container without `/sys/class/dmi` mounted) or when the field is blank.
#[cfg(target_os = "linux")]
fn dmi_sys_vendor() -> Option<String> {
    read_sysfs_string("/sys/class/dmi/id/sys_vendor")
        .or_else(|| read_sysfs_string("/sys/devices/virtual/dmi/id/sys_vendor"))
        .filter(|s| !s.is_empty())
}

/// Detect whether the host is running under a hypervisor. Returns the
/// hypervisor label when detected, `None` on bare metal.
///
/// Strategy (first hit wins — cheapest source first):
/// 1. `/sys/hypervisor/type` — Xen dom0/domU sets this to `xen`; some KVM
///    setups set `kvm`. Returned verbatim.
/// 2. `/proc/cpuinfo` `flags:` line containing the `hypervisor` CPUID bit
///    (set by KVM, VMware, Hyper-V, Xen HVM, ...). Refine with the DMI vendor
///    for a friendlier label; default to `KVM` when the bit is set but DMI is
///    unhelpful (the bit's most common producer on Linux).
/// 3. DMI `sys_vendor` — recognised hypervisor vendors (QEMU/KVM, VMware,
///    VirtualBox, Microsoft/Hyper-V, Xen) on hosts where neither of the above
///    is available.
#[cfg(target_os = "linux")]
pub fn detect_virtualization() -> Option<String> {
    // 1. /sys/hypervisor/type
    if let Some(t) = read_sysfs_string("/sys/hypervisor/type")
        && !t.is_empty()
    {
        return Some(t);
    }

    // 2. /proc/cpuinfo hypervisor CPUID flag.
    let has_hv_flag = fs::read_to_string("/proc/cpuinfo")
        .map(|ci| {
            ci.lines()
                .any(|l| l.starts_with("flags") && l.split_whitespace().any(|f| f == "hypervisor"))
        })
        .unwrap_or(false);
    if has_hv_flag {
        if let Some(label) = dmi_sys_vendor().and_then(|v| classify_hypervisor_vendor(&v)) {
            return Some(label);
        }
        return Some("KVM".to_string());
    }

    // 3. DMI vendor fallback (recognised hypervisor vendors only).
    if let Some(label) = dmi_sys_vendor().and_then(|v| classify_hypervisor_vendor(&v)) {
        return Some(label);
    }

    None
}

// ── macOS / Windows stubs ────────────────────────────────────────────────────
// `/sys/devices/system/cpu/.../topology` and
// `/proc/cpuinfo`+`/sys/hypervisor/type` virtualisation probes are Linux-only.
// Off Linux these return the type's empty/None default so the Linux adapter
// remains cross-compilable for architecture checks.
// The pure `classify_hypervisor_vendor` helper above stays cross-platform so its
// unit tests run on every OS.
// This crate intentionally owns only the Linux provider.
#[cfg(not(target_os = "linux"))]
pub fn detect_socket_count() -> Option<u16> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn detect_virtualization() -> Option<String> {
    None
}
