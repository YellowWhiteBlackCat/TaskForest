//! Pure Performance-page metric projections and formatting.

use super::*;

/// Percent readout. There is no shared percent formatter in
/// `shell::presentation` (the TUI keeps its own in `ui/units.rs`), so this
/// page owns one with the same semantics: missing and non-finite observations
/// render the shared dash, never a fabricated `0.0%`.
pub(super) fn observed_percentage(value: Option<f32>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(missing_value, |value| format!("{value:.1}%"))
}

/// A domain observation's usable value: current/partial read the live value,
/// stale/unavailable keep the last known one — `None` stays `None` all the
/// way to a dash, never a zero.
pub(super) fn usable_value<'a, T, V>(
    state: &'a SystemTelemetryDomainState<T>,
    current: fn(&'a T) -> Option<V>,
    last_known: fn(&'a T) -> Option<V>,
) -> Option<V> {
    match state {
        SystemTelemetryDomainState::Current(observation)
        | SystemTelemetryDomainState::Partial(observation) => current(observation),
        SystemTelemetryDomainState::Stale(observation)
        | SystemTelemetryDomainState::Unavailable {
            observation: Some(observation),
            ..
        } => last_known(observation),
        _ => None,
    }
}

/// Read the correlated six-domain projection first. The complete shell
/// snapshot is the honest fallback for demo/cold-start frames: it is already
/// the shell's committed render model, so Performance does not show dashes
/// merely because the newer partial telemetry stream has not warmed yet.
pub(super) fn cpu_metrics(shell: &ShellApp) -> Option<&CpuMetrics> {
    shell
        .projection()
        .system_telemetry
        .as_ref()
        .and_then(|telemetry| {
            usable_value(
                &telemetry.cpu,
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
            usable_value(
                &telemetry.memory,
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
            usable_value(
                &telemetry.gpu,
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
            usable_value(
                &telemetry.network,
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

pub(crate) fn summary_value(shell: &ShellApp, field: SummaryField) -> String {
    match field {
        SummaryField::Cpu => {
            observed_percentage(cpu_metrics(shell).and_then(|cpu| cpu.current_global_usage_pct()))
        }
        SummaryField::Cores => core_summary(cpu_metrics(shell)),
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

/// Per-core usages, one readout per projected core with honest dashes for
/// per-core gaps; no cores observed at all renders the plain dash.
pub(super) fn core_summary(cpu: Option<&CpuMetrics>) -> String {
    let Some(cpu) = cpu else {
        return missing_value();
    };
    let count = cpu.current_core_usage_len();
    if count == 0 {
        return missing_value();
    }
    (0..count)
        .map(|index| observed_percentage(cpu.current_core_usage_pct(index)))
        .collect::<Vec<_>>()
        .join(" ")
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
    match percentage {
        Some(percentage) => format!("{line} · {percentage}"),
        None => line,
    }
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
    shell.history.series(curve.series())
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

pub(super) fn cpu_field_text(shell: &ShellApp, field: CpuField) -> String {
    let Some(cpu) = cpu_metrics(shell) else {
        return missing_value();
    };
    match field {
        CpuField::Brand => cpu
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|brand| !brand.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(missing_value),
        CpuField::Usage => observed_percentage(cpu.current_global_usage_pct()),
        CpuField::Frequency => cpu
            .current_frequency_mhz()
            .map(|value| megahertz(value as f32))
            .unwrap_or_else(missing_value),
        CpuField::Temperature => cpu
            .current_temperature_c()
            .filter(|value| value.is_finite())
            .map(temperature_c)
            .unwrap_or_else(missing_value),
        CpuField::Power => cpu
            .current_power_w()
            .filter(|value| value.is_finite())
            .map(power_w)
            .unwrap_or_else(missing_value),
        CpuField::Core(index) => observed_percentage(cpu.current_core_usage_pct(index)),
    }
}

/// Bar fractions for one strip: the shared sparkline projection, emptied
/// while the window is not warm (the strip renders nothing rather than a
/// fake line).
pub(crate) fn strip_fractions(shell: &ShellApp, curve: SystemCurve) -> Vec<f32> {
    let samples = curve_samples(shell, curve);
    if curve_warm(&samples) {
        bar_fractions(&samples)
    } else {
        Vec::new()
    }
}

pub(super) fn curve_wanted(shell: &ShellApp, curve: SystemCurve) -> bool {
    match curve {
        SystemCurve::Gpu => gpu_devices(shell).is_some_and(|devices| !devices.is_empty()),
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
    }
}

/// One GPU block's joined fact line; each fact keeps its own dash-on-missing
/// semantics (TUI `gpu_data` parity via the shared formatters).
pub(super) fn gpu_fact_line(gpu: &GpuMetrics) -> String {
    [
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
    ]
    .join(" · ")
}

pub(super) fn nic_fact_line(nic: &NetworkMetrics) -> String {
    let rate = |value: Option<u64>| {
        value.map_or_else(missing_value, |value| format!("{}/s", bytes(value)))
    };
    [
        rate(nic.current_rx_bytes_per_sec()),
        rate(nic.current_tx_bytes_per_sec()),
        nic.current_link_speed_mbps()
            .map_or_else(missing_value, |mbps| format!("{mbps} Mbps")),
    ]
    .join(" · ")
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
