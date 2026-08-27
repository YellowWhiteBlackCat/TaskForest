//! Directory usage breakdown panel.
//!
//! State machine: directory-usage scans execute off-thread. The latest snapshot is
//! owned by `RootView` (`RootView::directory_usage`): it renders the typed
//! status, live scan totals, and the directory tree under the selected disk.

use gpui::{
    Context, Div, InteractiveElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use taskmanager_ui_contract::IconId;

use crate::core::DirectoryScanStatus;
use crate::core::DirectoryUsageEntry;
use crate::core::DirectoryUsageSnapshot;
use crate::core::metrics::{DiskMetrics, ScalarAvailability};
use crate::gpui_app::elements;
use crate::gpui_app::formatting::{DisplayUnits, UnitKind, missing_value};
use crate::gpui_app::icons;
use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens;
use crate::i18n;
use taskmanager_ui::primitives::card_surface::CardSurface;

/// Maximum entry rows painted before collapsing into a "+N" summary. The
/// snapshot itself is bounded (`max_reported`), but the left column under the
/// disk graph is not scrollable, so the panel keeps a compact projection;
/// drill-down reaches the rest.
const MAX_VISIBLE_ENTRIES: usize = 8;

/// Depth indent per nesting level, in device pixels.
const DEPTH_INDENT: f32 = 10.0;

pub(super) fn directory_usage_panel(
    theme: &Theme,
    d: &DiskMetrics,
    state: Option<&DirectoryUsageSnapshot>,
    units: DisplayUnits,
    cx: &mut Context<RootView>,
) -> Div {
    let ent = cx.entity();
    let mut panel = CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_10)
        .radius(tokens::control_radius(theme))
        .bordered(true)
        .render()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8);
    #[cfg(any(test, feature = "test-support"))]
    {
        panel = panel.debug_selector(|| "tm-disk-usage-panel".to_string());
    }

    let own = state.filter(|snapshot| snapshot_targets_disk(snapshot, d));

    // ── Header: title + typed scan status ────────────────────────────────────
    let mut header_right = div();
    if let Some(snapshot) = own {
        header_right = header_right
            .text_size(tokens::FONT_11)
            .text_color(theme.fg_dim)
            .child(status_text(snapshot));
    }
    panel = panel.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(tokens::SPACE_6)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(tokens::SPACE_6)
                    .text_size(tokens::FONT_13)
                    .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                    .text_color(theme.fg)
                    .child(icons::icon(IconId::Search).size(px(14.0)))
                    .child(i18n::t("disk.usage_analysis")),
            )
            .child(header_right),
    );

    // ── Scan targets: one pill per mounted partition ─────────────────────────
    let mounts: Vec<(usize, String)> = d
        .partitions
        .iter()
        .enumerate()
        .filter(|(_, partition)| !partition.mount_point.is_empty())
        .map(|(index, partition)| (index, partition.mount_point.clone()))
        .collect();
    if mounts.is_empty() {
        return panel.child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("disk.usage_no_mounts")),
        );
    }

    let mut targets = div().flex().flex_wrap().gap(tokens::SPACE_6);
    for (index, mount) in &mounts {
        let mount_label = mount.clone();
        let mount_root = mount.clone();
        let ent = ent.clone();
        let pill = elements::pill(
            theme,
            SharedString::from(format!("disk-usage-scan-{index}")),
            &mount_label,
            false,
            false,
            move |_win, cx| {
                ent.update(cx, |v, cx| {
                    let _ = v.start_directory_scan(mount_root.clone());
                    cx.notify();
                });
            },
            |_hovered, _win, _cx| {},
        );
        targets = targets.child(with_scan_selector(div().child(pill), *index));
    }
    if let Some(snapshot) = own.filter(|s| s.status == DirectoryScanStatus::Scanning) {
        let ent = ent.clone();
        let _scan_id = snapshot.scan_id;
        targets = targets.child(elements::pill(
            theme,
            "disk-usage-cancel",
            i18n::t("disk.usage_cancel"),
            false,
            false,
            move |_win, cx| {
                ent.update(cx, |v, cx| {
                    let _ = v.cancel_directory_scan();
                    cx.notify();
                });
            },
            |_hovered, _win, _cx| {},
        ));
    }
    panel = panel.child(targets);

    // ── Body: idle hint or the bounded report ────────────────────────────────
    let Some(snapshot) = own else {
        return panel.child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t("disk.usage_idle")),
        );
    };

    panel = panel.child(totals_row(theme, snapshot, units));

    let visible = snapshot.entries.len().min(MAX_VISIBLE_ENTRIES);
    let mut list = div().flex().flex_col().gap(tokens::SPACE_4);
    for (index, entry) in snapshot.entries.iter().take(visible).enumerate() {
        list = list.child(with_entry_selector(
            entry_row(theme, snapshot, entry, units, ent.clone(), index),
            index,
        ));
    }
    if snapshot.entries.len() > visible {
        list = list.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(format!(
                    "+{} {}",
                    snapshot.entries.len() - visible,
                    i18n::t("disk.usage_more_entries")
                )),
        );
    }
    panel.child(list)
}

/// Whether the snapshot's scan root is this disk's own partition tree (the
/// partition mount point itself, or a drill-down path below it).
#[must_use]
pub(super) fn snapshot_targets_disk(snapshot: &DirectoryUsageSnapshot, d: &DiskMetrics) -> bool {
    d.partitions.iter().any(|partition| {
        if partition.mount_point.is_empty() {
            return false;
        }
        if partition.mount_point == "/" {
            // The root partition covers the whole tree, including /boot, /var…
            return snapshot.root.starts_with('/');
        }
        snapshot.root == partition.mount_point
            || snapshot
                .root
                .strip_prefix(&partition.mount_point)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn status_text(snapshot: &DirectoryUsageSnapshot) -> String {
    match snapshot.status {
        DirectoryScanStatus::Scanning => i18n::t("disk.usage_scanning").to_string(),
        DirectoryScanStatus::Completed => i18n::t("disk.usage_completed").to_string(),
        DirectoryScanStatus::Cancelled => i18n::t("disk.usage_cancelled").to_string(),
        DirectoryScanStatus::Failed(_) => i18n::t("disk.usage_failed").to_string(),
    }
}

/// Honest cumulative counters: files/dirs counted, the typed byte sum, and
/// the partial / unreadable / capped markers — each failure dimension is a
/// separate fact, never folded into a fabricated "complete" number.
fn totals_row(theme: &Theme, snapshot: &DirectoryUsageSnapshot, units: DisplayUnits) -> Div {
    let totals = &snapshot.totals;
    let mut text = format!(
        "{} {} · {} {}",
        totals.files_counted,
        i18n::t("disk.usage_files"),
        totals.directories_visited,
        i18n::t("disk.usage_dirs"),
    );
    if let Some(bytes) = totals.bytes_counted.current_value() {
        text.push_str(" · ");
        text.push_str(&units.format(*bytes, UnitKind::Drive, false));
    }
    if matches!(
        totals.bytes_counted.availability(),
        ScalarAvailability::Partial(_)
    ) {
        text.push_str(" · ");
        text.push_str(i18n::t("disk.usage_partial"));
    }
    if totals.unreadable_directories > 0 {
        text.push_str(&format!(
            " · {} {}",
            totals.unreadable_directories,
            i18n::t("disk.usage_unreadable_dirs")
        ));
    }
    if totals.capped {
        text.push_str(" · ");
        text.push_str(i18n::t("disk.usage_capped"));
    }
    div()
        .text_size(tokens::FONT_11)
        .text_color(theme.fg_dim)
        .child(text)
}

fn entry_row(
    theme: &Theme,
    snapshot: &DirectoryUsageSnapshot,
    entry: &DirectoryUsageEntry,
    units: DisplayUnits,
    ent: gpui::Entity<RootView>,
    index: usize,
) -> gpui::Stateful<Div> {
    let label = if entry.path.is_empty() {
        snapshot.root.clone()
    } else {
        entry.path.clone()
    };
    let size_text = match entry.size_bytes.current_value() {
        Some(bytes) => units.format(*bytes, UnitKind::Drive, false),
        None if entry.unreadable.is_some() => i18n::t("disk.usage_unreadable").to_string(),
        None => missing_value(),
    };
    let size_color = if entry.unreadable.is_some() {
        theme.danger
    } else {
        theme.fg
    };
    let count_text = match entry.file_count.current_value() {
        Some(count) => format!("{} {}", count, i18n::t("disk.usage_files")),
        None => String::new(),
    };

    let mut row = div()
        .id(SharedString::from(format!("disk-usage-entry-{index}")))
        .flex()
        .items_center()
        .justify_between()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex()
                .items_center()
                .gap(tokens::SPACE_4)
                .min_w(px(0.0))
                .child(
                    div()
                        .w(px(DEPTH_INDENT * entry.depth as f32))
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg)
                        .child(elements::truncated_text(&label)),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(tokens::SPACE_6)
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(count_text),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .text_color(size_color)
                        .child(size_text),
                ),
        );

    // Drill-down: a new bounded scan of this directory. Only when the scan is
    // not currently running — starting a new scan supersedes the active one,
    // and drilling mid-scan would silently discard its progress.
    let drillable = !entry.path.is_empty() && snapshot.status != DirectoryScanStatus::Scanning;
    if drillable {
        let root = format!("{}/{}", snapshot.root, entry.path);
        row = row
            .focusable()
            .cursor_pointer()
            .on_click(move |_event, _win, cx| {
                ent.update(cx, |v, cx| {
                    let _ = v.start_directory_scan(root.clone());
                    cx.notify();
                });
            });
    }
    row
}

#[cfg(any(test, feature = "test-support"))]
fn with_scan_selector(row: Div, index: usize) -> Div {
    row.debug_selector(move || format!("tm-disk-usage-scan:{index}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn with_scan_selector(row: Div, _index: usize) -> Div {
    row
}

#[cfg(any(test, feature = "test-support"))]
fn with_entry_selector<E>(row: E, index: usize) -> E
where
    E: InteractiveElement,
{
    row.debug_selector(move || format!("tm-disk-usage-entry:{index}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn with_entry_selector<E>(row: E, _index: usize) -> E {
    row
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_directory_usage_tests.rs"]
mod tests;
