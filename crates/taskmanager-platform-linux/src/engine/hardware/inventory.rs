//! Static Linux hardware inventory composed from five independently fallible sources.
//!
//! Host identity, kernel, CPU topology, firmware, and DRM display probes each
//! carry their own `SourceStatus`; `HardwareInventoryCollector::refresh` folds
//! them into one `HardwareInfo` snapshot without fabricating fields a source
//! could not supply.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sysinfo::System;
use taskmanager_core::CpuInstructionFeature;
use taskmanager_core::core::hardware::{
    ComputeTopology, CoreBreakdown, CpuType, FirmwareInfo, HardwareInfo, HostIdentity, KernelInfo,
};
use taskmanager_platform_contract::{
    CompositeSourceSnapshot, FailureKind, ProviderId, SourceOutcome, SourceStatus,
};

use super::display;
use super::{
    detect_cpu_core_breakdown, detect_cpu_types, detect_socket_count, detect_virtualization,
};

mod compute;

use compute::ComputeTopologySource;

mod system_info;
use system_info::{
    detect_desktop_environment_version, detect_package_count, detect_package_manager,
    detect_window_manager, detect_window_manager_version, normalize_desktop_environment,
    normalize_optional_text, normalize_virtual_terminal,
};

const SYSTEM_PROVIDER: &str = "linux.hardware.system";
const KERNEL_PROVIDER: &str = "linux.hardware.kernel";
const TOPOLOGY_PROVIDER: &str = "linux.hardware.cpu-topology";
const FIRMWARE_PROVIDER: &str = "linux.hardware.firmware";
const DISPLAY_PROVIDER: &str = "linux.hardware.display";

#[derive(Debug)]
struct SourceFragment<T> {
    value: T,
    status: SourceStatus,
}

impl<T> SourceFragment<T> {
    fn new(value: T, provider: &'static str, outcome: SourceOutcome, item_count: usize) -> Self {
        Self {
            value,
            status: SourceStatus {
                provider: ProviderId::borrowed(provider),
                outcome,
                item_count,
            },
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct FailureSummary {
    failure: Option<FailureKind>,
}

impl FailureSummary {
    fn record(&mut self, failure: FailureKind) {
        if self
            .failure
            .is_none_or(|current| failure_priority(failure) > failure_priority(current))
        {
            self.failure = Some(failure);
        }
    }

    fn record_io(&mut self, error: &io::Error) {
        self.record(classify_io_failure(error));
    }

    fn get_or(&self, fallback: FailureKind) -> FailureKind {
        self.failure.unwrap_or(fallback)
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

fn classify_io_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

fn required_source_outcome(
    observed: usize,
    expected: usize,
    failures: &FailureSummary,
) -> SourceOutcome {
    if observed == expected {
        SourceOutcome::Available
    } else if observed == 0 {
        SourceOutcome::Unavailable(failures.get_or(FailureKind::TemporarilyUnavailable))
    } else {
        SourceOutcome::Partial(failures.get_or(FailureKind::TemporarilyUnavailable))
    }
}

#[derive(Debug, Clone)]
struct InventoryPaths {
    proc_root: PathBuf,
    cpu_root: PathBuf,
    base_frequency: PathBuf,
    dmi_roots: [PathBuf; 2],
    efivars_root: PathBuf,
    display_root: PathBuf,
}

impl Default for InventoryPaths {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            cpu_root: PathBuf::from("/sys/devices/system/cpu"),
            base_frequency: PathBuf::from("/sys/devices/system/cpu/cpu0/cpufreq/base_frequency"),
            dmi_roots: [
                PathBuf::from("/sys/class/dmi/id"),
                PathBuf::from("/sys/devices/virtual/dmi/id"),
            ],
            efivars_root: PathBuf::from("/sys/firmware/efi/efivars"),
            display_root: PathBuf::from("/sys/class/drm"),
        }
    }
}

impl InventoryPaths {
    fn uses_native_cpu_root(&self) -> bool {
        self.cpu_root == Path::new("/sys/devices/system/cpu")
    }
}

#[derive(Debug, Default)]
struct SystemProbe {
    os_name: Option<String>,
    os_version: Option<String>,
    kernel_version: Option<String>,
    hostname: Option<String>,
    shell: Option<String>,
    terminal: Option<String>,
    terminal_version: Option<String>,
    locale: Option<String>,
    package_manager: Option<String>,
    package_manager_version: Option<String>,
    package_count: Option<u64>,
    desktop_environment: Option<String>,
    desktop_environment_version: Option<String>,
    windowing_system: Option<String>,
    virtual_terminal: Option<String>,
    window_manager: Option<String>,
    window_manager_version: Option<String>,
    compositor_backend: Option<String>,
    cpu_brand: Option<String>,
    logical_cpu_count: Option<usize>,
    total_memory_mb: Option<u64>,
}

impl SystemProbe {
    fn capture(system: &System) -> Self {
        let cpu_brand = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty());
        let logical_cpu_count = (!system.cpus().is_empty()).then_some(system.cpus().len());
        let total_memory_mb =
            (system.total_memory() > 0).then_some(system.total_memory() / (1024 * 1024));
        let distribution_id = System::distribution_id();
        let (package_manager, package_manager_version) = detect_package_manager(&distribution_id);
        let package_count = detect_package_count(package_manager.as_deref());
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .ok();
        let desktop_environment = desktop.clone().and_then(normalize_desktop_environment);
        let desktop_environment_version = desktop.as_deref().and_then(|desktop| {
            detect_desktop_environment_version(desktop, package_manager.as_deref())
        });
        let wayland = display::probe_wayland();
        let (process_window_manager, process_backend) = detect_window_manager(system);
        let window_manager = wayland
            .as_ref()
            .and_then(|facts| facts.compositor.clone())
            .or(process_window_manager);
        let compositor_backend = wayland
            .as_ref()
            .and_then(|facts| facts.compositor_backend.clone())
            .or(process_backend);
        let window_manager_version =
            detect_window_manager_version(window_manager.as_deref(), package_manager.as_deref());
        let windowing_system = std::env::var("XDG_SESSION_TYPE")
            .ok()
            .and_then(normalize_optional_text);
        let virtual_terminal = std::env::var("XDG_VTNR")
            .ok()
            .and_then(normalize_virtual_terminal);
        let shell = std::env::var("SHELL")
            .ok()
            .and_then(normalize_optional_text);
        let terminal = std::env::var("TERM_PROGRAM")
            .ok()
            .and_then(normalize_optional_text)
            .or_else(|| std::env::var("TERM").ok().and_then(normalize_optional_text));
        let terminal_version = std::env::var("TERM_PROGRAM_VERSION")
            .ok()
            .and_then(normalize_optional_text);
        let locale = std::env::var("LC_ALL")
            .ok()
            .and_then(normalize_optional_text)
            .or_else(|| std::env::var("LANG").ok().and_then(normalize_optional_text));

        Self {
            os_name: System::name().filter(|value| !value.trim().is_empty()),
            os_version: System::os_version().filter(|value| !value.trim().is_empty()),
            kernel_version: System::kernel_version().filter(|value| !value.trim().is_empty()),
            hostname: System::host_name().filter(|value| !value.trim().is_empty()),
            shell,
            terminal,
            terminal_version,
            locale,
            package_manager,
            package_manager_version,
            package_count,
            desktop_environment,
            desktop_environment_version,
            windowing_system,
            virtual_terminal,
            window_manager,
            window_manager_version,
            compositor_backend,
            cpu_brand,
            logical_cpu_count,
            total_memory_mb,
        }
    }
}

struct InventoryContext<'a> {
    system: &'a SystemProbe,
    paths: &'a InventoryPaths,
    virtualization: Option<String>,
}

trait InventorySource {
    type Value;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value>;
}

#[derive(Debug, Default)]
struct SystemIdentitySource;

impl InventorySource for SystemIdentitySource {
    type Value = HostIdentity;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value> {
        let mut failures = FailureSummary::default();
        let init_system =
            read_optional_text(&context.paths.proc_root.join("1/comm"), &mut failures);
        let observed = [
            context.system.os_name.is_some(),
            context.system.os_version.is_some(),
            context.system.hostname.is_some(),
            context.system.shell.is_some(),
            context.system.terminal.is_some(),
            context.system.terminal_version.is_some(),
            context.system.locale.is_some(),
            init_system.is_some(),
            context.system.package_manager.is_some(),
            context.system.package_manager_version.is_some(),
            context.system.package_count.is_some(),
            context.system.desktop_environment.is_some(),
            context.system.desktop_environment_version.is_some(),
            context.system.windowing_system.is_some(),
            context.system.virtual_terminal.is_some(),
            context.system.window_manager.is_some(),
            context.system.window_manager_version.is_some(),
            context.system.compositor_backend.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        SourceFragment::new(
            HostIdentity {
                os_name: context.system.os_name.clone(),
                os_version: context.system.os_version.clone(),
                hostname: context.system.hostname.clone(),
                shell: context.system.shell.clone(),
                terminal: context.system.terminal.clone(),
                terminal_version: context.system.terminal_version.clone(),
                locale: context.system.locale.clone(),
                init_system,
                package_manager: context.system.package_manager.clone(),
                package_manager_version: context.system.package_manager_version.clone(),
                package_count: context.system.package_count,
                desktop_environment: context.system.desktop_environment.clone(),
                desktop_environment_version: context.system.desktop_environment_version.clone(),
                windowing_system: context.system.windowing_system.clone(),
                virtual_terminal: context.system.virtual_terminal.clone(),
                window_manager: context.system.window_manager.clone(),
                window_manager_version: context.system.window_manager_version.clone(),
                compositor_backend: context.system.compositor_backend.clone(),
            },
            SYSTEM_PROVIDER,
            // Eighteen independent session/system facts are counted above. The
            // desktop-version slot is intentionally absent on shells where a
            // safe version source is unavailable, so this threshold must
            // describe the complete current schema rather than the old
            // eight-field identity fragment.
            required_source_outcome(observed, 18, &failures),
            observed,
        )
    }
}

#[derive(Debug, Default)]
struct KernelSource;

impl InventorySource for KernelSource {
    type Value = KernelInfo;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value> {
        let mut failures = FailureSummary::default();
        let modules = read_required_text(&context.paths.proc_root.join("modules"), &mut failures)
            .map(|text| text.lines().filter(|line| !line.is_empty()).count());
        let command_line =
            read_required_text(&context.paths.proc_root.join("cmdline"), &mut failures);
        let build =
            read_linux_kernel_build(&context.paths.proc_root.join("version"), &mut failures);
        let observed = [
            context.system.kernel_version.is_some(),
            modules.is_some(),
            command_line.is_some(),
            build.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();

        SourceFragment::new(
            KernelInfo {
                version: context.system.kernel_version.clone(),
                modules_count: modules,
                command_line,
                build,
            },
            KERNEL_PROVIDER,
            required_source_outcome(observed, 4, &failures),
            observed,
        )
    }
}

fn read_linux_kernel_build(path: &Path, failures: &mut FailureSummary) -> Option<String> {
    let raw = read_required_text(path, failures)?;
    match parse_linux_kernel_build_description(&raw) {
        Some(build) => Some(build),
        None => {
            failures.record(FailureKind::ProviderFault);
            None
        }
    }
}

/// Parse Linux's native kernel record into the platform-neutral build
/// description exposed through [`KernelInfo::build`].
fn parse_linux_kernel_build_description(raw: &str) -> Option<String> {
    let rest = raw.trim().strip_prefix("Linux version ")?;
    let mut parts = rest.splitn(2, char::is_whitespace);
    let release = parts.next()?.trim();
    let build = parts.next()?.trim();
    (!release.is_empty() && !build.is_empty()).then(|| build.to_owned())
}

fn read_required_text(path: &Path, failures: &mut FailureSummary) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text.trim().to_string()),
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

#[derive(Debug, Default)]
struct FirmwareSource;

impl InventorySource for FirmwareSource {
    type Value = FirmwareInfo;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value> {
        let mut failures = FailureSummary::default();
        let dmi_root = readable_dmi_root(&context.paths.dmi_roots, &mut failures);
        let (firmware, observed) = super::firmware::collect_firmware_facts(
            context.virtualization.as_deref(),
            dmi_root.as_deref(),
            &context.paths.efivars_root,
            &mut failures,
        );
        let outcome = match (observed, failures.failure) {
            (_, Some(failure)) if observed > 0 => SourceOutcome::Partial(failure),
            (_, Some(failure)) => SourceOutcome::Unavailable(failure),
            (0, None) => SourceOutcome::Empty,
            (_, None) => SourceOutcome::Available,
        };

        SourceFragment::new(firmware, FIRMWARE_PROVIDER, outcome, observed)
    }
}

#[derive(Debug, Default)]
struct DisplaySource;

impl InventorySource for DisplaySource {
    type Value = Vec<taskmanager_core::DisplayInfo>;

    fn collect(&mut self, context: &InventoryContext<'_>) -> SourceFragment<Self::Value> {
        // Hardware inventory is static: DRM/EDID owns monitor identity and
        // preferred timing. Wayland current mode/HDR/VRR facts belong to a
        // separate runtime display capability and must not refresh this page.
        let (displays, outcome) = display::collect_displays(&context.paths.display_root);
        let item_count = displays.len();
        SourceFragment::new(displays, DISPLAY_PROVIDER, outcome, item_count)
    }
}

fn readable_dmi_root(candidates: &[PathBuf; 2], failures: &mut FailureSummary) -> Option<PathBuf> {
    for candidate in candidates {
        match fs::read_dir(candidate) {
            Ok(_) => return Some(candidate.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => failures.record_io(&error),
        }
    }
    None
}

pub(super) fn read_optional_text(path: &Path, failures: &mut FailureSummary) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let value = text.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

/// Stateful construction root for the five independently fallible pieces of
/// static hardware inventory. The public SPI remains one capability because
/// these pieces share scheduling and are never controlled independently.
#[derive(Debug, Default)]
pub struct HardwareInventoryCollector {
    paths: InventoryPaths,
    system: SystemIdentitySource,
    kernel: KernelSource,
    topology: ComputeTopologySource,
    firmware: FirmwareSource,
    display: DisplaySource,
}

impl HardwareInventoryCollector {
    #[must_use]
    pub fn refresh(&mut self) -> CompositeSourceSnapshot<HardwareInfo> {
        let system = System::new_all();
        let probe = SystemProbe::capture(&system);
        let context = InventoryContext {
            system: &probe,
            paths: &self.paths,
            virtualization: detect_virtualization(),
        };
        let host = self.system.collect(&context);
        let kernel = self.kernel.collect(&context);
        let topology = self.topology.collect(&context);
        let firmware = self.firmware.collect(&context);
        let display = self.display.collect(&context);
        let sources = vec![
            host.status,
            kernel.status,
            topology.status,
            firmware.status,
            display.status,
        ];

        CompositeSourceSnapshot::new(
            HardwareInfo::from_fragments_with_displays(
                host.value,
                kernel.value,
                topology.value,
                firmware.value,
                display.value,
            ),
            sources,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_inventory_tests.rs"]
mod tests;
