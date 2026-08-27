//! Pure section builders for the System page (design: hero + static tiles +
//! sectioned spec cards). Everything here is render-neutral and unit-tested:
//! each builder returns the typed [`SystemSection`] the card renderer paints,
//! with the SAME honest-row conventions the page always had — a fact that
//! exists renders (dash only for a live gap), a fact that does not exist on
//! this host omits its row, and a section with no facts at all is omitted.

mod npu;

use taskmanager_ui_contract::IconId;

use crate::core::hardware::{DisplayInfo, HardwareInfo};
use crate::core::metrics::SystemSnapshot;

use super::{
    fmt_cache_kb, fmt_clock_ghz, fmt_observed_clock_ghz, joined_optional_text, kernel_display,
    optional_text, truncate_cmdline,
};
use crate::i18n;

/// One horizontal progress meter inside a section card (memory in use,
/// battery charge). `pct` is 0..=100; the renderer clamps.
pub(crate) struct SystemMeter {
    pub(crate) label: String,
    /// `None` keeps the track unfilled without turning an unavailable
    /// percentage into a visible 0% bar.
    pub(crate) pct: Option<f32>,
    pub(crate) note: String,
}

/// One section card's data: icon + localized title + spec rows + optional
/// feature chips (instruction set) and meters.
pub(crate) struct SystemSection {
    pub(crate) icon: IconId,
    pub(crate) title_key: &'static str,
    pub(crate) rows: Vec<(String, String)>,
    pub(crate) chips: Vec<String>,
    pub(crate) meters: Vec<SystemMeter>,
}

impl SystemSection {
    fn new(icon: IconId, title_key: &'static str) -> Self {
        Self {
            icon,
            title_key,
            rows: Vec::new(),
            chips: Vec::new(),
            meters: Vec::new(),
        }
    }

    /// A section with no rows, chips, or meters is not rendered at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.chips.is_empty() && self.meters.is_empty()
    }
}

/// Host identity + OS + kernel + firmware + desktop session. The newly surfaced
/// adapter facts (product version, package manager + version, desktop
/// environment, windowing system) are conditional rows — present only when the
/// native adapter actually reported them.
pub(super) fn device_section(hw: &HardwareInfo) -> SystemSection {
    let mut s = SystemSection::new(IconId::System, "system.section.device");
    s.rows.push((
        i18n::t("system.os").to_string(),
        joined_optional_text(hw.os_name.as_deref(), hw.os_version.as_deref()),
    ));
    // The native adapter has already separated version from its optional
    // build description; this frontend only composes display-ready facts.
    s.rows.push((
        i18n::t("system.kernel").to_string(),
        kernel_display(hw.kernel_version.as_deref(), hw.kernel_build.as_deref()),
    ));
    // Kernel boot facts stay directly beneath the Kernel row (modules count,
    // then boot args), hidden when absent — the Windows/macOS providers report
    // no module count, so a permanent dash would be a platform leak.
    if let Some(count) = hw.kernel_modules_count {
        s.rows.push((
            i18n::t("system.kernel_modules").to_string(),
            count.to_string(),
        ));
    }
    if let Some(args) = hw.kernel_cmdline.as_deref() {
        s.rows.push((
            i18n::t("system.boot_args").to_string(),
            truncate_cmdline(args),
        ));
    }
    s.rows.push((
        i18n::t("system.hostname").to_string(),
        optional_text(hw.hostname.as_deref()),
    ));
    s.rows.push((
        i18n::t("system.model").to_string(),
        hw.product_name
            .clone()
            .unwrap_or_else(crate::gpui_app::formatting::missing_value),
    ));
    if let Some(version) = hw.product_version.as_deref() {
        let version = version.trim();
        if !version.is_empty() {
            s.rows.push((
                i18n::t("system.product_version").to_string(),
                version.to_string(),
            ));
        }
    }
    s.rows.push((
        i18n::t("system.firmware").to_string(),
        joined_optional_text(
            hw.firmware_vendor.as_deref(),
            hw.firmware_version.as_deref(),
        ),
    ));
    // Desktop session facts (conditional — servers and Windows report none).
    if let Some(package_manager) = hw.package_manager.as_deref() {
        let package_manager = package_manager.trim();
        if !package_manager.is_empty() {
            let value = match hw.package_manager_version.as_deref().map(str::trim) {
                Some(version) if !version.is_empty() => format!("{package_manager} {version}"),
                _ => package_manager.to_string(),
            };
            s.rows
                .push((i18n::t("system.package_manager").to_string(), value));
        }
    }
    if let Some(count) = hw.package_count {
        s.rows.push((
            i18n::t("system.package_count").to_string(),
            count.to_string(),
        ));
    }
    let desktop = joined_optional_text(
        hw.desktop_environment.as_deref(),
        hw.desktop_environment_version.as_deref(),
    );
    if desktop != crate::gpui_app::formatting::missing_value() {
        s.rows
            .push((i18n::t("system.desktop_environment").to_string(), desktop));
    }
    if let Some(windowing) = hw.windowing_system.as_deref() {
        let windowing = windowing.trim();
        if !windowing.is_empty() {
            s.rows.push((
                i18n::t("system.windowing_system").to_string(),
                windowing.to_string(),
            ));
        }
    }
    let window_manager = joined_optional_text(
        hw.window_manager.as_deref(),
        hw.window_manager_version.as_deref(),
    );
    if window_manager != crate::gpui_app::formatting::missing_value() {
        s.rows.push((
            i18n::t("system.field.window_manager").to_string(),
            window_manager,
        ));
    }
    if let Some(backend) = hw.compositor_backend.as_deref() {
        let backend = backend.trim();
        if !backend.is_empty() {
            s.rows.push((
                i18n::t("system.field.compositor_backend").to_string(),
                backend.to_string(),
            ));
        }
    }
    for (key, value) in [
        ("system.field.shell", hw.shell.as_deref()),
        ("system.field.terminal", hw.terminal.as_deref()),
        (
            "system.field.terminal_version",
            hw.terminal_version.as_deref(),
        ),
        ("system.field.locale", hw.locale.as_deref()),
        ("system.field.init_system", hw.init_system.as_deref()),
    ] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            s.rows.push((i18n::t(key).to_string(), value.to_string()));
        }
    }
    for display in &hw.displays {
        s.rows.push((
            i18n::t("system.display").to_string(),
            display_summary(display),
        ));
    }
    s.rows.push((
        i18n::t("common.virtualization").to_string(),
        hw.virt
            .clone()
            .unwrap_or_else(|| i18n::t("common.none").to_string()),
    ));
    s
}

fn display_summary(display: &DisplayInfo) -> String {
    let identity = [display.manufacturer.as_deref(), display.model.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut parts = vec![display.connector.clone()];
    if !identity.is_empty() {
        parts.push(identity);
    }
    if let (Some(width), Some(height)) = (display.width_px, display.height_px) {
        parts.push(format!("{width}×{height}"));
    }
    if let Some(refresh) = display
        .refresh_hz
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        parts.push(format!("{refresh:.1} Hz"));
    }
    if let Some(hdr) = display_hdr_capability(display) {
        parts.push(hdr);
    }
    if let (Some(width), Some(height)) = (display.width_mm, display.height_mm) {
        parts.push(format!("{width}×{height} mm"));
    }
    if let Some(serial) = display.serial.as_deref().filter(|value| !value.is_empty()) {
        parts.push(format!("S/N {serial}"));
    }
    parts.join(" · ")
}

fn display_hdr_capability(display: &DisplayInfo) -> Option<String> {
    let state = match display.hdr_supported {
        Some(true) => i18n::t("system.hdr_supported"),
        Some(false) => i18n::t("system.hdr_unsupported"),
        None => return None,
    };
    Some(format!("{} {state}", i18n::t("system.hdr")))
}

/// CPU topology + clocks + caches; the instruction set renders as chips.
pub(super) fn cpu_section(hw: &HardwareInfo, snap: &SystemSnapshot) -> SystemSection {
    let mut s = SystemSection::new(IconId::Cpu, "system.section.cpu");
    s.rows.push((
        i18n::t("common.cpu").to_string(),
        optional_text(hw.cpu_brand.as_deref()),
    ));
    s.rows.push((
        i18n::t("common.cores").to_string(),
        format!(
            "{} {} / {} {}",
            snap.cpu
                .physical_cores
                .map_or_else(crate::gpui_app::formatting::missing_value, |cores| cores
                    .to_string()),
            i18n::t("common.physical"),
            snap.cpu
                .logical_cores
                .or(hw.cpu_cores)
                .map_or_else(crate::gpui_app::formatting::missing_value, |cores| cores
                    .to_string()),
            i18n::t("common.logical"),
        ),
    ));
    // Heterogeneous topology: one aligned row per core class (mirrors the CPU
    // page's details panel) so a big.LITTLE part scans without truncation.
    s.rows
        .extend(crate::gpui_app::cpu_view::heterogeneous_core_rows(
            &hw.core_breakdown,
        ));
    // Sockets label + honest-dash fold come from the CPU details panel's
    // shared builder (ADR-020 single source).
    s.rows
        .push(crate::gpui_app::cpu_view::sockets_row(hw.sockets));
    s.rows.push((
        i18n::t("system.base_clock").to_string(),
        fmt_clock_ghz(hw.base_freq_mhz),
    ));
    s.rows.push((
        i18n::t("system.max_clock").to_string(),
        fmt_observed_clock_ghz(snap.cpu.current_max_frequency_mhz().filter(|mhz| *mhz > 0)),
    ));
    s.rows.push((
        i18n::t("common.l1_cache").to_string(),
        fmt_cache_kb(snap.cpu.l1_cache_kb),
    ));
    s.rows.push((
        i18n::t("common.l2_cache").to_string(),
        fmt_cache_kb(snap.cpu.l2_cache_kb),
    ));
    s.rows.push((
        i18n::t("common.l3_cache").to_string(),
        fmt_cache_kb(snap.cpu.l3_cache_kb),
    ));
    // Instruction-set features become chips (compact, glanceable) instead of
    // one long joined string. Absent feature list → no chips, no row.
    s.chips = hw
        .instruction_features
        .iter()
        .map(|feature| feature.label().to_string())
        .collect();
    s
}

/// Static memory capacity + module facts.
///
/// The System page is an inventory surface. Live used/available memory belongs
/// to the Performance page; keeping it out of this section prevents the
/// hardware card from changing every telemetry tick.
pub(super) fn memory_section(hw: &HardwareInfo, snap: &SystemSnapshot) -> SystemSection {
    use crate::gpui_app::formatting;
    let mut s = SystemSection::new(IconId::Memory, "system.section.memory");
    let m = &snap.memory;
    let installed = hw
        .total_memory_mb
        .and_then(|mb| mb.checked_mul(1024 * 1024))
        .map(formatting::format_mib_whole)
        .unwrap_or_else(formatting::missing_value);
    s.rows
        .push((i18n::t("common.memory").to_string(), installed));
    // Swap capacity is a system configuration fact, not a live usage readout.
    if let Some(total) = m.current_swap_total_bytes()
        && total > 0
    {
        s.rows.push((
            i18n::t("mem.swap").to_string(),
            formatting::format_gib(total),
        ));
    }
    // RAM speed (from smbios/hwmon). Only when a non-zero speed was detected.
    if let Some(mhz) = m.current_speed_mhz()
        && mhz > 0
    {
        s.rows.push((
            i18n::t("system.ram_speed").to_string(),
            format!("{mhz} MT/s"),
        ));
    }
    if let Some(module_type) = m.current_module_type() {
        s.rows.push((
            i18n::t("system.memory_type").to_string(),
            module_type.to_owned(),
        ));
    }
    if let Some(manufacturer) = m.current_module_manufacturer() {
        s.rows.push((
            i18n::t("system.memory_manufacturer").to_string(),
            manufacturer.to_owned(),
        ));
    }
    if let Some(form_factor) = m.current_module_form_factor() {
        s.rows.push((
            i18n::t("system.memory_form_factor").to_string(),
            form_factor.to_owned(),
        ));
    }
    if let (Some(used), Some(total)) = (m.current_slots_used(), m.current_slots_total()) {
        s.rows.push((
            i18n::t("system.memory_slots").to_string(),
            format!("{used} / {total} {}", i18n::t("common.used")),
        ));
    }
    s
}

/// GPU identity/capacity parameters and the complete discovered NPU read model.
///
/// GPU utilization and temperature deliberately stay on the Performance page.
/// NPU has no separate GPUI performance page, so its typed aggregate/engine
/// utilization and memory facts remain adjacent to its discovered identity.
pub(super) fn graphics_section(
    snap: &SystemSnapshot,
    npu_inventory: Option<&taskmanager_application::NpuInventorySnapshot>,
) -> SystemSection {
    let mut s = SystemSection::new(IconId::Gpu, "system.section.graphics");
    let gpu_count = snap.gpu.len();
    for (i, g) in snap.gpu.iter().enumerate() {
        let label = if gpu_count > 1 {
            format!("{} {i}", i18n::t("common.gpu"))
        } else {
            i18n::t("common.gpu").to_string()
        };
        let mut value = g
            .marketing_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| optional_text(Some(g.brand.as_str())));
        if let Some(brand) = g
            .marketing_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .and_then(|marketing| {
                (!g.brand.trim().is_empty() && g.brand.trim() != marketing.trim())
                    .then_some(g.brand.trim())
            })
        {
            value.push_str(" · ");
            value.push_str(brand);
        }
        s.rows.push((label, value));
        if let Some(memory) = g
            .current_dedicated_vram_total_bytes()
            .or_else(|| g.current_memory_total_bytes())
        {
            let memory_label = if gpu_count > 1 {
                format!("{} {}", i18n::t("system.graphics_memory"), i + 1)
            } else {
                i18n::t("system.graphics_memory").to_string()
            };
            s.rows.push((
                memory_label,
                crate::gpui_app::formatting::bytes_to_human(memory),
            ));
        }
        if let Some(slot) = g.pci_slot.as_deref().filter(|slot| !slot.trim().is_empty()) {
            s.rows.push((
                i18n::t("system.graphics_slot").to_string(),
                slot.to_string(),
            ));
        }
    }
    s.rows.extend(npu::inventory_rows(npu_inventory));
    s
}

/// Static storage identity/capacity parameters. Filesystem free space and I/O
/// rates remain exclusively on the Performance storage page.
pub(super) fn storage_section(snap: &SystemSnapshot) -> SystemSection {
    let mut s = SystemSection::new(IconId::Disk, "system.section.storage");
    let disk_count = snap.disks.len();
    for (index, disk) in snap.disks.iter().enumerate() {
        let label = if disk_count > 1 {
            format!("{} {}", i18n::t("common.disk"), index + 1)
        } else {
            i18n::t("common.disk").to_string()
        };
        let identity = if !disk.model.trim().is_empty() {
            disk.model.trim().to_string()
        } else if !disk.name.trim().is_empty() {
            disk.name.trim().to_string()
        } else {
            crate::gpui_app::formatting::missing_value()
        };
        let mut details = vec![identity];
        if !disk.disk_type.trim().is_empty() {
            details.push(disk.disk_type.trim().to_string());
        }
        if let Some(capacity) = disk.current_capacity_bytes() {
            details.push(crate::gpui_app::formatting::bytes_to_human(capacity));
        }
        s.rows.push((label, details.join(" · ")));
        if let Some(revision) = disk
            .revision
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            s.rows
                .push((i18n::t("disk.revision").to_string(), revision.to_string()));
        }
        if let Some(serial) = disk
            .serial
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            s.rows
                .push((i18n::t("disk.serial").to_string(), serial.to_string()));
        }
    }
    s
}

/// Ordered section list for the page; empty sections drop out here so the
/// renderer never paints a header without facts.
pub(super) fn build_sections(
    hw: &HardwareInfo,
    snap: &SystemSnapshot,
    npu_inventory: Option<&taskmanager_application::NpuInventorySnapshot>,
) -> Vec<SystemSection> {
    let sections = vec![
        device_section(hw),
        cpu_section(hw, snap),
        memory_section(hw, snap),
        storage_section(snap),
        graphics_section(snap, npu_inventory),
    ];
    sections.into_iter().filter(|s| !s.is_empty()).collect()
}

/// One static hardware-parameter tile's data (the row beneath the hero card).
pub(crate) struct SystemTile {
    pub(crate) icon: IconId,
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) note: String,
}

/// Keep the parameter tile readable without discarding the full provider fact
/// shown in the processor section below. Legal trademark markers are noise at
/// this small size; removing them is presentation-only and never changes the
/// copied/exported hardware value.
fn compact_cpu_label(value: Option<&str>) -> String {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return crate::gpui_app::formatting::missing_value();
    };
    let compact = value
        .replace("(R)", "")
        .replace("(TM)", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    taskmanager_application::truncate_text(&compact, 28)
}

/// The four static parameter tiles: CPU, Memory, Storage, Graphics.
///
/// No live utilization, process count, uptime, temperature, or free-space
/// value is read here. The tile row is the hardware summary; live values stay
/// on the Performance pages.
pub(super) fn build_tiles(hw: &HardwareInfo, snap: &SystemSnapshot) -> Vec<SystemTile> {
    use crate::gpui_app::formatting;
    let cpu_value = compact_cpu_label(hw.cpu_brand.as_deref());
    let mut cpu_note = Vec::new();
    if let Some(physical) = snap.cpu.physical_cores {
        cpu_note.push(format!("{physical} {}", i18n::t("common.physical")));
    }
    if let Some(logical) = hw.cpu_cores.or(snap.cpu.logical_cores) {
        cpu_note.push(format!("{logical} {}", i18n::t("common.logical")));
    }
    if let Some(base) = hw.base_freq_mhz {
        cpu_note.push(formatting::format_ghz(base));
    }

    let memory_value = hw
        .total_memory_mb
        .and_then(|mb| mb.checked_mul(1024 * 1024))
        .map(formatting::format_mib_whole)
        .unwrap_or_else(formatting::missing_value);
    let memory_note = snap
        .memory
        .current_module_type()
        .map(str::to_owned)
        .or_else(|| {
            snap.memory
                .current_speed_mhz()
                .filter(|mhz| *mhz > 0)
                .map(|mhz| format!("{mhz} MT/s"))
        })
        .unwrap_or_else(formatting::missing_value);

    let storage_count = snap.disks.len();
    let storage_value = match storage_count {
        0 => formatting::missing_value(),
        1 => snap.disks[0]
            .model
            .trim()
            .is_empty()
            .then(|| snap.disks[0].name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(formatting::missing_value),
        count => format!("{count} {}", i18n::t("common.disk")),
    };
    let storage_total = snap
        .disks
        .iter()
        .filter_map(|disk| disk.current_capacity_bytes())
        .fold(0_u64, u64::saturating_add);
    let storage_note = if storage_total > 0 {
        formatting::bytes_to_human(storage_total)
    } else {
        formatting::missing_value()
    };

    let graphics_count = snap.gpu.len();
    let graphics_value = snap
        .gpu
        .first()
        .map(|gpu| {
            gpu.marketing_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| (!gpu.brand.trim().is_empty()).then_some(gpu.brand.as_str()))
                .map(str::to_string)
                .unwrap_or_else(formatting::missing_value)
        })
        .unwrap_or_else(formatting::missing_value);
    let graphics_note = if graphics_count == 0 {
        formatting::missing_value()
    } else {
        let memory = snap.gpu.first().and_then(|gpu| {
            gpu.current_dedicated_vram_total_bytes()
                .or_else(|| gpu.current_memory_total_bytes())
        });
        match memory {
            Some(memory) => format!(
                "{graphics_count} {} · {}",
                i18n::t("common.gpu"),
                formatting::bytes_to_human(memory)
            ),
            None => format!("{graphics_count} {}", i18n::t("common.gpu")),
        }
    };
    vec![
        SystemTile {
            icon: IconId::Cpu,
            title: i18n::t("common.cpu").to_string(),
            value: cpu_value,
            note: if cpu_note.is_empty() {
                formatting::missing_value()
            } else {
                cpu_note.join(" · ")
            },
        },
        SystemTile {
            icon: IconId::Memory,
            title: i18n::t("common.memory").to_string(),
            value: memory_value,
            note: memory_note,
        },
        SystemTile {
            icon: IconId::Disk,
            title: i18n::t("system.storage").to_string(),
            value: storage_value,
            note: storage_note,
        },
        SystemTile {
            icon: IconId::Gpu,
            title: i18n::t("common.gpu").to_string(),
            value: graphics_value,
            note: graphics_note,
        },
    ]
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_system_view_sections_tests.rs"]
mod tests;
