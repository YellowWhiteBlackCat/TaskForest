//! Per-partition filesystem-space panel for the physical disk page.

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px, relative};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use taskmanager_application::i18n;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::DiskPartition;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::presentation::device_status_i18n_key;
use taskmanager_theme::Theme;
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui::primitives::tooltip::{Tooltip, TooltipHost};

mod stats;
use stats::{PartitionUsage, partition_usage};
use taskmanager_theme::tokens;

/// The Performance main viewport never scrolls. Keep the partition detail
/// bounded so a device with a large partition table cannot consume the
/// headline's frame or leave a half-row at the bottom.
pub(super) const MAX_VISIBLE_MOUNTED_PARTITIONS: usize = 4;

pub(super) fn partition_panel(
    theme: &Theme,
    partitions: &[DiskPartition],
    units: UnitPreferences,
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
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(taskmanager_ui::icons_binding::icon(IconId::Disk).size(px(14.0)))
                .child(i18n::t("disk.partitions")),
        )
        .render()
        .flex()
        .flex_col()
        .w_full()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ));
    #[cfg(any(test, feature = "test-support"))]
    {
        panel = panel.debug_selector(|| "tm-disk-partitions".to_string());
    }

    if partitions.is_empty() {
        return panel.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
    for (index, partition) in mounted
        .iter()
        .take(MAX_VISIBLE_MOUNTED_PARTITIONS)
        .enumerate()
    {
        panel = panel.child(with_partition_selector(
            partition_row(theme, partition, units, index),
            index,
        ));
    }
    if mounted.len() > MAX_VISIBLE_MOUNTED_PARTITIONS {
        panel = panel.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("disk.partitions_more").replace(
                    "{count}",
                    &(mounted.len() - MAX_VISIBLE_MOUNTED_PARTITIONS).to_string(),
                )),
        );
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
    units: UnitPreferences,
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
                    units.format_quantity_pair(used, total, QuantityFamily::Drive, false),
                    units.format_quantity(free, QuantityFamily::Drive, false),
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
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::small_radius(theme),
        ))
        .overflow_hidden()
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_bg));
    if let Some(fraction) = usage.1 {
        bar = bar.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(relative(fraction))
                .bg(taskmanager_ui::theme_binding::fill(theme.disk)),
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
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg)),
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
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim)),
        ),
        index,
        "usage",
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_5,
        ))
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(
            elements::truncated_text(&summary)
                .flex_1()
                .min_w(px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim)),
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
