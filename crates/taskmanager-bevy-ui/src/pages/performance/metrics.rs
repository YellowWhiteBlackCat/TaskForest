//! Pure Performance-page metric projections and formatting.

use super::*;
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_shell::presentation::device_status_i18n_key;
use taskmanager_shell::presentation::effective_smart_status;
use taskmanager_shell::presentation::has_smart_fields;
use taskmanager_shell::presentation::smart_section_visible;
use taskmanager_shell::presentation::trend::window;

pub(super) mod cpu;

/// Percent readout. There is no shared percent formatter in
/// `shell::presentation` (the TUI keeps its own in `ui/units.rs`), so this
/// page owns one with the same semantics: missing and non-finite observations
/// render the shared dash, never a fabricated `0.0%`.
pub(super) fn observed_percentage(value: Option<f32>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.1}%"))
}

/// Read the correlated six-domain projection first. The complete shell
/// snapshot is the honest fallback for demo/cold-start frames: it is already
/// the shell's committed render model, so Performance does not show dashes
/// merely because the newer partial telemetry stream has not warmed yet.
///
/// Every read goes through the application layer's staleness fold
/// ([`taskmanager_application::SystemTelemetryDomainState::usable`]):
/// current/partial domains read the live observation, stale/unavailable ones
/// keep their last known fact, and nothing here turns a missing fact into a
/// value.
pub(super) fn cpu_metrics(shell: &ShellApp) -> Option<&CpuMetrics> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            telemetry.cpu.usable(
                CpuTelemetryObservation::current_value,
                CpuTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| &snapshot.cpu)
        })
}

pub(super) fn memory_metrics(shell: &ShellApp) -> Option<&MemoryMetrics> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            telemetry.memory.usable(
                MemoryTelemetryObservation::current_value,
                MemoryTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| &snapshot.memory)
        })
}

pub(super) fn gpu_devices(shell: &ShellApp) -> Option<&[GpuMetrics]> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            telemetry.gpu.usable(
                GpuTelemetryObservation::current_value,
                GpuTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.gpu.as_slice())
        })
}

pub(super) fn network_devices(shell: &ShellApp) -> Option<&[NetworkMetrics]> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            telemetry.network.usable(
                NetworkTelemetryObservation::current_value,
                NetworkTelemetryObservation::last_known_value,
            )
        })
        .or_else(|| {
            shell
                .projection()
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.networks.as_slice())
        })
}

pub(super) fn disks(shell: &ShellApp) -> Option<&[DiskMetrics]> {
    shell
        .projection()
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.disks.as_slice())
}

pub(super) fn batteries(shell: &ShellApp) -> Option<&[BatteryInfo]> {
    shell
        .projection()
        .power_supplies
        .as_ref()
        .map(|ps| ps.batteries.as_slice())
}

pub(crate) fn summary_value(shell: &ShellApp, field: SummaryField) -> String {
    match field {
        SummaryField::Cpu => {
            observed_percentage(cpu_metrics(shell).and_then(|cpu| cpu.current_global_usage_pct()))
        }
        SummaryField::Cores => core_summary(shell),
        SummaryField::Memory => memory_summary(memory_metrics(shell)),
        // `swap_breakdown` only answers when a positive total is configured,
        // so `None` (no swap) is the dash, never "0 / 0".
        SummaryField::Swap => {
            memory_metrics(shell)
                .and_then(swap_breakdown)
                .map_or_else(missing_value, |swap| {
                    let pct = (swap.used_bytes as f64 / swap.total_bytes as f64 * 100.0)
                        .clamp(0.0, 100.0);
                    format!(
                        "{} / {} ({pct:.0}%)",
                        bytes(swap.used_bytes),
                        bytes(swap.total_bytes)
                    )
                })
        }
        SummaryField::NetReceive => network_rate(
            network_devices(shell),
            NetworkMetrics::current_rx_bytes_per_sec,
        ),
        SummaryField::NetSend => network_rate(
            network_devices(shell),
            NetworkMetrics::current_tx_bytes_per_sec,
        ),
    }
}

/// One core's folded utilization observation — the read every per-core view
/// (numeric cell, bar fill) shares, so a bar and its number can never come
/// from two different observations.
pub(super) fn core_usage_pct(shell: &ShellApp, index: usize) -> Option<f32> {
    cpu_metrics(shell).and_then(|cpu| cpu.current_core_usage_pct(index))
}

/// How many per-core usages the projection carries right now; zero until the
/// CPU domain supplies a core group, so an unmeasured machine paints no grid.
pub(super) fn core_usage_count(shell: &ShellApp) -> usize {
    cpu_metrics(shell)
        .map(|cpu| cpu.current_core_usage_len())
        .unwrap_or(0)
}

/// The per-core bar's paint-ready fill percentage: a finite observation clamps
/// into the 0..=100 range, anything else collapses to a zero fill — a missing
/// fact is never painted as progress.
pub(super) fn core_usage_fill_pct(shell: &ShellApp, index: usize) -> f32 {
    core_usage_pct(shell, index)
        .filter(|value| value.is_finite())
        .map_or(0.0, |value| value.clamp(0.0, 100.0))
}

/// A disk rail row's caption: activity percentage and the two transfer rates.
/// Each fact keeps its own dash-on-missing semantics.
pub(super) struct PartitionViewModel {
    pub(super) name: String,
    pub(super) usage_text: String,
    pub(super) pct: f32,
}

pub(super) fn disk_partition_view_models(disk: &DiskMetrics) -> Vec<PartitionViewModel> {
    disk.partitions
        .iter()
        .map(|part| {
            let name = if !part.mount_point.is_empty() {
                format!("{} ({})", part.mount_point, part.fs_type)
            } else {
                part.name.clone()
            };
            let (used_str, cap_str, pct) =
                match (part.current_used_bytes(), part.current_capacity_bytes()) {
                    (Some(used), Some(cap)) if cap > 0 => {
                        let pct = (used as f32 / cap as f32 * 100.0).clamp(0.0, 100.0);
                        (bytes(used), bytes(cap), pct)
                    }
                    _ => (missing_value(), missing_value(), 0.0),
                };
            PartitionViewModel {
                name,
                usage_text: format!("{used_str} / {cap_str} ({pct:.0}%)"),
                pct,
            }
        })
        .collect()
}

pub(super) struct GpuVramViewModel {
    pub(super) label: String,
    pub(super) pct: f32,
}

pub(super) fn gpu_vram_view_model(gpu: &GpuMetrics) -> Option<GpuVramViewModel> {
    let used = gpu.current_dedicated_vram_used_bytes()?;
    let total = gpu.current_dedicated_vram_total_bytes()?;
    if total == 0 {
        return None;
    }
    let pct = (used as f32 / total as f32 * 100.0).clamp(0.0, 100.0);
    Some(GpuVramViewModel {
        label: format!(
            "{}: {} / {} ({pct:.0}%)",
            t("npu.dedicated_memory"),
            bytes(used),
            bytes(total)
        ),
        pct,
    })
}

pub(super) fn disk_caption(disk: &DiskMetrics) -> String {
    let rate = |value: Option<u64>| value.map_or_else(missing_value, bytes);
    let mut parts = vec![
        disk.current_active_time_pct()
            .map_or_else(missing_value, |value| format!("{value:.0}%")),
        rate(disk.current_read_bytes_per_sec()),
        rate(disk.current_write_bytes_per_sec()),
    ];
    if let Some(iops) = disk.current_iops() {
        parts.push(format!("{} {iops}", t("disk.iops")));
    }
    if let Some(ms) = disk.current_response_time_ms() {
        parts.push(format!("{ms:.1} ms"));
    }
    if let Some(depth) = disk.current_average_queue_depth() {
        parts.push(format!("{} {depth:.2}", t("disk.queue_depth")));
    }
    if let Some(ms) = disk.current_service_time_ms() {
        parts.push(format!("{} {ms:.1} ms", t("disk.service_time")));
    }
    if let Some(temperature) = disk.smart_temperature_c.filter(|value| value.is_finite()) {
        parts.push(format!(
            "{} {temperature:.0}\u{b0}C",
            t("common.temperature")
        ));
    }
    for (index, temperature) in disk.smart_temperature_sensors_c.iter().enumerate() {
        if temperature.is_finite() {
            parts.push(format!(
                "{} {} {:.0}\u{b0}C",
                t("disk.temperature_sensor"),
                index + 1,
                temperature
            ));
        }
    }
    if let Some(pct) = disk.smart_percent_used.filter(|value| value.is_finite()) {
        parts.push(format!("{} {pct:.0}%", t("disk.endurance_used")));
    }
    if let Some(spare) = disk.smart_available_spare_pct {
        // The warning affordance is a semantic bitmap sibling in the scene;
        // keep this dynamic text free of decoration codepoints (the tofu law).
        parts.push(format!("{} {spare:.0}%", t("disk.available_spare")));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        let days = hours / 24;
        parts.push(
            t("disk.power_on_format")
                .replace("{hours}", &hours.to_string())
                .replace("{days}", &days.to_string()),
        );
    }
    if let Some(count) = disk.smart_unsafe_shutdowns {
        parts.push(format!("{} {count}", t("disk.unsafe_shutdowns")));
    }
    // Reported availability is an existence fact, not a measurement: when the
    // provider reports a usable SMART state but no concrete field, the shared
    // status fold still speaks (Iced parity), and nothing is fabricated when
    // the section is honestly hidden.
    if smart_section_visible(disk) && !has_smart_fields(disk) {
        parts.push(format!(
            "{} {}",
            t("disk.smart_status"),
            t(device_status_i18n_key(effective_smart_status(disk))),
        ));
    }
    if disk.current_read_merges_per_sec().is_some() || disk.current_write_merges_per_sec().is_some()
    {
        parts.push(format!(
            "{} {}/{}",
            t("disk.merged_requests"),
            disk.current_read_merges_per_sec()
                .map_or_else(missing_value, |value| value.to_string()),
            disk.current_write_merges_per_sec()
                .map_or_else(missing_value, |value| value.to_string()),
        ));
    }
    parts.join(" · ")
}

/// Whether the disk's observed spare pool crossed its typed warning level.
/// The caller owns the warning icon; this predicate only folds facts and never
/// treats an absent or non-finite SMART reading as a warning.
pub(super) fn disk_spare_warning(disk: &DiskMetrics) -> bool {
    let Some(spare) = disk.smart_available_spare_pct else {
        return false;
    };
    if !spare.is_finite() {
        return false;
    }
    let threshold = disk.smart_available_spare_threshold_pct.unwrap_or(10.0);
    threshold.is_finite() && spare <= threshold
}

pub(super) fn disk_spare_warning_for(shell: &ShellApp, device_id: &str) -> bool {
    disks(shell)
        .and_then(|devices| devices.iter().find(|disk| disk.device_id == device_id))
        .is_some_and(disk_spare_warning)
}

pub(super) fn battery_sidebar_title(battery: &BatteryInfo, index: usize) -> String {
    if !battery.model_name.trim().is_empty() {
        battery.model_name.trim().to_string()
    } else if !battery.display_name.trim().is_empty() {
        battery.display_name.trim().to_string()
    } else {
        format!("{} {index}", t("common.battery"))
    }
}

pub(super) fn battery_caption(battery: &BatteryInfo) -> String {
    let charge = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |pct| format!("{pct}%"));
    let watts = battery
        .current_power_w()
        .filter(|w| w.is_finite())
        .map_or(String::new(), |w| format!(" · {w:.1} W"));
    format!("{charge}{watts}")
}

/// Per-core usages, one readout per projected core with honest dashes for
/// per-core gaps; no cores observed at all renders the plain dash.
pub(super) fn core_summary(shell: &ShellApp) -> String {
    let count = core_usage_count(shell);
    if count == 0 {
        return missing_value();
    }
    let visible = count.min(3);
    let mut summary = (0..visible)
        .map(|index| observed_percentage(core_usage_pct(shell, index)))
        .collect::<Vec<_>>()
        .join(" ");
    if count > visible {
        summary.push_str(" …");
    }
    summary
}

/// "used / total · pct" with a dash per missing side; the percentage comes
/// from the core's own observed fold, never recomputed here.
pub(super) fn memory_summary(memory: Option<&MemoryMetrics>) -> String {
    let Some(memory) = memory else {
        return missing_value();
    };
    let used = memory.current_used_bytes().map(bytes);
    let total = memory.current_total_bytes().map(bytes);
    let percentage = memory
        .used_percentage_observed()
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}%"));
    let line = format!(
        "{} / {}",
        used.unwrap_or_else(missing_value),
        total.unwrap_or_else(missing_value)
    );
    let mut summary = match percentage {
        Some(percentage) => format!("{line} · {percentage}"),
        None => line,
    };
    if let Some(rate) = memory.current_swap_in_bytes_per_sec() {
        summary.push_str(&format!(" · {} {}/s", t("mem.swap_in_rate"), bytes(rate)));
    }
    if let Some(rate) = memory.current_swap_out_bytes_per_sec() {
        summary.push_str(&format!(" · {} {}/s", t("mem.swap_out_rate"), bytes(rate)));
    }
    summary
}

/// Sum one rate direction across the projected adapters. An empty or absent
/// list — or any adapter missing the fact — stays a dash: a partial sum (or
/// a zero over nothing) would read as the system total it is not.
pub(super) fn network_rate(
    devices: Option<&[NetworkMetrics]>,
    rate: fn(&NetworkMetrics) -> Option<u64>,
) -> String {
    let Some(devices) = devices.filter(|devices| !devices.is_empty()) else {
        return missing_value();
    };
    devices
        .iter()
        .map(rate)
        .collect::<Option<Vec<u64>>>()
        .map_or_else(missing_value, |rates| {
            format!("{}/s", bytes(rates.iter().sum()))
        })
}

pub(super) fn curve_samples(shell: &ShellApp, curve: SystemCurve) -> Vec<f32> {
    if curve == SystemCurve::Npu {
        return shell
            .projection()
            .npu_usage_history
            .iter()
            .copied()
            .collect();
    }
    window(&shell.history, curve.series())
}

/// TUI parity: a window under two samples is still collecting — the curve
/// area shows the collecting placeholder, never a fabricated flat line.
pub(super) fn curve_warm(samples: &[f32]) -> bool {
    samples.len() >= 2
}

pub(crate) fn curve_caption(shell: &ShellApp, curve: SystemCurve) -> String {
    let samples = curve_samples(shell, curve);
    if !curve_warm(&samples) {
        return t("perf.collecting_samples").to_owned();
    }
    graph_summary(&samples).map_or_else(missing_value, |summary| {
        format!(
            "{} {} · {} {} · {} {}",
            t("common.latest"),
            curve.format_value(summary.latest),
            t("common.avg"),
            curve.format_value(summary.average),
            t("common.peak"),
            curve.format_value(summary.maximum),
        )
    })
}

pub(super) fn curve_wanted(shell: &ShellApp, curve: SystemCurve) -> bool {
    match curve {
        SystemCurve::Gpu => gpu_devices(shell).is_some_and(|devices| !devices.is_empty()),
        SystemCurve::Npu => shell
            .projection()
            .npu_inventory
            .as_ref()
            .is_some_and(|inv| !inv.devices.is_empty()),
        _ => true,
    }
}

pub(super) fn segment_key(kind: MemSegmentKind) -> String {
    format!("{kind:?}")
}

/// Ordered block keys for one section: the shell projection's device list
/// (stable device ids) or the shared memory segment kinds.
pub(crate) fn section_keys(shell: &ShellApp, section: Section) -> Vec<String> {
    match section {
        Section::Gpu => gpu_devices(shell).map_or_else(Vec::new, |devices| {
            devices.iter().map(|gpu| gpu.device_id.clone()).collect()
        }),
        Section::Network => network_devices(shell).map_or_else(Vec::new, |devices| {
            devices
                .iter()
                .map(|nic| (*nic.device_id).to_owned())
                .collect()
        }),
        Section::MemorySegments => memory_metrics(shell).map_or_else(Vec::new, |memory| {
            memory_segments(memory)
                .iter()
                .map(|segment| segment_key(segment.kind))
                .collect()
        }),
        Section::Disk => disks(shell).map_or_else(Vec::new, |devices| {
            devices.iter().map(|disk| disk.device_id.clone()).collect()
        }),
        Section::Battery => batteries(shell).map_or_else(Vec::new, |devices| {
            devices.iter().map(|b| b.id.clone()).collect()
        }),
    }
}

pub(super) fn battery_fact_line(battery: &BatteryInfo) -> String {
    let charge = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |pct| format!("{pct}%"));
    let watts = battery
        .current_power_w()
        .filter(|w| w.is_finite())
        .map_or_else(missing_value, |w| format!("{w:.1} W"));
    format!(
        "{}: {charge} · {}: {watts}",
        t("battery.capacity"),
        t("battery.power")
    )
}

/// One GPU block's joined fact line; each fact keeps its own dash-on-missing
/// semantics (TUI `gpu_data` parity via the shared formatters).
pub(super) fn gpu_fact_line(gpu: &GpuMetrics) -> String {
    let mut values = vec![
        observed_percentage(gpu.current_utilization_pct()),
        gpu.current_temperature_c()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, temperature_c),
        gpu.current_frequency_mhz()
            .map_or_else(missing_value, |mhz| megahertz(mhz as f32)),
        gpu.current_power_w()
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, power_w),
        gpu_memory_line(gpu),
    ];
    if let Some(connected) = gpu.display_connected {
        values.push(format!(
            "{} {}",
            t("gpu.display_output"),
            t(if connected { "common.yes" } else { "common.no" })
        ));
    }
    if let Some(version) = gpu
        .vbios_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {version}", t("gpu.vbios_version")));
    }
    if let Some(api) = gpu.graphics_api.as_ref()
        && let Some(version) = api
            .mesa_version
            .as_deref()
            .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {version}", t("gpu.mesa_version")));
    }
    if let Some(rpm) = gpu.current_fan_speed_rpm() {
        values.push(format!("{} {rpm} RPM", t("fan.rpm")));
    }
    if let Some(pct) = gpu
        .current_fan_speed_pct()
        .filter(|value| value.is_finite())
    {
        values.push(format!("{} {pct:.0}%", t("fan.pwm")));
    }
    if let Some(bandwidth) = gpu
        .memory_bandwidth_gbps
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        values.push(format!("{} {bandwidth:.1} GB/s", t("gpu.memory_bandwidth")));
    }
    if let Some(depth) = gpu.queue_depth {
        values.push(format!("{} {depth}", t("gpu.queue_depth")));
    }
    values.join(" · ")
}

pub(super) fn nic_fact_line(nic: &NetworkMetrics) -> String {
    let rate = |value: Option<u64>| {
        value.map_or_else(missing_value, |value| format!("{}/s", bytes(value)))
    };
    let mut values = vec![
        rate(nic.current_rx_bytes_per_sec()),
        rate(nic.current_tx_bytes_per_sec()),
        nic.current_link_speed_mbps()
            .map_or_else(missing_value, |mbps| format!("{mbps} Mbps")),
    ];
    if let Some(width) = nic.current_channel_width_mhz() {
        values.push(format!("{width} MHz"));
    }
    if let Some(mtu) = nic.current_mtu_bytes() {
        values.push(format!("{} {mtu} B", t("net.mtu")));
    }
    if let Some(queue) = nic.current_tx_queue_len() {
        values.push(format!("{} {queue}", t("net.tx_queue")));
    }
    for (label, left, right) in [
        (
            t("net.drops"),
            nic.current_rx_drops(),
            nic.current_tx_drops(),
        ),
        (
            t("net.errors"),
            nic.current_rx_errors(),
            nic.current_tx_errors(),
        ),
        (
            t("net.overruns"),
            nic.current_rx_overruns(),
            nic.current_tx_overruns(),
        ),
    ] {
        if left.is_some() || right.is_some() {
            values.push(format!(
                "{label} {} / {}",
                left.map_or_else(missing_value, |value| value.to_string()),
                right.map_or_else(missing_value, |value| value.to_string()),
            ));
        }
    }
    if let Some(master) = nic
        .master_interface
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {master}", t("net.master")));
    }
    if let Some(peer) = nic
        .peer_interface
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        values.push(format!("{} {peer}", t("net.peer")));
    }
    values.join(" · ")
}

/// A device block's current fact line from the projection; a device id that
/// left the projection renders the dash (its block is being despawned).
pub(crate) fn device_line(shell: &ShellApp, section: Section, device: &str) -> String {
    match section {
        Section::Gpu => gpu_devices(shell)
            .and_then(|devices| devices.iter().find(|gpu| gpu.device_id == device))
            .map_or_else(missing_value, gpu_fact_line),
        Section::Network => network_devices(shell)
            .and_then(|devices| devices.iter().find(|nic| &*nic.device_id == device))
            .map_or_else(missing_value, nic_fact_line),
        Section::MemorySegments => missing_value(),
        Section::Disk => disks(shell)
            .and_then(|devices| devices.iter().find(|disk| disk.device_id == device))
            .map_or_else(missing_value, disk_caption),
        Section::Battery => batteries(shell)
            .and_then(|devices| devices.iter().find(|b| b.id == device))
            .map_or_else(missing_value, battery_fact_line),
    }
}

/// First present VRAM pair (dedicated, then shared, then general) rendered as
/// "used / total"; a pair needs a positive total — an absent counter is not a
/// believable zero capacity (TUI `gpu_data` parity).
pub(super) fn gpu_memory_line(gpu: &GpuMetrics) -> String {
    let vram_pair = |used: Option<u64>, total: Option<u64>| -> Option<(u64, u64)> {
        match (used, total) {
            (Some(used), Some(total)) if total > 0 => Some((used.min(total), total)),
            _ => None,
        }
    };
    let pair = vram_pair(
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    )
    .or_else(|| {
        vram_pair(
            gpu.current_shared_vram_used_bytes(),
            gpu.current_shared_vram_total_bytes(),
        )
    })
    .or_else(|| {
        vram_pair(
            gpu.current_memory_used_bytes(),
            gpu.current_memory_total_bytes(),
        )
    });
    pair.map_or_else(missing_value, |(used, total)| {
        format!("{} / {}", bytes(used), bytes(total))
    })
}

pub(crate) fn segment_value(shell: &ShellApp, kind: MemSegmentKind) -> String {
    let Some(memory) = memory_metrics(shell) else {
        return missing_value();
    };
    memory_segments(memory)
        .iter()
        .find(|segment| segment.kind == kind)
        .map_or_else(missing_value, |segment| {
            segment_line(segment, memory.current_total_bytes())
        })
}

/// One composition legend row: byte count plus its clamped share of a known
/// positive total (the segment math itself — which categories exist and
/// their saturating byte sums — is owned by `taskmanager_shell::memory`).
pub(super) fn segment_line(segment: &MemSegment, total: Option<u64>) -> String {
    let share = total.filter(|total| *total > 0).map(|total| {
        let pct = (segment.bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
        format!("{pct:.0}%")
    });
    share.map_or_else(
        || bytes(segment.bytes),
        |share| format!("{} · {share}", bytes(segment.bytes)),
    )
}

pub(super) fn dyn_field_text(shell: &ShellApp, field: &DynField) -> String {
    match field {
        DynField::Summary(field) => summary_value(shell, *field),
        DynField::CurveCaption(curve) => curve_caption(shell, *curve),
        DynField::Cpu(field) => cpu_field_text(shell, *field),
        DynField::Device { section, device } => device_line(shell, *section, device),
        DynField::Segment(kind) => segment_value(shell, *kind),
    }
}
