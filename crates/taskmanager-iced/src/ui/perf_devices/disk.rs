//! Performance page Disk detail block, throughput chart and partition summary.

use std::rc::Rc;

use super::*;
use iced::Element;
use taskmanager_application::{DiskMetrics, SystemSnapshot};
use taskmanager_shell::presentation::{
    MISSING_VALUE, device_status_i18n_key, effective_smart_status, has_smart_fields, missing_value,
    smart_section_visible,
};
use taskmanager_theme::tokens;

/// The Performance-page per-disk panel readiness.
#[must_use]
pub(crate) fn disk_section_state(snapshot: Option<&SystemSnapshot>) -> tables::ListState {
    match snapshot {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.disks.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One disk's display identity.
#[must_use]
pub(crate) fn disk_title(disk: &DiskMetrics) -> String {
    let name = (!disk.name.trim().is_empty())
        .then(|| disk.name.trim().to_string())
        .or_else(|| (!disk.model.trim().is_empty()).then(|| disk.model.trim().to_string()));
    match name {
        Some(name) => format!("{}: {name}", t("common.disk")),
        None => t("common.disk").to_string(),
    }
}

/// Build the disk stat readout rows for the Performance device page.
#[must_use]
pub(crate) fn disk_summary_lines(
    disk: &DiskMetrics,
    use_bytes: bool,
    use_base2: bool,
) -> Vec<(String, String)> {
    let observed = super::projection::DiskObservation::from(disk);
    let mut rows = vec![
        (
            t("device.status").to_string(),
            t(device_status_i18n_key(disk.device_state.status)).to_string(),
        ),
        (
            t("disk.read").to_string(),
            rate_text_pref(observed.read_bytes_per_sec, use_bytes, use_base2),
        ),
        (
            t("disk.write").to_string(),
            rate_text_pref(observed.write_bytes_per_sec, use_bytes, use_base2),
        ),
        (
            t("disk.active_time").to_string(),
            observed
                .active_time_pct
                .map_or_else(missing_value, |value| format!("{:.0}%", value.round())),
        ),
    ];

    if let Some(ms) = observed.response_time_ms {
        rows.push((t("disk.response").to_string(), format!("{ms:.2} ms")));
    }
    if let Some(iops) = observed.iops {
        rows.push((t("disk.iops").to_string(), iops.to_string()));
    }
    if let Some(total) = observed.capacity_bytes {
        rows.push((
            t("disk.capacity").to_string(),
            quantity_text_pref(total, use_bytes, use_base2),
        ));
    }
    if let Some(free) = observed.available_bytes {
        rows.push((
            t("disk.free").to_string(),
            quantity_text_pref(free, use_bytes, use_base2),
        ));
    }
    if !disk.disk_type.trim().is_empty() {
        rows.push((
            t("common.type").to_string(),
            disk.disk_type.trim().to_string(),
        ));
    }
    if let Some(serial) = disk.serial.as_deref().filter(|value| !value.is_empty()) {
        rows.push((t("disk.serial").to_string(), serial.to_owned()));
    }
    if let Some(revision) = disk.revision.as_deref().filter(|value| !value.is_empty()) {
        rows.push((t("disk.revision").to_string(), revision.to_owned()));
    }
    if !disk.fs_type.trim().is_empty() {
        rows.push((
            t("disk.filesystem").to_string(),
            disk.fs_type.trim().to_string(),
        ));
    }

    if let Some(temp) = disk.smart_temperature_c {
        let warn = disk.smart_critical_warning == Some(true);
        let label = if warn {
            format!("{} ⚠", t("common.temperature"))
        } else {
            t("common.temperature").to_string()
        };
        let value = match disk.smart_temp_critical_c {
            Some(crit) if crit > 0.0 => format!("{temp:.0} / {crit:.0} °C"),
            _ => format!("{temp:.0} °C"),
        };
        rows.push((label, value));
    }
    if let Some(pct) = disk.smart_percent_used {
        rows.push((t("disk.endurance_used").to_string(), format!("{pct:.0}%")));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        rows.push((
            t("disk.power_on").to_string(),
            format!("{} h ({} d)", hours, hours / 24),
        ));
    }
    if smart_section_visible(disk) && !has_smart_fields(disk) {
        rows.push((
            t("disk.smart_status").to_string(),
            t(device_status_i18n_key(effective_smart_status(disk))).to_string(),
        ));
    }
    if disk.media_removable() == Some(true) {
        rows.push((t("disk.removable").to_string(), t("common.yes").to_string()));
    }

    for (partition, partition_observation) in disk.partitions.iter().zip(&observed.partitions) {
        let heading = if !partition.mount_point.is_empty() {
            partition.mount_point.clone()
        } else if !partition.name.is_empty() {
            format!("/dev/{}", partition.name)
        } else {
            continue;
        };
        let value = match (
            partition_observation.used_bytes,
            partition_observation.capacity_bytes,
        ) {
            (Some(used), Some(total)) if total > 0 => format!(
                "{} / {}",
                quantity_text_pref(used, use_bytes, use_base2),
                quantity_text_pref(total, use_bytes, use_base2)
            ),
            (None, Some(total)) => {
                format!(
                    "{MISSING_VALUE} / {}",
                    quantity_text_pref(total, use_bytes, use_base2)
                )
            }
            (Some(used), _) => quantity_text_pref(used, use_bytes, use_base2),
            _ => missing_value(),
        };
        rows.push((format!("{} · {}", t("disk.partitions"), heading), value));
    }
    rows
}

/// The Performance-page per-disk panel.
pub(crate) fn disk_section(
    app: &crate::IcedApp,
    index: usize,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let theme_snapshot = app.theme();
    let color = theme::color(theme_snapshot.disk);
    let compact = app.compact_layout();
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
                disk,
                disk_graphs(app, index, disk, color, theme_snapshot, disk_graph, compact),
                theme_snapshot,
                compact,
                app.drive_units(),
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

fn disk_graphs<'a>(
    app: &'a crate::IcedApp,
    index: usize,
    disk: &'a DiskMetrics,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    graph: device_chart::GraphPrefs,
    compact: bool,
) -> Vec<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let (read_samples, write_samples) =
        app.cached_disk_split_series(&disk.device_id, disk.device_generation.get());
    let smart_button = (smart_section_visible(disk) && has_smart_fields(disk)).then(|| {
        crate::ui::focus::ghost_button(
            theme_snapshot,
            crate::app::FocusTarget::DiskSmartOpen { index },
            t("disk.smart_health"),
            Message::OpenDiskSmart { index },
        )
    });
    let mut graphs = Vec::with_capacity(4);
    if let Some(smart_button) = smart_button {
        graphs.push(
            iced::widget::row![smart_button]
                .spacing(f32::from(tokens::SPACE_8))
                .into(),
        );
    }
    graphs.extend([
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
        // Active-time percentage curve beneath the throughput pair: the same
        // generation-scoped ring the recorder feeds, on the fixed 0..100
        // ceiling every percentage series shares (no selector, no second
        // scale authority). The caption keeps the honest "· collecting" state
        // while the window holds fewer than two samples.
        device_chart::device_mini_graph_with_height(
            app.cached_disk_active_time_series(&disk.device_id, disk.device_generation.get()),
            device_chart::DeviceMetricScale::Percent,
            color,
            t("disk.active_time").to_string(),
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            graph,
        ),
        directory_usage::usage_panel(app, disk),
    ]);
    if let Some(panel) = partition_panel(disk, theme_snapshot, app.drive_units()) {
        graphs.push(panel);
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

/// Dedicated visual cards for disk partitions with progress bars and filesystem tags.
pub(crate) fn partition_panel<'a>(
    disk: &'a DiskMetrics,
    theme_snapshot: &'a taskmanager_theme::Theme,
    units: UnitPrefs,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    if disk.partitions.is_empty() {
        return None;
    }
    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        Vec::with_capacity(disk.partitions.len() + 1);
    rows.push(
        iced::widget::text(t("disk.partitions"))
            .size(f32::from(tokens::FONT_13))
            .style(move |_| iced::widget::text::Style {
                color: Some(theme::muted_text_color(theme_snapshot)),
            })
            .into(),
    );

    let accent_color = theme::color(theme_snapshot.palette().accent);
    let danger_color = theme::color(theme_snapshot.palette().danger);
    let bar_bg = theme::color(theme_snapshot.shade);

    let observed = super::projection::DiskObservation::from(disk);
    for (partition, partition_observation) in disk.partitions.iter().zip(&observed.partitions) {
        let name = if !partition.mount_point.is_empty() {
            partition.mount_point.as_str()
        } else if !partition.name.is_empty() {
            partition.name.as_str()
        } else {
            "Partition"
        };
        let fs = if !partition.fs_type.is_empty() {
            partition.fs_type.as_str()
        } else {
            ""
        };

        let used = partition_observation.used_bytes;
        let total = partition_observation.capacity_bytes;
        let free = partition_observation.free_bytes;
        let pct = match (used, total) {
            (Some(used), Some(total)) if total > 0 => {
                Some((used as f32 / total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        };

        let used_str = used.map_or_else(missing_value, |value| {
            quantity_text_pref(value, units.use_bytes, units.use_base2)
        });
        let total_str = total.map_or_else(missing_value, |value| {
            quantity_text_pref(value, units.use_bytes, units.use_base2)
        });
        let free_str = free.map_or_else(missing_value, |value| {
            quantity_text_pref(value, units.use_bytes, units.use_base2)
        });

        let bar_fill_color = if pct.is_some_and(|value| value > 0.90) {
            danger_color
        } else {
            accent_color
        };

        let header_row = iced::widget::row![
            iced::widget::text(name)
                .size(f32::from(tokens::FONT_12))
                .width(iced::Length::Fill)
                .wrapping(iced::widget::text::Wrapping::Glyph),
            iced::widget::container(iced::widget::text(fs).size(f32::from(tokens::FONT_10)))
                .padding([1, 4])
                .style(move |_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(bar_bg)),
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: 1.0,
                        color: theme::color(theme_snapshot.palette().border),
                    },
                    ..Default::default()
                }),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .width(iced::Length::Fill);

        // `FillPortion` is resolved by a flex parent. Keeping the fill and
        // the remainder as siblings makes the measured used fraction visible;
        // putting a single `FillPortion` inside a container can collapse it
        // to the child's intrinsic zero width on Iced.
        let progress_bar_content = match pct {
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

        let stats_row = iced::widget::row![
            iced::widget::text(format!(
                "{used_str} / {total_str} ({})",
                pct.map_or_else(missing_value, |value| format!("{:.1}%", value * 100.0))
            ))
            .size(f32::from(tokens::FONT_11)),
            iced::widget::text(format!("{} {free_str}", t("disk.free")))
                .size(f32::from(tokens::FONT_11))
                .style(move |_| iced::widget::text::Style {
                    color: Some(theme::muted_text_color(theme_snapshot)),
                }),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let card = iced::widget::container(
            iced::widget::column![header_row, progress_bar, stats_row].spacing(4),
        )
        .padding(6)
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(theme::color(
                theme_snapshot.palette().surface,
            ))),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: theme::color(theme_snapshot.palette().border),
            },
            ..Default::default()
        });

        rows.push(card.into());
    }

    Some(
        iced::widget::container(iced::widget::column(rows).spacing(6))
            .padding(8)
            .style(move |_| theme::panel_style(theme_snapshot))
            .into(),
    )
}

fn disk_block<'a>(
    disk: &'a DiskMetrics,
    graphs: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    units: UnitPrefs,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    perf_layout::main_with_stats(
        theme_snapshot,
        disk_title(disk),
        t("disk.throughput").to_string(),
        graphs,
        disk_summary_lines(disk, units.use_bytes, units.use_base2),
        compact,
        perf_layout::DetailExtent::for_scroll_parent(compact),
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/perf_devices/disk_split_chart_tests.rs"]
mod split_chart_tests;
