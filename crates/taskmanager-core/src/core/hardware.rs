//! Static hardware inventory: host identity, kernel metadata, compute topology
//! (including P/E/LP-E core breakdown and per-logical-CPU type), and firmware
//! facts, assembled into the compatibility `HardwareInfo` read model.

use serde::{Deserialize, Serialize};

use crate::core::CpuInstructionFeature;

/// Heterogeneous core breakdown when a native provider can classify it.
///
/// Zeroed fields mean that class was not observed. Providers must not put an
/// unclassified homogeneous CPU into `p_cores`; per-logical-CPU
/// [`CpuType::Unknown`] preserves that distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CoreBreakdown {
    pub p_cores: u16,
    pub e_cores: u16,
    pub lp_cores: u16,
}

impl CoreBreakdown {
    pub fn total(&self) -> u16 {
        self.p_cores + self.e_cores + self.lp_cores
    }
}

/// Per-logical-CPU type on a hybrid part (Intel Thread Director / big.LITTLE).
/// `cpu_types[i]` is the type of logical CPU `i`. Unclassified processors stay
/// `Unknown`; a provider must not infer performance cores from missing nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CpuType {
    Performance,
    Efficient,
    LowPower,
    #[default]
    Unknown,
}

impl CpuType {
    pub fn label(self) -> &'static str {
        match self {
            CpuType::Performance => "P-cores",
            CpuType::Efficient => "E-cores",
            CpuType::LowPower => "LP-E-cores",
            CpuType::Unknown => "Cores",
        }
    }
}

/// Stable host identity fields supplied by an OS adapter.
///
/// This is a construction fragment rather than a second read model: native
/// adapters collect it independently and merge it into [`HardwareInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HostIdentity {
    /// Operating-system family or product name, when the native adapter can
    /// identify it. Missing identity must not be replaced with the adapter's
    /// compile-time target.
    #[serde(default)]
    pub os_name: Option<String>,
    /// Native operating-system release or product version.
    #[serde(default)]
    pub os_version: Option<String>,
    /// Host name supplied by the operating system.
    #[serde(default)]
    pub hostname: Option<String>,
    /// Login shell reported by the active user session (`SHELL` on Unix).
    /// This is session metadata, not an executable launch request.
    #[serde(default)]
    pub shell: Option<String>,
    /// Terminal program/type reported by the active session environment.
    #[serde(default)]
    pub terminal: Option<String>,
    /// Terminal program version when the session exports one.
    #[serde(default)]
    pub terminal_version: Option<String>,
    /// Active locale reported by the session environment.
    #[serde(default)]
    pub locale: Option<String>,
    /// PID 1 implementation name when the native adapter can read it.
    #[serde(default)]
    pub init_system: Option<String>,
    /// Native package-manager name when the platform can identify one.
    ///
    /// This is a reported host fact, not a distribution-name inference: a
    /// provider may leave it absent when the executable or package database
    /// cannot be confirmed.
    #[serde(default)]
    pub package_manager: Option<String>,
    /// Version reported by the confirmed package-manager executable/database.
    #[serde(default)]
    pub package_manager_version: Option<String>,
    /// Number of installed packages reported by the native package database.
    /// This is optional because package databases may be unavailable even
    /// when the package-manager executable itself is present.
    #[serde(default)]
    pub package_count: Option<u64>,
    /// Desktop environment name reported by the active graphical session.
    #[serde(default)]
    pub desktop_environment: Option<String>,
    /// Desktop environment version observed for the active graphical session.
    #[serde(default)]
    pub desktop_environment_version: Option<String>,
    /// Windowing system observed for the active graphical session.
    #[serde(default)]
    pub windowing_system: Option<String>,
    /// Virtual terminal identifier observed for the active graphical session.
    #[serde(default)]
    pub virtual_terminal: Option<String>,
    /// Window manager/compositor identity proven by a native session source.
    #[serde(default)]
    pub window_manager: Option<String>,
    /// Window manager package/build version when a native source can prove it.
    #[serde(default)]
    pub window_manager_version: Option<String>,
    /// Rendering/session backend associated with the observed window manager.
    #[serde(default)]
    pub compositor_backend: Option<String>,
}

/// Kernel and boot metadata, isolated from host identity because the native
/// kernel-information source can fail independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KernelInfo {
    /// Kernel/runtime version supplied by the native platform.
    pub version: Option<String>,
    pub modules_count: Option<usize>,
    pub command_line: Option<String>,
    /// Display-ready build description supplied by the native adapter.
    ///
    /// This excludes `version` and never carries an unparsed provider record,
    /// path-specific syntax, or an operating-system-specific prefix.
    pub build: Option<String>,
    /// Kernel compiler description supplied by the native adapter, e.g.
    /// `"gcc 14.2.0"`. Absent when the platform exposes no compiler record —
    /// never parsed from the build string by a frontend.
    #[serde(default)]
    pub compiler: Option<String>,
}

/// CPUID version identity for the processor (leaf-0 vendor string plus the
/// leaf-1 `EAX` version fields).
///
/// The numeric fields are the raw CPUID fields, not their display
/// combination: combining them for presentation is a pure rule below, so one
/// authority owns the SDM arithmetic. Every field is absent together when the
/// platform cannot probe the identity (non-x86 host, synthetic fixture root);
/// partial presence only arises from legacy snapshots and renders per field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CpuIdentity {
    /// Native CPUID vendor string, e.g. `"GenuineIntel"` or `"AuthenticAMD"`.
    #[serde(default)]
    pub vendor_id: Option<String>,
    /// CPUID leaf-1 base family field (bits 8-11).
    #[serde(default)]
    pub family: Option<u8>,
    /// CPUID leaf-1 extended family field (bits 20-27).
    #[serde(default)]
    pub ext_family: Option<u8>,
    /// CPUID leaf-1 base model field (bits 4-7).
    #[serde(default)]
    pub model: Option<u8>,
    /// CPUID leaf-1 extended model field (bits 16-19).
    #[serde(default)]
    pub ext_model: Option<u8>,
    /// CPUID leaf-1 stepping field (bits 0-3).
    #[serde(default)]
    pub stepping: Option<u8>,
}

impl CpuIdentity {
    /// Assemble the identity from the raw leaf-1 version fields supplied by
    /// the native CPUID reader. The `EAX` bit layout stays in the platform
    /// adapter; only these typed fields cross into the domain.
    #[must_use]
    pub fn from_cpuid_parts(
        vendor_id: Option<String>,
        family: u8,
        ext_family: u8,
        model: u8,
        ext_model: u8,
        stepping: u8,
    ) -> Self {
        Self {
            vendor_id,
            family: Some(family),
            ext_family: Some(ext_family),
            model: Some(model),
            ext_model: Some(ext_model),
            stepping: Some(stepping),
        }
    }

    /// Display family per the SDM combination rule: the base family, plus the
    /// extended family when the base field is exhausted (`0xF`). `None` for an
    /// exhausted base field whose extension the source did not report — a bare
    /// `15` would misidentify every modern processor.
    #[must_use]
    pub fn display_family(&self) -> Option<u32> {
        match self.family {
            None => None,
            Some(0xF) => self
                .ext_family
                .map(|ext| u32::from(0xF_u8) + u32::from(ext)),
            Some(family) => Some(u32::from(family)),
        }
    }

    /// Display model per the SDM combination rule: the base model, plus the
    /// extended model shifted into place when the *base* family field is `6`
    /// or `0xF`. Extended-model absence on those families leaves the model
    /// unavailable rather than presenting a truncated identity.
    #[must_use]
    pub fn display_model(&self) -> Option<u32> {
        let model = u32::from(self.model?);
        match self.family {
            Some(0x6) | Some(0xF) => self.ext_model.map(|ext| (u32::from(ext) << 4) + model),
            _ => Some(model),
        }
    }

    /// Compact one-line identity code, e.g. `"6 / 183 / 2"` (display family,
    /// display model, stepping). `None` while the family/model pair cannot be
    /// derived; stepping is appended only when reported.
    #[must_use]
    pub fn code(&self) -> Option<String> {
        let family = self.display_family()?;
        let model = self.display_model()?;
        match self.stepping {
            Some(stepping) => Some(format!("{family} / {model} / {stepping}")),
            None => Some(format!("{family} / {model}")),
        }
    }

    /// Marketing codename for the probed identity (e.g. "Raptor Lake-S/HX
    /// (13th/14th gen)"), from the shared codename table. `None` for an
    /// uncovered pair or vendor — never a guess from the brand string.
    #[must_use]
    pub fn codename(&self) -> Option<&'static str> {
        let vendor =
            crate::core::cpu_codename::CpuVendor::from_vendor_id(self.vendor_id.as_deref()?)?;
        crate::core::cpu_codename::classify_cpu_codename(
            vendor,
            self.display_family()?,
            self.display_model()?,
        )
        .map(|(codename, _)| codename)
    }

    /// Process node for the probed identity (e.g. "Intel 7", "TSMC N4"),
    /// from the shared codename table. `None` alongside an absent codename.
    #[must_use]
    pub fn process_node(&self) -> Option<&'static str> {
        let vendor =
            crate::core::cpu_codename::CpuVendor::from_vendor_id(self.vendor_id.as_deref()?)?;
        crate::core::cpu_codename::classify_cpu_codename(
            vendor,
            self.display_family()?,
            self.display_model()?,
        )
        .map(|(_, node)| node)
    }

    /// Whether any identity field was observed. Absent everywhere is the
    /// honest "not probed" state on non-x86 hosts and fixture roots.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.vendor_id.is_some()
            || self.family.is_some()
            || self.ext_family.is_some()
            || self.model.is_some()
            || self.ext_model.is_some()
            || self.stepping.is_some()
    }
}

/// Static compute topology. Live frequency, temperature and power readings do
/// not belong here and are supplied by telemetry/sensor capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComputeTopology {
    /// Native processor description. This is descriptive text, not a stable
    /// hardware identity.
    pub cpu_brand: Option<String>,
    /// Number of logical processors exposed to this process.
    pub logical_cpu_count: Option<usize>,
    pub socket_count: Option<u16>,
    /// Installed/visible physical memory in MiB.
    pub total_memory_mb: Option<u64>,
    pub core_breakdown: CoreBreakdown,
    pub cpu_types: Vec<CpuType>,
    pub base_frequency_mhz: Option<u64>,
    /// Instruction-set capabilities mapped from the native CPU source. An
    /// unreported feature is absent from this list; providers never guess.
    #[serde(default)]
    pub instruction_features: Vec<CpuInstructionFeature>,
    /// CPUID version identity (vendor string plus raw family/model/stepping
    /// fields). Absent together when the platform cannot probe it; never
    /// inferred from the marketing brand string.
    #[serde(default)]
    pub cpu_identity: CpuIdentity,
}

/// Optional firmware and machine identity facts.
///
/// An empty fragment is valid on machines without DMI/SMBIOS; the native
/// source outcome distinguishes that from a read or permission failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FirmwareInfo {
    pub virtualization: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    /// Native firmware vendor. The legacy serialized name remains `bios_vendor`
    /// so existing snapshots continue to decode on non-BIOS platforms.
    #[serde(default, rename = "bios_vendor", alias = "firmware_vendor")]
    pub firmware_vendor: Option<String>,
    /// Native firmware version (BIOS, UEFI, boot ROM, or platform equivalent).
    #[serde(default, rename = "bios_version", alias = "firmware_version")]
    pub firmware_version: Option<String>,
    /// Motherboard/baseboard vendor when the native source exposes one.
    #[serde(default)]
    pub motherboard_vendor: Option<String>,
    /// Motherboard/baseboard model when the native source exposes one.
    #[serde(default)]
    pub motherboard_model: Option<String>,
    /// Motherboard/baseboard revision (DMI `board_version`) when the native
    /// source exposes one. Absence is honest — never a guessed "rev 1".
    #[serde(default)]
    pub motherboard_version: Option<String>,
    /// Platform/chipset model (e.g. `"Z690 Chipset"`) when the native source
    /// can prove it from its own PCI/DMI tables. Absence is honest — never a
    /// guess from the CPU brand or vendor name.
    #[serde(default)]
    pub chipset: Option<String>,
    /// Firmware release date (e.g. DMI `bios_date`) when the native source
    /// exposes one. Absence is honest — never a fabricated date.
    #[serde(default)]
    pub firmware_release_date: Option<String>,
    /// Secure Boot state when the native source can prove it. `None` is
    /// unknown (unreadable efivars / no safe API), never a guessed value.
    #[serde(default)]
    pub secure_boot: Option<bool>,
}

/// Map a DMI `sys_vendor` / SMBIOS Type 1 manufacturer (or
/// `/sys/hypervisor/type`) string to a short, stable hypervisor label.
/// Returns `None` for unknown / bare-metal vendors so callers can fall through
/// to another source rather than mislabel a real host. This is the single
/// shared classifier for every platform adapter (ADR-020); do not copy it.
#[must_use]
pub fn classify_hypervisor_vendor(vendor: &str) -> Option<String> {
    let v = vendor.to_lowercase();
    if v.contains("qemu") || v.contains("kvm") {
        Some("KVM".to_string())
    } else if v.contains("vmware") {
        Some("VMware".to_string())
    } else if v.contains("virtualbox") || v.contains("innotek") {
        Some("VirtualBox".to_string())
    } else if v.contains("microsoft") || v.contains("hyper-v") || v.contains("hyperv") {
        Some("Hyper-V".to_string())
    } else if v.contains("xen") {
        Some("Xen".to_string())
    } else {
        None
    }
}

/// One physically connected display described by a native display provider.
///
/// Linux can obtain this without a compositor or privileged helper by reading
/// the DRM connector's EDID. Other platforms may leave the collection empty
/// until their native display provider supplies equivalent facts. Optional
/// fields remain independent: a connected connector with unreadable EDID is
/// still useful identity, but must not acquire fabricated mode information.
///
/// This is static hardware inventory. Current compositor mode, HDR enablement,
/// and VRR policy belong to [`DisplayRuntimeInfo`] and must not be folded into
/// this read model or rendered as hardware identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DisplayInfo {
    /// Stable native connector/target name, e.g. `DP-1` or `HDMI-A-1`.
    pub connector: String,
    /// Three-letter EDID manufacturer code when the block is valid.
    #[serde(default)]
    pub manufacturer: Option<String>,
    /// EDID display name, if the monitor advertises one.
    #[serde(default)]
    pub model: Option<String>,
    /// EDID serial descriptor or numeric serial, when present.
    #[serde(default)]
    pub serial: Option<String>,
    /// Physical panel dimensions in millimetres.
    #[serde(default)]
    pub width_mm: Option<u32>,
    #[serde(default)]
    pub height_mm: Option<u32>,
    /// Preferred/detailed timing dimensions in pixels.
    #[serde(default)]
    pub width_px: Option<u32>,
    #[serde(default)]
    pub height_px: Option<u32>,
    /// Preferred/detailed timing refresh rate in Hz.
    #[serde(default)]
    pub refresh_hz: Option<f32>,
    /// Whether the panel advertises HDR Static Metadata in EDID.
    #[serde(default)]
    pub hdr_supported: Option<bool>,
}

/// Dynamic display state obtained from a compositor/session provider.
///
/// This model is intentionally separate from [`DisplayInfo`]. It is suitable
/// for a future display telemetry capability or a performance projection, but
/// it does not belong in static hardware inventory or the hardware details
/// page. A missing compositor leaves these fields unavailable; it never changes
/// the static monitor identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DisplayRuntimeInfo {
    /// Connector/output identity used to join this runtime sample to
    /// [`DisplayInfo`].
    pub connector: String,
    /// Current compositor-selected mode dimensions.
    #[serde(default)]
    pub current_width_px: Option<u32>,
    #[serde(default)]
    pub current_height_px: Option<u32>,
    /// Current fixed refresh rate. `None` is correct for VRR or an unavailable
    /// compositor mode source.
    #[serde(default)]
    pub current_refresh_hz: Option<f32>,
    /// HDR capability exposed by the active compositor/session, distinct from
    /// the panel capability in static EDID inventory.
    #[serde(default)]
    pub compositor_hdr_supported: Option<bool>,
    /// Whether HDR is currently enabled by the compositor.
    #[serde(default)]
    pub hdr_enabled: Option<bool>,
    /// Whether the compositor currently exposes variable refresh rate.
    #[serde(default)]
    pub compositor_vrr_supported: Option<bool>,
    /// Compositor policy for using VRR, when exposed.
    #[serde(default)]
    pub vrr_policy: Option<String>,
    /// Whether the compositor currently exposes the output as enabled.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Provider identity for current mode facts.
    #[serde(default)]
    pub current_mode_source: Option<String>,
    /// Provider identity for dynamic HDR facts.
    #[serde(default)]
    pub hdr_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareInfo {
    #[serde(default)]
    pub os_name: Option<String>,
    #[serde(default)]
    pub os_version: Option<String>,
    #[serde(default)]
    pub kernel_version: Option<String>,
    /// Number of loaded kernel modules/components when exposed by the platform.
    #[serde(default)]
    pub kernel_modules_count: Option<usize>,
    /// Native kernel/boot command line, or `None` when unavailable.
    #[serde(default)]
    pub kernel_cmdline: Option<String>,
    /// Display-ready build description, already separated from the version by
    /// the native adapter. Frontends must not parse provider-specific records.
    #[serde(default)]
    pub kernel_build: Option<String>,
    /// Kernel compiler description from the kernel fragment, e.g.
    /// `"gcc 14.2.0"`. `None` when the platform exposes no compiler record.
    #[serde(default)]
    pub kernel_compiler: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    /// Login shell reported by the active user session.
    #[serde(default)]
    pub shell: Option<String>,
    /// Terminal program/type reported by the active session.
    #[serde(default)]
    pub terminal: Option<String>,
    /// Terminal program version when available.
    #[serde(default)]
    pub terminal_version: Option<String>,
    /// Active locale reported by the active session.
    #[serde(default)]
    pub locale: Option<String>,
    /// PID 1 implementation name when the native adapter can read it.
    #[serde(default)]
    pub init_system: Option<String>,
    /// Package manager name reported by the native host provider.
    #[serde(default)]
    pub package_manager: Option<String>,
    /// Package manager version reported by the native host provider.
    #[serde(default)]
    pub package_manager_version: Option<String>,
    /// Number of installed packages reported by the native package database.
    #[serde(default)]
    pub package_count: Option<u64>,
    /// Desktop environment name reported by the native session provider.
    #[serde(default)]
    pub desktop_environment: Option<String>,
    /// Desktop environment version reported by the native session provider.
    #[serde(default)]
    pub desktop_environment_version: Option<String>,
    /// Windowing system reported by the native session (`wayland`, `x11`, …).
    #[serde(default)]
    pub windowing_system: Option<String>,
    /// Virtual terminal identifier reported by the native session.
    #[serde(default)]
    pub virtual_terminal: Option<String>,
    /// Window manager/compositor identity proven by a native session source.
    #[serde(default)]
    pub window_manager: Option<String>,
    /// Window manager package/build version when available.
    #[serde(default)]
    pub window_manager_version: Option<String>,
    /// Rendering/session backend associated with the observed window manager.
    #[serde(default)]
    pub compositor_backend: Option<String>,
    #[serde(default)]
    pub cpu_brand: Option<String>,
    #[serde(default)]
    pub cpu_cores: Option<usize>,
    /// Number of CPU sockets (physical packages) when supplied by the native
    /// topology provider. Missing topology nodes remain `None`.
    #[serde(default)]
    pub sockets: Option<u16>,
    #[serde(default)]
    pub total_memory_mb: Option<u64>,
    /// Heterogeneous core breakdown (P / E / LP-E). An unclassified
    /// homogeneous processor leaves all classes zero rather than pretending
    /// that every logical processor is a P-core.
    pub core_breakdown: CoreBreakdown,
    /// Per-logical-CPU type (length == cpu_cores). Drives the grouped core grid.
    #[serde(default)]
    pub cpu_types: Vec<CpuType>,
    /// Advertised static base clock (MHz), or `None` when unavailable.
    #[serde(default)]
    pub base_freq_mhz: Option<u64>,
    /// Instruction-set capabilities reported by the native CPU source.
    /// Missing when the adapter cannot read a native feature source.
    #[serde(default)]
    pub instruction_features: Vec<CpuInstructionFeature>,
    /// CPUID version identity from the topology source (vendor, family,
    /// model, stepping fields). Absent when the platform cannot probe it.
    #[serde(default)]
    pub cpu_identity: CpuIdentity,
    /// Hypervisor label (e.g. "KVM", "Hyper-V", "VMware", "VirtualBox", "Xen",
    /// or the raw native provider label). `None` on bare metal.
    #[serde(default)]
    pub virt: Option<String>,
    /// Firmware/system product name. `None` when unavailable.
    #[serde(default)]
    pub product_name: Option<String>,
    /// Firmware/system product version. `None` when unavailable.
    #[serde(default)]
    pub product_version: Option<String>,
    /// Firmware vendor. `None` when unavailable. Serialization preserves the
    /// legacy `bios_vendor` key for snapshot compatibility.
    #[serde(default, rename = "bios_vendor", alias = "firmware_vendor")]
    pub firmware_vendor: Option<String>,
    /// Firmware version. `None` when unavailable.
    #[serde(default, rename = "bios_version", alias = "firmware_version")]
    pub firmware_version: Option<String>,
    /// Neutral host architecture (`x86_64`, `aarch64`, ...) — a compile-time
    /// fact of this binary, never a provider guess.
    #[serde(default)]
    pub architecture: Option<String>,
    /// Motherboard/baseboard vendor. `None` when unavailable.
    #[serde(default)]
    pub motherboard_vendor: Option<String>,
    /// Motherboard/baseboard model. `None` when unavailable.
    #[serde(default)]
    pub motherboard_model: Option<String>,
    /// Firmware release date. `None` when unavailable.
    #[serde(default)]
    pub firmware_release_date: Option<String>,
    /// Motherboard/baseboard revision from the firmware fragment.
    #[serde(default)]
    pub motherboard_version: Option<String>,
    /// Platform/chipset model from the firmware fragment. `None` when the
    /// platform exposes no provable chipset identity source.
    #[serde(default)]
    pub chipset: Option<String>,
    /// Secure Boot state. `None` when the platform exposes no safe source.
    #[serde(default)]
    pub secure_boot: Option<bool>,
    /// Static facts for physically connected displays discovered by the native
    /// display source. Current compositor state is not part of this inventory.
    /// An empty list is an honest no-display/unsupported result, not a hidden
    /// monitor with zero dimensions.
    #[serde(default)]
    pub displays: Vec<DisplayInfo>,
}

impl HardwareInfo {
    /// Neutral host architecture resolved at compile time. Covers the product
    /// targets (x86_64/aarch64) plus the common tier-1/tier-2 architectures;
    /// anything else stays the honest `"unknown"` rather than a guessed name.
    const HOST_ARCH: &str = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else if cfg!(target_arch = "riscv64") {
        "riscv64"
    } else if cfg!(target_arch = "powerpc64") {
        "powerpc64"
    } else if cfg!(target_arch = "s390x") {
        "s390x"
    } else {
        "unknown"
    };

    /// Assemble the public compatibility read model from independently
    /// observed fragments. Source health is intentionally not inferred here;
    /// it travels beside this value in `CompositeSourceSnapshot`.
    #[must_use]
    pub fn from_fragments(
        host: HostIdentity,
        kernel: KernelInfo,
        topology: ComputeTopology,
        firmware: FirmwareInfo,
    ) -> Self {
        Self::from_fragments_with_displays(host, kernel, topology, firmware, Vec::new())
    }

    /// Assemble the public model with an optional static display inventory.
    /// The four original fragments remain the compatibility entry point; a
    /// platform that owns a safe display source opts into this additive path.
    #[must_use]
    pub fn from_fragments_with_displays(
        host: HostIdentity,
        kernel: KernelInfo,
        topology: ComputeTopology,
        firmware: FirmwareInfo,
        displays: Vec<DisplayInfo>,
    ) -> Self {
        Self {
            os_name: host.os_name,
            os_version: host.os_version,
            kernel_version: kernel.version,
            kernel_modules_count: kernel.modules_count,
            kernel_cmdline: kernel.command_line,
            kernel_build: kernel.build,
            kernel_compiler: kernel.compiler,
            hostname: host.hostname,
            shell: host.shell,
            terminal: host.terminal,
            terminal_version: host.terminal_version,
            locale: host.locale,
            init_system: host.init_system,
            package_manager: host.package_manager,
            package_manager_version: host.package_manager_version,
            package_count: host.package_count,
            desktop_environment: host.desktop_environment,
            desktop_environment_version: host.desktop_environment_version,
            windowing_system: host.windowing_system,
            virtual_terminal: host.virtual_terminal,
            window_manager: host.window_manager,
            window_manager_version: host.window_manager_version,
            compositor_backend: host.compositor_backend,
            cpu_brand: topology.cpu_brand,
            cpu_cores: topology.logical_cpu_count,
            sockets: topology.socket_count,
            total_memory_mb: topology.total_memory_mb,
            core_breakdown: topology.core_breakdown,
            cpu_types: topology.cpu_types,
            base_freq_mhz: topology.base_frequency_mhz,
            instruction_features: topology.instruction_features,
            cpu_identity: topology.cpu_identity,
            virt: firmware.virtualization,
            product_name: firmware.product_name,
            product_version: firmware.product_version,
            firmware_vendor: firmware.firmware_vendor,
            firmware_version: firmware.firmware_version,
            architecture: Some(Self::HOST_ARCH.to_string()),
            motherboard_vendor: firmware.motherboard_vendor,
            motherboard_model: firmware.motherboard_model,
            motherboard_version: firmware.motherboard_version,
            chipset: firmware.chipset,
            firmware_release_date: firmware.firmware_release_date,
            secure_boot: firmware.secure_boot,
            displays,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_hardware_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/core_core_hardware_hypervisor_classify_tests.rs"]
mod hypervisor_classify_tests;
