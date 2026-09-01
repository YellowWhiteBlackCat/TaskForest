//! Performance page Disk detail block, throughput chart and partition summary.

use std::rc::Rc;

use super::*;
use iced::Element;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::{DiskMetrics, DiskPartition, SystemSnapshot};

use taskmanager_shell::presentation::{
    device_status_i18n_key, effective_smart_status, has_smart_fields, missing_value,
    smart_section_visible,
};
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::tokens;

use super::super::responsive::{
    DeviceNavigationPresentation, PerformanceChartInventory, PerformancePageBudget,
};

/// The Performance-page per-disk panel readiness.
#[must_use]
pub(crate) fn disk_section_state(snapshot: Option<&SystemSnapshot>) -> tables::ListState {
    match snapshot {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.disks.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One disk's display identity (GPUI parity): the model when known, else the
/// `/dev/`-stripped name — no family prefix.
#[must_use]
pub(crate) fn disk_title(disk: &DiskMetrics) -> String {
    if disk.model.trim().is_empty() {
        disk.name.trim_start_matches("/dev/").to_string()
    } else {
        disk.model.trim().to_string()
    }
}

/// The disk page's undroppable one-line capacity fact (GPUI
/// `disk_vital_line` parity): used/total plus the partition census. Honest
/// absence — a disk whose capacity or partition facts are uncollected keeps
/// the dash or omits the segment, never a fabricated zero.
#[must_use]
pub(crate) fn disk_vital_line(disk: &DiskMetrics, units: UnitPrefs) -> String {
    let observed = super::projection::DiskObservation::from(disk);
    let mut segments = Vec::new();
    match (observed.capacity_bytes, observed.available_bytes) {
        (Some(total), Some(free)) if total > 0 => {
            let used = total.saturating_sub(free).min(total);
            segments.push(format!(
                "{} / {}",
                quantity_text_pref(used, units.use_bytes, units.use_base2),
                quantity_text_pref(total, units.use_bytes, units.use_base2),
            ));
        }
        _ => segments.push(missing_value()),
    }
    if !disk.partitions.is_empty() {
        segments.push(format!(
            "{} {}",
            disk.partitions.len(),
            t("disk.partitions").to_lowercase(),
        ));
    }
    segments.join(" · ")
}

/// Build the disk stat readout rows as pre-folded shell [`StatRow`]s for the
/// Performance device page (GPUI `disk_stats` parity: one fold, three
/// renderers). Rate-family fields whose absence is a sampling gap keep their
/// row with `None` (the shared dash); existence facts omit their rows when
/// the host does not have them.
#[must_use]
pub(crate) fn disk_summary_lines(
    disk: &DiskMetrics,
    use_bytes: bool,
    use_base2: bool,
    temperature_samples: &[f32],
) -> Vec<StatRow> {
    let observed = super::projection::DiskObservation::from(disk);
    let rate = |value: Option<u64>| value.map(|v| rate_text_pref(Some(v), use_bytes, use_base2));
    let mut rows = vec![
        StatRow::text(
            t("device.status"),
            Some(t(device_status_i18n_key(disk.device_state.status)).to_string()),
        ),
        StatRow::text(
            t("disk.active_time"),
            observed
                .active_time_pct
                .map(|value| format!("{:.0}%", value.round())),
        ),
        StatRow::text(t("disk.read"), rate(observed.read_bytes_per_sec)),
        StatRow::text(t("disk.write"), rate(observed.write_bytes_per_sec)),
        StatRow::text(t("disk.iops"), observed.iops.map(|v| v.to_string())),
        StatRow::text(
            t("disk.response"),
            observed
                .response_time_ms
                .map(|value| format!("{value:.2} ms")),
        ),
        StatRow::text(
            t("disk.capacity"),
            observed
                .capacity_bytes
                .map(|v| quantity_text_pref(v, use_bytes, use_base2)),
        ),
        StatRow::text(
            t("disk.free"),
            observed
                .available_bytes
                .map(|v| quantity_text_pref(v, use_bytes, use_base2)),
        ),
        StatRow::text(t("common.type"), Some(disk.disk_type.trim().to_string())),
        StatRow::text(t("disk.filesystem"), Some(disk.fs_type.trim().to_string())),
    ];
    if let Some(serial) = disk.serial.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatRow::text(t("disk.serial"), Some(serial.to_owned())));
    }
    if let Some(revision) = disk.revision.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatRow::text(t("disk.revision"), Some(revision.to_owned())));
    }
    // ── NVMe / SMART health (only when the kernel exposes a health node) ──
    // The critical-warning prefix surfaces the most actionable SMART bit the
    // hwmon layer carries; otherwise a plain temperature readout.
    if let Some(temp) = disk.smart_temperature_c {
        let warn = disk.smart_critical_warning == Some(true);
        let label = if warn {
            format!("{} \u{26a0}", t("common.temperature"))
        } else {
            t("common.temperature").to_string()
        };
        let val = match disk.smart_temp_critical_c {
            Some(crit) if crit > 0.0 => format!("{:.0} / {:.0} \u{b0}C", temp, crit),
            _ => format!("{:.0} \u{b0}C", temp),
        };
        rows.push(StatRow::text(label, Some(val)));
        // SMART temperature trend from this disk identity's generation-scoped
        // window. Only a window with at least one finite sample renders a row;
        // another disk can never influence it.
        if let Some(trend) = temperature_trend_value(temperature_samples) {
            rows.push(StatRow::text(t("proc.trend"), Some(trend)));
        }
    }
    if let Some(pct) = disk.smart_percent_used {
        rows.push(StatRow::text(
            t("disk.endurance_used"),
            Some(format!("{pct:.0}%")),
        ));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        let days = hours / 24;
        rows.push(StatRow::text(
            t("disk.power_on"),
            Some(
                t("disk.power_on_format")
                    .replace("{hours}", &hours.to_string())
                    .replace("{days}", &days.to_string()),
            ),
        ));
    }
    if smart_section_visible(disk) && !has_smart_fields(disk) {
        rows.push(StatRow::text(
            t("disk.smart_status"),
            Some(t(device_status_i18n_key(effective_smart_status(disk))).to_string()),
        ));
    }
    if disk.media_removable() == Some(true) {
        rows.push(StatRow::text(
            t("disk.removable"),
            Some(t("common.yes").to_string()),
        ));
    }
    // The partition census lives ONCE, in the vital line (GPUI parity): the
    // panel below carries per-partition usage; the stats rail never
    // duplicates it.
    rows
}

/// Latest/average/peak summary of one disk's SMART temperature window (°C) —
/// GPUI `temperature_trend_value` parity. `None` when the window holds no
/// finite sample: the honest absence renders no row, never a fabricated
/// "0 °C" trend.
#[must_use]
pub(crate) fn temperature_trend_value(samples: &[f32]) -> Option<String> {
    let mut latest = f32::NAN;
    let mut peak = f32::NAN;
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for &value in samples.iter().filter(|value| value.is_finite()) {
        latest = value;
        // `f32::max` ignores NaN operands, so the first sample seeds `peak`.
        peak = peak.max(value);
        sum += value;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(format!(
        "{} {:.0} \u{b0}C · {} {:.0} \u{b0}C · {} {:.0} \u{b0}C",
        t("common.latest"),
        latest,
        t("common.avg"),
        sum / count as f32,
        t("common.peak"),
        peak,
    ))
}

/// The Performance-page per-disk panel.
pub(crate) fn disk_section(
    app: &crate::IcedApp,
    index: usize,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let theme_snapshot = app.theme();
    let color = crate::theme_binding::color(theme_snapshot.disk);
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
    let mut disk_graph = app.graph_prefs();
    disk_graph.hover = true;
    let rows = match (disk_section_state(snapshot), snapshot) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("disk.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.disks.get(index) {
            Some(disk) => vec![disk_block(
                app,
                disk,
                index,
                disk_graphs(
                    app,
                    disk,
                    index,
                    color,
                    theme_snapshot,
                    disk_graph,
                    compact,
                    budget,
                ),
                smart_footer(app, disk, index, theme_snapshot),
                theme_snapshot,
                compact,
                budget,
            )],
            None => vec![tables::message_panel(theme_snapshot, t("disk.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

/// The SMART footer pinned under the statistics rail (GPUI parity): the
/// health button when the disk exposes SMART fields, the honest status
/// footer when the section is visible but fields are absent, and nothing
/// when the section is hidden.
fn smart_footer<'a>(
    _app: &crate::IcedApp,
    disk: &DiskMetrics,
    index: usize,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    if !smart_section_visible(disk) {
        return None;
    }
    if has_smart_fields(disk) {
        Some(crate::ui::focus::ghost_button(
            theme_snapshot,
            crate::app::FocusTarget::DiskSmartOpen { index },
            t("disk.smart_health"),
            Message::OpenDiskSmart { index },
        ))
    } else {
        super::device_status_footer(theme_snapshot, effective_smart_status(disk))
    }
}

#[allow(clippy::too_many_arguments)]
fn disk_graphs<'a>(
    app: &'a crate::IcedApp,
    disk: &'a DiskMetrics,
    index: usize,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    graph: device_chart::GraphPrefs,
    compact: bool,
    budget: PerformancePageBudget,
) -> Vec<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let _ = index;
    let (read_samples, write_samples) =
        app.cached_disk_split_series(&disk.device_id, disk.device_generation.get());
    let activity_samples =
        app.cached_disk_active_time_series(&disk.device_id, disk.device_generation.get());
    let mut graphs = vec![
        // Two-series read/write graph: the disk family token strokes read, the
        // same token lifted toward white strokes write, both resolved through
        // one shared slot grid and one shared max so the directions stay
        // directly comparable; each direction keeps its own gap evidence. The
        // canvas takes the remaining column height on a wide card and the
        // fixed primary height inside a compact scrollable.
        device_chart::multi::device_multi_graph_fill(
            device_chart::multi::DeviceMultiGraphSpec {
                primary: disk_split_series(t("disk.read").to_string(), read_samples),
                secondary: disk_split_series(t("disk.write").to_string(), write_samples),
                family_color: color,
                capacity: app.graph_data_points(),
                format_value: drive_throughput_formatter(app.drive_units()),
                prefs: graph,
            },
            t("disk.throughput").to_string(),
            theme_snapshot,
            compact,
        ),
    ];
    // Active-time percentage curve beneath the throughput pair: rendered only
    // when the ring holds samples AND the Full chart inventory keeps
    // secondary charts (GPUI parity) — a cold window or an AggregateOnly
    // frame keeps the absence instead of a captioned empty card.
    if !activity_samples.is_empty() && budget.chart_inventory == PerformanceChartInventory::Full {
        graphs.push(device_chart::device_mini_graph_with_height(
            activity_samples,
            device_chart::DeviceMetricScale::Percent,
            color,
            t("disk.active_time").to_string(),
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            graph,
        ));
    }
    // The below band renders from the Core rung up (GPUI parity): directory
    // usage and the partition panel yield first when height collapses.
    if budget.vertical.carries_core_stack() {
        graphs.push(directory_usage::usage_panel(app, disk));
        if let Some(panel) = partition_panel(disk, theme_snapshot, app.drive_units()) {
            graphs.push(panel);
        }
    }
    graphs
}

/// One legend-labeled series of the disk's two-series graph. The stroke color
/// is derived from the family token inside the chart factory; `Color::WHITE`
/// here is the placeholder the factory contract overwrites.
fn disk_split_series(label: String, samples: Rc<[f32]>) -> device_chart::multi::DeviceMultiSeries {
    device_chart::multi::DeviceMultiSeries {
        samples,
        label,
        color: iced::Color::WHITE,
    }
}

/// The injected unit formatter for the two-series disk graph's y-axis ticks
/// and hover pill: the same `throughput_scale`/`summary_value` authority the
/// single-series graphs and scalar rows use, resolved to a plain `fn` pointer
/// for the resolved drive unit pair.
fn drive_throughput_formatter(units: UnitPrefs) -> fn(f32) -> String {
    fn pair(use_bytes: bool, use_base2: bool, value: f32) -> String {
        device_chart::summary_value(
            throughput_scale(UnitPrefs {
                use_bytes,
                use_base2,
            }),
            value,
        )
    }
    match (units.use_bytes, units.use_base2) {
        (true, true) => |value| pair(true, true, value),
        (true, false) => |value| pair(true, false, value),
        (false, true) => |value| pair(false, true, value),
        (false, false) => |value| pair(false, false, value),
    }
}

/// The per-partition filesystem-space panel (GPUI `partition_stats` parity):
/// a titled card that ALWAYS renders for a disk — an empty partition list
/// shows the explicit "no partitions" line, mounted partitions get the full
/// identity row + usage row + 6px family-colored bar, and unmounted
/// partitions collapse into one dim summary line because they have no
/// trustworthy free/used numbers.
pub(crate) fn partition_panel<'a>(
    disk: &'a DiskMetrics,
    theme_snapshot: &'a taskmanager_theme::Theme,
    units: UnitPrefs,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        Vec::with_capacity(disk.partitions.len() + 2);
    rows.push(
        iced::widget::text(t("disk.partitions"))
            .size(f32::from(tokens::FONT_13))
            .style(move |_| iced::widget::text::Style {
                color: Some(theme::muted_text_color(theme_snapshot)),
            })
            .into(),
    );
    if disk.partitions.is_empty() {
        rows.push(
            iced::widget::text(t("disk.no_partitions"))
                .size(f32::from(tokens::FONT_12))
                .style(move |_| iced::widget::text::Style {
                    color: Some(theme::muted_text_color(theme_snapshot)),
                })
                .into(),
        );
    } else {
        let observed = super::projection::DiskObservation::from(disk);
        let mut mounted_names = Vec::new();
        for (partition, partition_observation) in disk.partitions.iter().zip(&observed.partitions) {
            if partition.mount_point.trim().is_empty() {
                mounted_names.push(
                    partition
                        .name
                        .trim_start_matches("/dev/")
                        .trim()
                        .to_string(),
                );
                continue;
            }
            rows.push(partition_row(
                partition,
                partition_observation,
                theme_snapshot,
                units,
            ));
        }
        if !mounted_names.is_empty() {
            let names = mounted_names
                .into_iter()
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            rows.push(
                iced::widget::text(t("disk.unmounted_summary").replace("{names}", &names))
                    .size(f32::from(tokens::FONT_11))
                    .style(move |_| iced::widget::text::Style {
                        color: Some(theme::muted_text_color(theme_snapshot)),
                    })
                    .into(),
            );
        }
    }

    Some(
        iced::widget::container(iced::widget::column(rows).spacing(6))
            .padding(8)
            .style(move |_| theme::panel_style(theme_snapshot))
            .into(),
    )
}

/// One mounted partition's identity row + usage row + 6px family bar (GPUI
/// `partition_row` parity).
#[must_use]
pub(crate) fn partition_usage_text(
    used_bytes: Option<u64>,
    capacity_bytes: Option<u64>,
    free_bytes: Option<u64>,
    status: DeviceStatus,
    units: UnitPrefs,
) -> (String, Option<f32>) {
    match (used_bytes, capacity_bytes, free_bytes) {
        (Some(used), Some(total), Some(free)) if total > 0 => {
            let used = used.min(total);
            let ratio = (used as f32 / total as f32).clamp(0.0, 1.0);
            (
                format!(
                    "{} / {} · {} {} · {:.0}%",
                    quantity_text_pref(used, units.use_bytes, units.use_base2),
                    quantity_text_pref(total, units.use_bytes, units.use_base2),
                    t("disk.free"),
                    quantity_text_pref(free, units.use_bytes, units.use_base2),
                    ratio * 100.0,
                ),
                Some(ratio),
            )
        }
        _ => (
            if status == DeviceStatus::Healthy {
                t("disk.usage_unavailable").to_string()
            } else {
                t(device_status_i18n_key(status)).to_string()
            },
            None,
        ),
    }
}

fn partition_row<'a>(
    partition: &taskmanager_core::core::metrics::DiskPartition,
    observed: &super::projection::PartitionObservation,
    theme_snapshot: &'a taskmanager_theme::Theme,
    units: UnitPrefs,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let label = partition_label(partition);
    // Usage text: "used / total · free · pct"; unavailable facts name the
    // reason instead of an empty bar with numbers that do not exist.
    let usage = partition_usage_text(
        observed.used_bytes,
        observed.capacity_bytes,
        observed.free_bytes,
        partition.device_state.status,
        units,
    );

    // 6px family-colored progress bar (FillPortion siblings keep the measured
    // fraction visible on Iced).
    let bar_fill_color = crate::theme_binding::color(theme_snapshot.disk);
    let bar_bg = crate::theme_binding::color(theme_snapshot.shade);
    let progress_bar_content = match usage.1 {
        Some(value) => {
            let fill_portion = ((value * 1000.0).round() as u16).clamp(1, 1000);
            let remainder_portion = 1000_u16.saturating_sub(fill_portion);
            let fill = iced::widget::container(iced::widget::text("").size(0))
                .width(iced::Length::FillPortion(fill_portion))
                .height(iced::Length::Fixed(6.0))
                .style(move |_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(bar_fill_color)),
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            let remainder = iced::widget::container(iced::widget::text("").size(0))
                .width(if remainder_portion > 0 {
                    iced::Length::FillPortion(remainder_portion)
                } else {
                    iced::Length::Shrink
                })
                .height(iced::Length::Fixed(6.0));
            iced::widget::row![fill, remainder]
                .width(iced::Length::Fill)
                .height(iced::Length::Fixed(6.0))
        }
        None => iced::widget::row![]
            .width(iced::Length::Fill)
            .height(iced::Length::Fixed(6.0)),
    };
    let progress_bar = iced::widget::container(progress_bar_content)
        .width(iced::Length::Fill)
        .height(iced::Length::Fixed(6.0))
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(bar_bg)),
            border: iced::Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    iced::widget::column![
        iced::widget::text(label).size(f32::from(tokens::FONT_12)),
        iced::widget::text(usage.0)
            .size(f32::from(tokens::FONT_11))
            .style(move |_| iced::widget::text::Style {
                color: Some(theme::muted_text_color(theme_snapshot)),
            }),
        progress_bar,
    ]
    .spacing(4)
    .width(iced::Length::Fill)
    .into()
}

/// One partition's identity: `name · mount · fs` with `/dev/` handling and
/// the unmounted qualifier (GPUI `partition_label` parity).
fn partition_label(partition: &DiskPartition) -> String {
    let raw_name = partition.name.trim();
    let raw_mount = partition.mount_point.trim();
    let is_windows_path = raw_name.contains(':')
        || raw_name.starts_with('\\')
        || raw_mount.contains(':')
        || raw_mount.starts_with('\\')
        || cfg!(target_os = "windows");
    let name = raw_name.trim_start_matches("/dev/");
    let mount = raw_mount.trim_start_matches("/dev/");
    let prefix = if !is_windows_path && !name.starts_with('/') && !name.is_empty() {
        "/dev/"
    } else {
        ""
    };

    if mount.is_empty() {
        format!("{prefix}{name} · {}", t("disk.unmounted"))
    } else if partition.fs_type.is_empty() {
        if name.eq_ignore_ascii_case(mount) || name.is_empty() {
            mount.to_string()
        } else {
            format!("{prefix}{name} · {mount}")
        }
    } else if name.eq_ignore_ascii_case(mount) || name.is_empty() {
        format!("{mount} · {}", partition.fs_type)
    } else {
        format!("{prefix}{name} · {mount} · {}", partition.fs_type)
    }
}

#[allow(clippy::too_many_arguments)]
fn disk_block<'a>(
    app: &'a crate::IcedApp,
    disk: &'a DiskMetrics,
    _index: usize,
    graphs: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
    stats_footer: Option<Element<'a, Message, iced::Theme, iced::Renderer>>,
    theme_snapshot: &'a taskmanager_theme::Theme,
    _compact: bool,
    budget: PerformancePageBudget,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let temperature_samples =
        app.cached_disk_temperature_series(&disk.device_id, disk.device_generation.get());
    perf_layout::main_with_stats(
        theme_snapshot,
        disk_title(disk),
        format!(
            "{} · {} · {}",
            disk.name.trim_start_matches("/dev/"),
            disk.disk_type,
            disk.fs_type
        ),
        Some(disk_vital_line(disk, app.drive_units())),
        graphs,
        disk_summary_lines(
            disk,
            app.drive_units().use_bytes,
            app.drive_units().use_base2,
            &temperature_samples,
        ),
        stats_footer,
        budget,
        perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation),
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/perf_devices/disk_split_chart_tests.rs"]
mod split_chart_tests;
