//! The live per-instance segments for the Performance selector strip, read
//! from the same projection snapshots and the same generation-scoped
//! `LiveGraphHistory` windows as the device detail below and the gpui device
//! sidebar (`sidebar.rs` + `sidebar/captions.rs`).

use super::*;
use taskmanager_core::core::metrics::{GpuMetrics, NetworkAdapterType};
use taskmanager_core::core::sensors::SensorQuantity;
use taskmanager_core::core::units::format_quantity_with;
use taskmanager_shell::presentation::trend;
use taskmanager_shell::presentation::{gpu_display_identity, missing_value};

pub(super) fn perf_selector_instances(app: &TuiApp, theme: TuiTheme) -> Vec<SelectorInstance> {
    let Some(snapshot) = app.projection().snapshot.as_ref() else {
        return Vec::new();
    };
    let window = app.prefs.graph_points;
    let glyphs = theme.terminal.glyphs;
    match app.perf_device {
        PerfDevice::Cpu => vec![cpu_selector_instance(app, theme, snapshot, glyphs, window)],
        PerfDevice::Memory => {
            vec![memory_selector_instance(
                app, theme, snapshot, glyphs, window,
            )]
        }
        PerfDevice::Disk => snapshot
            .disks
            .iter()
            .filter(|_| app.prefs.show[2])
            .map(|disk| {
                let trend = sparkline::device_trend_in(
                    glyphs,
                    &app.history
                        .disk_active_time_pct_for(&disk.device_id, disk.device_generation.get()),
                    window,
                );
                let active = disk
                    .current_active_time_pct()
                    .map_or_else(missing_value, |value| format!("{:.0}%", value.round()));
                let rate = match (
                    disk.current_read_bytes_per_sec(),
                    disk.current_write_bytes_per_sec(),
                ) {
                    (Some(read), Some(write)) => format_quantity_with(
                        read.saturating_add(write),
                        app.prefs.units[2],
                        app.prefs.units[3],
                        true,
                    ),
                    _ => missing_value(),
                };
                let heading = if disk.model.is_empty() {
                    disk.name.trim_start_matches("/dev/").to_owned()
                } else {
                    disk.model.clone()
                };
                selector_instance(
                    IconId::Disk,
                    &heading,
                    &trend,
                    format!("{active} · {rate}"),
                    theme,
                )
            })
            .collect(),
        PerfDevice::Network => snapshot
            .networks
            .iter()
            .filter(|network| selector_network_visible(&app.prefs.show, network.adapter_type()))
            .map(|network| {
                let trend = sparkline::device_trend_in(
                    glyphs,
                    &app.history.network_bytes_per_sec_for(
                        &network.device_id,
                        network.device_generation.get(),
                    ),
                    window,
                );
                let rate = |bytes_per_sec: Option<u64>| {
                    bytes_per_sec.map_or_else(missing_value, |value| {
                        format_quantity_with(value, app.prefs.units[4], app.prefs.units[5], true)
                    })
                };
                let caption = format!(
                    "{} {} · {} {}",
                    t("sidebar.send_label"),
                    rate(network.current_tx_bytes_per_sec()),
                    t("sidebar.recv_label"),
                    rate(network.current_rx_bytes_per_sec()),
                );
                selector_instance(
                    IconId::Network,
                    &network.interface_name,
                    &trend,
                    caption,
                    theme,
                )
            })
            .collect(),
        PerfDevice::Gpu => snapshot
            .gpu
            .iter()
            .filter(|_| app.prefs.show[9])
            .map(|gpu| {
                let trend = sparkline::device_trend_in(
                    glyphs,
                    &app.history
                        .gpu_usage_pct_for(&gpu.device_id, gpu.device_generation.get()),
                    window,
                );
                let heading = gpu_display_identity(gpu)
                    .headline
                    .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE);
                selector_instance(
                    IconId::Gpu,
                    heading,
                    &trend,
                    gpu_selector_caption(gpu, app.prefs.units[0], app.prefs.units[1]),
                    theme,
                )
            })
            .collect(),
        PerfDevice::Battery => app
            .projection()
            .power_supplies
            .as_ref()
            .map(|power| power.batteries.iter().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, battery)| {
                let trend = sparkline::device_trend_in(
                    glyphs,
                    &app.history.battery_capacity_pct_for(&battery.id),
                    window,
                );
                let heading = if !battery.model_name.is_empty() {
                    battery.model_name.clone()
                } else if !battery.display_name.is_empty() {
                    battery.display_name.clone()
                } else {
                    format!("{} {index}", t("common.battery"))
                };
                let capacity = battery
                    .current_capacity_pct()
                    .map_or_else(missing_value, |value| format!("{value}%"));
                let status = if battery.status.is_empty() {
                    missing_value()
                } else {
                    battery.status.clone()
                };
                selector_instance(
                    IconId::Health,
                    &heading,
                    &trend,
                    format!("{capacity} · {status}"),
                    theme,
                )
            })
            .collect(),
        PerfDevice::Fan => app
            .projection()
            .sensors
            .as_ref()
            .map(|sensors| sensors.readings.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
            .enumerate()
            .map(|(index, reading)| {
                let trend = sparkline::device_trend_in(
                    glyphs,
                    &app.history.fan_rpm_for(reading.id()),
                    window,
                );
                let rpm = reading
                    .current_number()
                    .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
                let label = reading.label();
                let caption = if label.is_empty() {
                    rpm
                } else {
                    format!("{label} · {rpm}")
                };
                selector_instance(
                    IconId::Health,
                    &format!("{} {}", t("common.fan"), index + 1),
                    &trend,
                    caption,
                    theme,
                )
            })
            .collect(),
    }
}

/// The applied network-subcategory visibility (the same class map the
/// network detail panel applies), so a hidden NIC class drops out of the
/// strip too instead of ghosting back beside the filtered panel.
fn selector_network_visible(show: &[bool; 10], adapter_type: NetworkAdapterType) -> bool {
    match adapter_type {
        NetworkAdapterType::Ethernet => show[4],
        NetworkAdapterType::WiFi => show[5],
        NetworkAdapterType::Vpn => show[6],
        NetworkAdapterType::Virtual => show[7],
        NetworkAdapterType::Unknown | NetworkAdapterType::Loopback | NetworkAdapterType::Other => {
            show[8]
        }
    }
}

/// The CPU strip segment: brand plus utilization / clock / package
/// temperature — the gpui `cpu_caption` fields on one line.
fn cpu_selector_instance(
    app: &TuiApp,
    theme: TuiTheme,
    snapshot: &taskmanager_core::core::metrics::SystemSnapshot,
    glyphs: crate::TuiGlyphMode,
    window: usize,
) -> SelectorInstance {
    let cpu = &snapshot.cpu;
    let trend = sparkline::device_trend_in(glyphs, &trend::cpu_usage_percent(&app.history), window);
    let mut caption = vec![
        cpu.current_global_usage_pct()
            .map_or_else(missing_value, |value| format!("{:.0}%", value.round())),
    ];
    if let Some(mhz) = cpu.current_frequency_mhz() {
        caption.push(selector_clock_readout(mhz));
    }
    if let Some(temp) = cpu.current_temperature_c() {
        caption.push(format!("{:.0} °C", temp.round()));
    }
    let brand = cpu
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(missing_value, str::to_owned);
    selector_instance(IconId::Cpu, &brand, &trend, caption.join(" · "), theme)
}

/// The memory strip segment: used / total plus the observed percentage — the
/// gpui `mem_caption` fields.
fn memory_selector_instance(
    app: &TuiApp,
    theme: TuiTheme,
    snapshot: &taskmanager_core::core::metrics::SystemSnapshot,
    glyphs: crate::TuiGlyphMode,
    window: usize,
) -> SelectorInstance {
    let memory = &snapshot.memory;
    let trend =
        sparkline::device_trend_in(glyphs, &trend::memory_usage_percent(&app.history), window);
    let readout = |value: u64| memory_text_pref(value, app.prefs.units[0], app.prefs.units[1]);
    let used = memory
        .current_used_bytes()
        .map_or_else(missing_value, readout);
    let total = memory
        .current_total_bytes()
        .map_or_else(missing_value, readout);
    let percentage = memory
        .used_percentage_observed()
        .map_or_else(missing_value, |value| format!("{value:.0}%"));
    selector_instance(
        IconId::Memory,
        t("common.memory"),
        &trend,
        format!("{used} / {total} · {percentage}"),
        theme,
    )
}

/// Caption clock: `{:.2} GHz` at 1000 MHz and above, plain MHz below — the
/// gpui sidebar caption spelling.
fn selector_clock_readout(mhz: u64) -> String {
    if mhz >= 1000 {
        format!("{:.2} GHz", mhz as f64 / 1000.0)
    } else {
        format!("{mhz} MHz")
    }
}

/// The GPU strip caption: the absolute-memory-or-clock identity line followed
/// by utilization / VRAM share / temperature / power — the gpui
/// `gpu_caption_line1` + `gpu_caption_line2` fields on one line.
fn gpu_selector_caption(gpu: &GpuMetrics, use_bytes: bool, use_base2: bool) -> String {
    let readout = |value: u64| memory_text_pref(value, use_bytes, use_base2);
    // Same cascade as the gpui first caption line: advertised dedicated VRAM
    // above 512 MiB, then the split-memory aperture, then dedicated clamped
    // to its own total.
    let (vram_used, vram_total) = if let (Some(used), Some(total)) = (
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    ) && total > 512 * 1024 * 1024
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        gpu.current_memory_used_bytes(),
        gpu.current_memory_total_bytes(),
    ) && total > 0
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    ) && total > 0
    {
        (Some(used.min(total)), Some(total))
    } else {
        (None, None)
    };
    let mut parts = vec![if let (Some(used), Some(total)) = (vram_used, vram_total)
        && total > 0
    {
        format!("VRAM {} / {}", readout(used), readout(total))
    } else if let Some(total) = vram_total
        && total > 0
    {
        format!("VRAM {}", readout(total))
    } else {
        match (gpu.current_frequency_mhz(), gpu.current_max_frequency_mhz()) {
            (Some(current), Some(max)) if max > 0 => format!("{current} / {max} MHz"),
            (Some(current), _) => format!("{current} MHz"),
            _ => gpu
                .driver
                .as_deref()
                .map(str::trim)
                .filter(|driver| !driver.is_empty())
                .map_or_else(missing_value, |driver| format!("Driver {driver}")),
        }
    }];
    if let Some(util) = gpu.current_utilization_pct() {
        parts.push(format!("{util:.0}%"));
    }
    if let (Some(used), Some(total)) = (vram_used, vram_total)
        && total > 0
    {
        let pct = (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
        parts.push(format!("VRAM {pct:.0}%"));
    }
    if let Some(temp) = gpu.current_temperature_c() {
        parts.push(format!("{:.0} °C", temp.round()));
    }
    if let Some(power) = gpu.current_power_w()
        && power > 0.0
    {
        parts.push(format!("{power:.0} W"));
    }
    parts.join(" · ")
}
