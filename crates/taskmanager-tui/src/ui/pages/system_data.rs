//! Pure typed projection for the TUI System page.
//!
//! The renderer consumes ordered sections and a bounded viewport; observation
//! availability and NPU engine/memory folds live here instead of paint code.

use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::{DisplayInfo, HardwareInfo};
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::npu::{NpuEngineKind, NpuInventorySnapshot};
use taskmanager_shell::presentation::{MISSING_VALUE, duration, missing_value, optional_bytes};

pub(super) struct SystemFact {
    pub(super) label: String,
    pub(super) value: String,
}

pub(super) struct SystemFactSection {
    pub(super) title: String,
    pub(super) facts: Vec<SystemFact>,
}

impl SystemFactSection {
    fn new(title_key: &'static str) -> Self {
        Self {
            title: t(title_key).to_owned(),
            facts: Vec::new(),
        }
    }

    fn push(&mut self, label: impl Into<String>, value: impl Into<String>) {
        self.facts.push(SystemFact {
            label: label.into(),
            value: value.into(),
        });
    }

    fn push_optional_text(&mut self, label_key: &'static str, value: Option<&str>) {
        self.push(
            t(label_key),
            value
                .filter(|text| !text.is_empty())
                .unwrap_or(MISSING_VALUE),
        );
    }
}

/// Ordered System facts. NPU inventory stays a fixed section: every device,
/// aggregate observation, reported engine and memory fact is materialized
/// once, with unavailable values rendered honestly as dashes.
pub(super) fn system_sections(
    hardware: Option<&HardwareInfo>,
    snapshot: Option<&SystemSnapshot>,
    npu_inventory: Option<&NpuInventorySnapshot>,
) -> Vec<SystemFactSection> {
    let window_manager = hardware.and_then(|item| {
        item.window_manager.as_deref().map(|name| {
            item.window_manager_version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map_or_else(|| name.to_owned(), |version| format!("{name} {version}"))
        })
    });

    let mut device = SystemFactSection::new("system.section.device");
    for (key, value) in [
        (
            "system.os",
            hardware.and_then(|item| item.os_name.as_deref()),
        ),
        (
            "common.operating_system",
            hardware.and_then(|item| item.os_version.as_deref()),
        ),
        (
            "system.kernel",
            hardware.and_then(|item| item.kernel_version.as_deref()),
        ),
        (
            "system.field.kernel_build",
            hardware.and_then(|item| item.kernel_build.as_deref()),
        ),
        (
            "system.field.kernel_compiler",
            hardware.and_then(|item| item.kernel_compiler.as_deref()),
        ),
        (
            "system.hostname",
            hardware.and_then(|item| item.hostname.as_deref()),
        ),
        (
            "system.field.package_manager",
            hardware.and_then(|item| item.package_manager.as_deref()),
        ),
        (
            "system.field.package_manager_version",
            hardware.and_then(|item| item.package_manager_version.as_deref()),
        ),
        (
            "system.field.desktop",
            hardware.and_then(|item| item.desktop_environment.as_deref()),
        ),
        (
            "system.field.desktop_version",
            hardware.and_then(|item| item.desktop_environment_version.as_deref()),
        ),
        (
            "system.field.windowing",
            hardware.and_then(|item| item.windowing_system.as_deref()),
        ),
        ("system.field.window_manager", window_manager.as_deref()),
        (
            "system.field.compositor_backend",
            hardware.and_then(|item| item.compositor_backend.as_deref()),
        ),
        (
            "system.field.virtual_terminal",
            hardware.and_then(|item| item.virtual_terminal.as_deref()),
        ),
        (
            "system.field.shell",
            hardware.and_then(|item| item.shell.as_deref()),
        ),
        (
            "system.field.terminal",
            hardware.and_then(|item| item.terminal.as_deref()),
        ),
        (
            "system.field.terminal_version",
            hardware.and_then(|item| item.terminal_version.as_deref()),
        ),
        (
            "system.field.locale",
            hardware.and_then(|item| item.locale.as_deref()),
        ),
        (
            "system.field.init_system",
            hardware.and_then(|item| item.init_system.as_deref()),
        ),
        (
            "system.model",
            hardware.and_then(|item| item.product_name.as_deref()),
        ),
        (
            "system.field.product_version",
            hardware.and_then(|item| item.product_version.as_deref()),
        ),
        (
            "system.firmware",
            hardware.and_then(|item| item.firmware_vendor.as_deref()),
        ),
        (
            "system.field.firmware_version",
            hardware.and_then(|item| item.firmware_version.as_deref()),
        ),
    ] {
        device.push_optional_text(key, value);
    }
    // Platform/chipset model belongs with the board/firmware identity; a host
    // whose adapter proved no chipset omits the row instead of dashing it.
    if let Some(chipset) = hardware
        .and_then(|item| item.chipset.as_deref())
        .map(str::trim)
        .filter(|chipset| !chipset.is_empty())
    {
        device.push(t("system.field.chipset"), chipset);
    }
    device.push(
        t("system.field.package_count"),
        hardware
            .and_then(|item| item.package_count)
            .map_or_else(missing_value, |count| count.to_string()),
    );
    device.push(
        t("common.uptime"),
        snapshot.map_or_else(missing_value, |item| duration(item.uptime_secs)),
    );
    device.push(
        t("common.processes"),
        snapshot.map_or_else(missing_value, |item| item.processes.to_string()),
    );
    device.push(
        t("common.threads"),
        snapshot.map_or_else(missing_value, |item| {
            item.threads
                .map_or_else(missing_value, |threads| threads.to_string())
        }),
    );

    let mut cpu = SystemFactSection::new("system.section.cpu");
    cpu.push_optional_text(
        "common.cpu",
        hardware.and_then(|item| item.cpu_brand.as_deref()),
    );
    if let Some(codename) = hardware.and_then(|item| item.cpu_identity.codename()) {
        cpu.push(t("system.cpu_codename"), codename);
    }
    if let Some(process) = hardware.and_then(|item| item.cpu_identity.process_node()) {
        cpu.push(t("system.cpu_process"), process);
    }
    if let Some(vendor) = hardware.and_then(|item| item.cpu_identity.vendor_id.clone()) {
        cpu.push(t("system.cpu_vendor"), vendor);
    }
    if let Some(code) = hardware.and_then(|item| item.cpu_identity.code()) {
        cpu.push(t("system.cpu_identity"), code);
    }
    cpu.push(
        t("common.logical_cores"),
        hardware
            .and_then(|item| item.cpu_cores)
            .map_or_else(missing_value, |cores| cores.to_string()),
    );
    cpu.push(
        t("common.sockets"),
        hardware
            .and_then(|item| item.sockets)
            .map_or_else(missing_value, |sockets| sockets.to_string()),
    );
    cpu.push(
        t("system.base_clock"),
        hardware
            .and_then(|item| item.base_freq_mhz)
            .map_or_else(missing_value, |mhz| format!("{mhz} MHz")),
    );
    cpu.push(
        t("cpu.multiplier"),
        hardware
            .and_then(|item| item.base_freq_mhz)
            .zip(snapshot.and_then(|snap| snap.cpu.current_frequency_mhz()))
            .map_or_else(missing_value, |(base, current)| {
                if base == 0 {
                    missing_value()
                } else {
                    format!("\u{00d7}{:.1}", current as f32 / base as f32)
                }
            }),
    );
    cpu.push(
        t("common.virtualization"),
        hardware
            .and_then(|item| item.virt.as_deref())
            .map_or_else(missing_value, str::to_owned),
    );
    if let Some(features) = hardware.map(|item| &item.instruction_features)
        && !features.is_empty()
    {
        cpu.push(
            t("system.field.instruction_features"),
            features
                .iter()
                .map(|feature| feature.label())
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }

    let mut memory = SystemFactSection::new("system.section.memory");
    memory.push(
        t("common.memory"),
        hardware
            .and_then(|item| item.total_memory_mb)
            .map_or_else(missing_value, |mb| format!("{mb} MiB")),
    );

    let mut graphics = SystemFactSection::new("system.section.graphics");
    if let Some(inventory) = npu_inventory.filter(|inventory| inventory.is_success()) {
        for device in &inventory.devices {
            let identity_label = format!("{} {}", t("npu.title"), device.device_id.as_str());
            graphics.push(
                identity_label,
                device
                    .brand
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| t("npu.device_title")),
            );
            graphics.push(
                t("common.driver"),
                device.driver.as_deref().unwrap_or(MISSING_VALUE),
            );
            graphics.push(
                t("common.utilization"),
                observed_percentage(device.utilization_pct.current_value().copied()),
            );
            for engine in &device.engines {
                graphics.push(
                    t(npu_engine_label_key(engine.kind)),
                    observed_percentage(engine.utilization_pct.current_value().copied()),
                );
            }
            graphics.push(
                t("npu.dedicated_memory"),
                optional_bytes(device.memory.dedicated_total_bytes.current_value().copied()),
            );
            graphics.push(
                t("npu.shared_memory"),
                optional_bytes(device.memory.shared_total_bytes.current_value().copied()),
            );
        }
    }
    if let Some(hardware) = hardware {
        for display in &hardware.displays {
            graphics.push(t("system.display"), display_summary(display));
        }
    }

    // Storage inventory (GPUI `storage_section` parity): one static
    // identity/capacity row per discovered disk. Field order mirrors the
    // parity checklist — name, capacity, available, type — and stays
    // minimal: free-space detail and I/O telemetry remain exclusively on
    // the Performance disk page. An unobserved byte figure is an honest
    // dash, never a fabricated zero.
    let mut storage = SystemFactSection::new("system.section.storage");
    if let Some(snapshot) = snapshot {
        let disk_count = snapshot.disks.len();
        for (index, disk) in snapshot.disks.iter().enumerate() {
            let label = if disk_count > 1 {
                format!("{} {}", t("common.disk"), index.saturating_add(1))
            } else {
                t("common.disk").to_owned()
            };
            let identity = [disk.model.trim(), disk.name.trim()]
                .into_iter()
                .find(|value| !value.is_empty())
                .unwrap_or(MISSING_VALUE)
                .to_owned();
            let mut details = vec![identity];
            details.push(optional_bytes(disk.current_capacity_bytes()));
            details.push(optional_bytes(disk.current_available_bytes()));
            let disk_type = disk.disk_type.trim();
            if !disk_type.is_empty() {
                details.push(disk_type.to_owned());
            }
            storage.push(label, details.join(" · "));
        }
    }

    let mut sections = vec![device, cpu, memory];
    if !graphics.facts.is_empty() {
        sections.push(graphics);
    }
    if !storage.facts.is_empty() {
        sections.push(storage);
    }
    sections
}

fn observed_percentage(value: Option<f32>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.1}%"))
}

const fn npu_engine_label_key(kind: NpuEngineKind) -> &'static str {
    match kind {
        NpuEngineKind::Compute => "npu.engine_compute",
        NpuEngineKind::Matrix => "npu.engine_matrix",
        NpuEngineKind::Vector => "npu.engine_vector",
        NpuEngineKind::Video => "npu.engine_video",
        NpuEngineKind::Copy => "npu.engine_copy",
        NpuEngineKind::Unknown => "npu.engine_unknown",
    }
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
        Some(true) => t("system.hdr_supported"),
        Some(false) => t("system.hdr_unsupported"),
        None => return None,
    };
    Some(format!("{} {state}", t("system.hdr")))
}
