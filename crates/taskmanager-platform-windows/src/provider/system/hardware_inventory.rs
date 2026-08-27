//! Hardware inventory for the Windows system domain.
//!
//! The stable host/kernel facts come from `sysinfo`'s safe Windows backend;
//! advertised base clock data comes from the safe CPUID reader and package
//! count comes from the ARP `Uninstall` registry trees via the safe
//! `windows-registry` wrapper. The static display inventory comes from the
//! audited EnumDisplayDevices + cached-EDID boundary combined with the shared
//! pure EDID parser. Firmware/SMBIOS fields remain typed `Unsupported`; this
//! provider never reaches WMI or a command interpreter and never invents
//! vendor/version strings.

use taskmanager_core::{
    ComputeTopology, DisplayInfo, FailureKind, HardwareInfo, HostIdentity, KernelInfo, ProviderId,
};
use taskmanager_platform_contract::{
    CompositeSourceSnapshot, ProviderFailure, SourceOutcome, SourceStatus,
};
use taskmanager_platform_provider::HardwareInventoryProvider;

use super::{HARDWARE_INVENTORY_PROVIDER, available_source, unavailable_source};

const DISPLAY_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.hardware.display.enum-registry");
const PACKAGE_COUNT_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.hardware.packages.registry");

pub struct WinHardwareInventoryProvider {
    system: sysinfo::System,
}

impl WinHardwareInventoryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
        }
    }
}

impl Default for WinHardwareInventoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareInventoryProvider for WinHardwareInventoryProvider {
    fn refresh(&mut self) -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure> {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();

        let host = HostIdentity {
            os_name: sysinfo::System::long_os_version(),
            os_version: sysinfo::System::os_version(),
            hostname: sysinfo::System::host_name(),
            ..HostIdentity::default()
        };
        let kernel = KernelInfo {
            version: sysinfo::System::kernel_version(),
            // Windows build, loaded-module count, and command line need a
            // dedicated safe native provider and are intentionally absent.
            build: None,
            modules_count: None,
            command_line: None,
        };
        let cpu_brand = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty());
        // Static base comes only from a static source (CPUID 0x16 /
        // PROCESSOR_POWER_INFORMATION.MaxMhz); a live sample is never a base.
        let (base_frequency_mhz, _) = super::cpu_info::advertised_frequencies_mhz();
        let native_topology = taskmanager_windows_api::processor_topology().ok();
        let core_breakdown = native_topology
            .as_ref()
            .and_then(|facts| facts.core_breakdown)
            .map(|b| taskmanager_core::CoreBreakdown {
                p_cores: b.p_cores,
                e_cores: b.e_cores,
                lp_cores: b.lp_cores,
            });
        let cpu_types = native_topology
            .as_ref()
            .map(|facts| {
                facts
                    .cpu_types
                    .iter()
                    .map(|kind| match kind {
                        taskmanager_windows_api::WindowsCpuType::Performance => {
                            taskmanager_core::CpuType::Performance
                        }
                        taskmanager_windows_api::WindowsCpuType::Efficient => {
                            taskmanager_core::CpuType::Efficient
                        }
                        taskmanager_windows_api::WindowsCpuType::LowPower => {
                            taskmanager_core::CpuType::LowPower
                        }
                        taskmanager_windows_api::WindowsCpuType::Unknown => {
                            taskmanager_core::CpuType::Unknown
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let topology = ComputeTopology {
            cpu_brand,
            logical_cpu_count: Some(self.system.cpus().len()),
            socket_count: native_topology
                .as_ref()
                .and_then(|facts| facts.socket_count),
            core_breakdown: core_breakdown.unwrap_or_default(),
            cpu_types,
            total_memory_mb: Some(self.system.total_memory() / 1024),
            base_frequency_mhz,
            instruction_features: super::cpu_info::detected_instruction_features(),
        };
        let mut sources = vec![available_source(HARDWARE_INVENTORY_PROVIDER, 1)];
        if native_topology.is_some() || base_frequency_mhz.is_some() {
            sources.push(available_source(HARDWARE_INVENTORY_PROVIDER, 1));
        } else {
            sources.push(unavailable_source(
                HARDWARE_INVENTORY_PROVIDER,
                FailureKind::Unsupported,
            ));
        }
        let firmware = super::smbios_info::query_firmware_info().unwrap_or_default();
        let (displays, display_status) = collect_displays();
        sources.push(display_status);
        let (package_count, package_status) = package_count_facts();
        sources.push(package_status);
        let host = HostIdentity {
            package_count,
            ..host
        };
        let hardware =
            HardwareInfo::from_fragments_with_displays(host, kernel, topology, firmware, displays);
        Ok(CompositeSourceSnapshot::new(hardware, sources))
    }
}

/// Count installed applications from the ARP `Uninstall` registry trees.
///
/// Only MSI/ARP registration entries are consulted (Add/Remove Programs'
/// own backing store): a subkey counts when it carries a non-empty
/// `DisplayName` value, which is the same filter the Settings app applies.
/// The three hives cover per-machine 64-bit, 32-bit-on-64-bit, and per-user
/// installs; nothing outside the `Uninstall` trees is parsed. The count has
/// no package-manager attribution — Windows has no single native package
/// database behind this list, so `package_manager`/`package_manager_version`
/// stay the honest `None` rather than borrowing a name.
///
/// The returned source receipt records per-hive outcome: a hive missing its
/// `Uninstall` tree (e.g. `Wow6432Node` on 32-bit hosts) is an honest
/// absence, a denied or failing hive degrades the outcome to
/// `Partial`/`Unavailable` while keeping any successfully counted siblings,
/// and off-Windows builds keep the never-observed state (registry-backed
/// startup provider precedent: `MissingDependency`).
fn package_count_facts() -> (Option<u64>, SourceStatus) {
    package_count_facts_for_target()
}

#[cfg(windows)]
fn package_count_facts_for_target() -> (Option<u64>, SourceStatus) {
    const MAX_UNINSTALL_SUBKEYS_PER_HIVE: usize = 4_096;
    const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_PATH_WOW64: &str =
        r"SOFTWARE\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    // `windows-registry` errors carry a Win32 HRESULT; the two codes that
    // change the bookkeeping are compared by value so this adapter never
    // names the wrapper's error type.
    const HRESULT_WIN32_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;
    const HRESULT_WIN32_ACCESS_DENIED: i32 = 0x8007_0005_u32 as i32;

    enum HiveScan {
        Counted(u64),
        Absent,
        Failed(FailureKind),
    }
    fn scan_uninstall_hive(root: &windows_registry::Key, path: &str) -> HiveScan {
        let key = match root.open(path) {
            Ok(key) => key,
            Err(error) if error.code().0 == HRESULT_WIN32_FILE_NOT_FOUND => {
                return HiveScan::Absent;
            }
            Err(error) if error.code().0 == HRESULT_WIN32_ACCESS_DENIED => {
                return HiveScan::Failed(FailureKind::PermissionDenied);
            }
            Err(_) => return HiveScan::Failed(FailureKind::ProviderFault),
        };
        let names = match key.keys() {
            Ok(names) => names,
            Err(_) => return HiveScan::Failed(FailureKind::ProviderFault),
        };
        let mut count = 0_u64;
        for (index, name) in names.enumerate() {
            if index >= MAX_UNINSTALL_SUBKEYS_PER_HIVE {
                // Beyond the fixed bound the hive is only partially read;
                // stop counting and let the truncation surface below.
                return HiveScan::Failed(FailureKind::ProviderFault);
            }
            // An entry that cannot be opened or carries no usable
            // DisplayName is not an installed app; skipping it mirrors how
            // the Settings app ignores broken ARP rows.
            if let Ok(display_name) = key
                .open(&name)
                .and_then(|entry| entry.get_string("DisplayName"))
                && !display_name.trim().is_empty()
            {
                count += 1;
            }
        }
        HiveScan::Counted(count)
    }

    let scans = [
        scan_uninstall_hive(windows_registry::LOCAL_MACHINE, UNINSTALL_PATH),
        scan_uninstall_hive(windows_registry::LOCAL_MACHINE, UNINSTALL_PATH_WOW64),
        scan_uninstall_hive(windows_registry::CURRENT_USER, UNINSTALL_PATH),
    ];
    let counted: Option<u64> = scans
        .iter()
        .filter_map(|scan| match scan {
            HiveScan::Counted(count) => Some(*count),
            _ => None,
        })
        .reduce(u64::saturating_add);
    let failure = scans.iter().find_map(|scan| match scan {
        HiveScan::Failed(FailureKind::PermissionDenied) => Some(FailureKind::PermissionDenied),
        HiveScan::Failed(_) => Some(FailureKind::ProviderFault),
        _ => None,
    });
    match (counted, failure) {
        (Some(count), None) => (
            Some(count),
            SourceStatus {
                provider: PACKAGE_COUNT_PROVIDER,
                outcome: SourceOutcome::Available,
                item_count: count as usize,
            },
        ),
        (Some(count), Some(failure)) => (
            Some(count),
            SourceStatus {
                provider: PACKAGE_COUNT_PROVIDER,
                outcome: SourceOutcome::Partial(failure),
                item_count: count as usize,
            },
        ),
        (None, Some(failure)) => (None, unavailable_source(PACKAGE_COUNT_PROVIDER, failure)),
        // Every hive lacking its Uninstall tree is not "zero packages"; it
        // is a registry without this source at all.
        (None, None) => (
            None,
            unavailable_source(PACKAGE_COUNT_PROVIDER, FailureKind::MissingDependency),
        ),
    }
}

#[cfg(not(windows))]
fn package_count_facts_for_target() -> (Option<u64>, SourceStatus) {
    (
        None,
        unavailable_source(PACKAGE_COUNT_PROVIDER, FailureKind::MissingDependency),
    )
}

/// Collect the static monitor inventory from the audited boundary plus the
/// shared EDID parser.
///
/// An attached output stays visible even when its cached EDID is absent or
/// fails validation — the row then carries only its connector identity,
/// mirroring how the Linux DRM inventory treats an unreadable EDID. The
/// connector is the monitor's registry instance path (stable across
/// re-enumeration and reboot, unique per monitor+port); the GDI device name
/// is the fallback when the monitor reports no instance.
fn collect_displays() -> (Vec<DisplayInfo>, SourceStatus) {
    let monitors = match taskmanager_windows_api::enumerate_display_monitors() {
        Ok(monitors) => monitors,
        Err(failure) => {
            return (
                Vec::new(),
                unavailable_source(DISPLAY_INVENTORY_PROVIDER, display_failure_kind(failure)),
            );
        }
    };

    let mut displays = Vec::new();
    let mut worst: Option<FailureKind> = None;
    for monitor in monitors.into_iter().filter(|monitor| monitor.is_active) {
        let connector = monitor
            .monitor_instance
            .clone()
            .unwrap_or_else(|| monitor.device_name.clone());
        let display = match monitor.edid.as_deref() {
            Some(edid) => match taskmanager_platform_portable::parse_edid(edid) {
                Some(facts) => {
                    let taskmanager_platform_portable::EdidFacts {
                        manufacturer,
                        model,
                        serial,
                        width_mm,
                        height_mm,
                        width_px,
                        height_px,
                        refresh_hz,
                        hdr_supported,
                    } = facts;
                    DisplayInfo {
                        connector,
                        manufacturer,
                        model,
                        serial,
                        width_mm,
                        height_mm,
                        width_px,
                        height_px,
                        refresh_hz,
                        hdr_supported,
                    }
                }
                // Present but invalid EDID: a provider fault, not a missing
                // capability.
                None => {
                    worst = select_failure(worst, FailureKind::ProviderFault);
                    DisplayInfo {
                        connector,
                        ..DisplayInfo::default()
                    }
                }
            },
            // No cached EDID: an honest absence (remote/virtual/override-
            // shadowed display), never a fabricated block.
            None => {
                worst = select_failure(worst, FailureKind::Unsupported);
                DisplayInfo {
                    connector,
                    ..DisplayInfo::default()
                }
            }
        };
        displays.push(display);
    }

    let outcome = match (&displays.is_empty(), worst) {
        (false, None) => SourceOutcome::Available,
        (false, Some(failure)) => SourceOutcome::Partial(failure),
        (true, Some(failure)) => SourceOutcome::Unavailable(failure),
        (true, None) => SourceOutcome::Empty,
    };
    let count = displays.len();
    (
        displays,
        SourceStatus {
            provider: DISPLAY_INVENTORY_PROVIDER,
            outcome,
            item_count: count,
        },
    )
}

/// Keep the first, highest-priority EDID-related failure for the source
/// receipt, matching the Linux display inventory's failure precedence.
fn select_failure(current: Option<FailureKind>, candidate: FailureKind) -> Option<FailureKind> {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => Some(current),
        _ => Some(candidate),
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 8,
        FailureKind::PermissionDenied => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::TimedOut => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn display_failure_kind(failure: taskmanager_windows_api::WindowsApiError) -> FailureKind {
    match failure {
        taskmanager_windows_api::WindowsApiError::PermissionDenied => FailureKind::PermissionDenied,
        taskmanager_windows_api::WindowsApiError::Unsupported => FailureKind::Unsupported,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_hardware_inventory.rs"]
mod tests;
