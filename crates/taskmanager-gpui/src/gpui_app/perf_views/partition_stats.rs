//! Per-partition filesystem-space panel for the physical disk page.

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px, relative};
use taskmanager_ui_contract::IconId;

use crate::core::device_state::DeviceStatus;
use crate::core::metrics::DiskPartition;
use crate::gpui_app::elements;
use crate::gpui_app::formatting::{DisplayUnits, UnitKind};
use crate::gpui_app::icons;
use crate::gpui_app::perf_views::device_status_i18n_key;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui::primitives::tooltip::{Tooltip, TooltipHost};

mod stats;
use stats::{PartitionUsage, partition_usage};

pub(super) fn partition_panel(
    theme: &Theme,
    partitions: &[DiskPartition],
    units: DisplayUnits,
) -> Div {
    let mut panel = CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_10)
        .radius(tokens::control_radius(theme))
        .bordered(true)
        .child(
            div()
                .flex()
                .items_center()
                .gap(tokens::SPACE_6)
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(theme.fg)
                .child(icons::icon(IconId::Disk).size(px(14.0)))
                .child(i18n::t("disk.partitions")),
        )
        .render()
        .flex()
        .flex_col()
        .w_full()
        .gap(tokens::SPACE_8);
    #[cfg(any(test, feature = "test-support"))]
    {
        panel = panel.debug_selector(|| "tm-disk-partitions".to_string());
    }

    if partitions.is_empty() {
        return panel.child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("disk.no_partitions")),
        );
    }

    // Mounted partitions get the full usage row + progress bar. Unmounted
    // partitions have no trustworthy free/used numbers, so they collapse into
    // one compact dim summary line instead of occupying a full row each —
    // identical on every platform (a Windows/macOS host simply has none).
    let mounted: Vec<&DiskPartition> = partitions
        .iter()
        .filter(|partition| !partition.mount_point.trim().is_empty())
        .collect();
    let unmounted: Vec<&DiskPartition> = partitions
        .iter()
        .filter(|partition| partition.mount_point.trim().is_empty())
        .collect();
    for (index, partition) in mounted.iter().enumerate() {
        panel = panel.child(with_partition_selector(
            partition_row(theme, partition, units, index),
            index,
        ));
    }
    if !unmounted.is_empty() {
        panel = panel.child(unmounted_summary(theme, &unmounted));
    }
    panel
}

#[cfg(any(test, feature = "test-support"))]
fn with_partition_selector(row: Div, index: usize) -> Div {
    row.debug_selector(move || format!("tm-disk-partition:{index}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn with_partition_selector(row: Div, _index: usize) -> Div {
    row
}

fn partition_row(
    theme: &Theme,
    partition: &DiskPartition,
    units: DisplayUnits,
    index: usize,
) -> Div {
    let label = partition_label(partition);
    let usage = match partition_usage(partition) {
        PartitionUsage::Current { used, free, total } if total > 0 => {
            let used = used.min(total);
            let ratio = (used as f64 / total as f64).clamp(0.0, 1.0);
            let percent = ratio * 100.0;
            (
                format!(
                    "{}  ·  {} {}  ·  {:.0}%",
                    units.format_pair(used, total, UnitKind::Drive, false),
                    units.format(free, UnitKind::Drive, false),
                    i18n::t("disk.free"),
                    percent
                ),
                Some(ratio as f32),
            )
        }
        PartitionUsage::Current { .. } | PartitionUsage::Unavailable(_) => (
            partition_unavailable_text(partition.device_state.status),
            None,
        ),
    };

    let mut bar = div()
        .relative()
        .flex()
        .flex_row()
        .w_full()
        .h(px(6.0))
        .rounded(tokens::small_radius(theme))
        .overflow_hidden()
        .bg(theme.sidebar_bg);
    if let Some(fraction) = usage.1 {
        bar = bar.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(relative(fraction))
                .bg(theme.disk),
        );
    }
    let bar = partition_slot(bar, index, "bar");

    // Keep identity and capacity in separate rows. A single horizontal row
    // makes a long mount point compete directly with the used/free readout;
    // the result is either unreadable text or a squeezed percentage. The
    // identity remains elastic/ellipsized, while the capacity row gets the
    // whole width and the bar below it always stays visually independent.
    let label_text = div().w_full().min_w(px(0.0)).child(
        elements::truncated_text(&label)
            .w_full()
            .text_size(tokens::FONT_12)
            .text_color(theme.fg),
    );
    let label_row = partition_slot(
        div().flex().w_full().min_w(px(0.0)).child(
            div().flex_1().min_w(px(0.0)).child(
                TooltipHost::new(("disk-partition-label-tooltip", index), label_text)
                    .tooltip(Tooltip::text(label.clone(), theme.palette())),
            ),
        ),
        index,
        "label",
    );
    let usage_row = partition_slot(
        div().flex().w_full().min_w(px(0.0)).child(
            elements::truncated_text(&usage.0)
                .flex_1()
                .min_w(px(0.0))
                .text_right()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim),
        ),
        index,
        "usage",
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(tokens::SPACE_5)
        .child(label_row)
        .child(usage_row)
        .child(bar)
}

#[cfg(any(test, feature = "test-support"))]
fn partition_slot(row: Div, index: usize, kind: &'static str) -> Div {
    row.debug_selector(move || format!("tm-disk-partition-{kind}:{index}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn partition_slot(row: Div, _index: usize, _kind: &'static str) -> Div {
    row
}

/// One compact dim line for every unmounted partition, e.g.
/// `未挂载：nvme0n1p3 · nvme0n1p4`. Unmounted partitions have no trusted
/// used/free observations, so a full row with an empty bar would only waste
/// vertical space; the names stay visible for identification.
fn unmounted_summary(theme: &Theme, partitions: &[&DiskPartition]) -> Div {
    let summary =
        i18n::t("disk.unmounted_summary").replace("{names}", &unmounted_names(partitions));
    let row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(tokens::SPACE_8)
        .child(
            elements::truncated_text(&summary)
                .flex_1()
                .min_w(px(0.0))
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim),
        );
    #[cfg(any(test, feature = "test-support"))]
    let row = row.debug_selector(|| "tm-disk-partitions-unmounted".to_string());
    row
}

/// Comma-free, locale-neutral name list for the unmounted summary.
fn unmounted_names(partitions: &[&DiskPartition]) -> String {
    partitions
        .iter()
        .map(|partition| partition.name.trim_start_matches("/dev/"))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn partition_unavailable_text(status: DeviceStatus) -> String {
    if status == DeviceStatus::Healthy {
        i18n::t("disk.usage_unavailable").to_string()
    } else {
        i18n::t(device_status_i18n_key(status)).to_string()
    }
}

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
        format!("{prefix}{name} · {}", i18n::t("disk.unmounted"))
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

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_partition_stats_tests.rs"]
mod tests;
