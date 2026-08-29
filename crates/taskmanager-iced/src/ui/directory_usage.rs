//! Directory-usage scan panel for the Performance Disk device (G-13).
//!
//! Mirrors the TUI's scan lifecycle semantics through the F1 typed effect
//! lane: the Disk-device action button toggles a bounded scan — an idle or
//! terminal slot starts a scan of the first mounted partition (or `/` when
//! none is reported), a `Scanning` slot cancels the active scan id — queued
//! as [`taskmanager_shell::ShellApp::request_directory_usage`] so the
//! frontend never constructs the payload itself. Progress and results arrive
//! as `PlatformEventBatch::directory_usage_events` and land in the shared
//! `SystemProjectionStore::directory_usage` slot (latest-wins), which this panel renders.
//!
//! Honesty contract (the shared [`DirectoryUsageSnapshot`] types, never
//! fabricated): each typed scan status renders its own label; an unreadable
//! subtree renders a danger mark instead of a zero; the cumulative byte sum
//! keeps its `Partial` mark; and the `capped` bound is a visible fact.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::DirectoryUsageRequest;
use taskmanager_application::i18n::t;
use taskmanager_core::core::directory_usage::{
    DirectoryScanBounds, DirectoryScanSpec, DirectoryScanStatus, DirectoryUsageEntry,
    DirectoryUsageSnapshot,
};
use taskmanager_core::core::metrics::{DiskMetrics, ScalarAvailability};

use taskmanager_theme::tokens;

use crate::app::Message;
use crate::focus;
use crate::theme;

use taskmanager_shell::presentation::{bytes, missing_value};

/// Maximum entry rows rendered before collapsing into a "+N more" line. The
/// snapshot itself is already bounded (`max_reported`); this is the panel's
/// presentational cap (mirrors the GPUI/TUI panel caps).
const MAX_VISIBLE_ENTRIES: usize = 8;
/// Depth indent per nesting level, in device pixels (capped so a deep path
/// cannot push its size column off-panel).
const MAX_INDENT_DEPTH: u32 = 6;

/// The typed request the Disk-device action submits (pure seam the update
/// path and the tests share). `active` is the shared latest snapshot, if any.
///
/// - An active scan whose root belongs to this disk toggles to `Cancel` (its
///   own `scan_id`; cancelling an unknown/finished scan is idempotent on the
///   provider side).
/// - Otherwise a `StartScan` of the disk's first mounted partition (or `/`
///   when none is reported) with the default bounded policy — the same
///   one-toggle semantics the TUI's `d` key and GPUI's scan pills implement.
#[must_use]
pub(crate) fn toggle_request(
    disk: &DiskMetrics,
    active: Option<&DirectoryUsageSnapshot>,
) -> Option<DirectoryUsageRequest> {
    if let Some(snapshot) = active
        && snapshot.status == DirectoryScanStatus::Scanning
        && snapshot_targets_disk(snapshot, disk)
    {
        return Some(DirectoryUsageRequest::Cancel(snapshot.scan_id));
    }
    let root = scan_target_root(disk)?;
    Some(DirectoryUsageRequest::StartScan(DirectoryScanSpec {
        root,
        bounds: DirectoryScanBounds::default(),
    }))
}

/// The scan root this disk's action button targets: the first mounted
/// partition's mount point, the disk's own mount point when the provider
/// reported no partition children (the demo / whole-disk-filesystem shape),
/// or `/` when neither is known (the TUI fallback — a disk without a reported
/// mount still owns the root partition on typical hosts).
fn scan_target_root(disk: &DiskMetrics) -> Option<String> {
    let mounted = disk
        .partitions
        .iter()
        .find(|partition| !partition.mount_point.is_empty())
        .map(|partition| partition.mount_point.clone())
        .or_else(|| (!disk.mount_point.is_empty()).then(|| disk.mount_point.clone()));
    Some(mounted.unwrap_or_else(|| "/".to_string()))
}

/// Whether one mount point owns the snapshot root: the mount itself or a
/// drill-down path below it (the root mount covers the whole tree).
fn mount_owns_root(mount_point: &str, root: &str) -> bool {
    if mount_point == "/" {
        return root.starts_with('/');
    }
    root == mount_point
        || root
            .strip_prefix(mount_point)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Whether the snapshot's scan root is this disk's own tree: one of its
/// partition mount points, or the disk-level mount point when no partition
/// children were reported. Mirrors the GPUI targeting rule so a scan started
/// on another disk's mount never leaks into this panel.
#[must_use]
pub(crate) fn snapshot_targets_disk(snapshot: &DirectoryUsageSnapshot, disk: &DiskMetrics) -> bool {
    disk.partitions
        .iter()
        .filter(|partition| !partition.mount_point.is_empty())
        .any(|partition| mount_owns_root(&partition.mount_point, &snapshot.root))
        || (!disk.mount_point.is_empty() && mount_owns_root(&disk.mount_point, &snapshot.root))
}

/// Whether the disk has any reported mount to scan (partition children or its
/// own mount point). A disk with neither renders the no-mounts hint instead
/// of an action that could only scan a fabricated root.
fn has_reported_mount(disk: &DiskMetrics) -> bool {
    disk.partitions
        .iter()
        .any(|partition| !partition.mount_point.is_empty())
        || !disk.mount_point.is_empty()
}

/// The localized label for one typed scan status (each terminal state is its
/// own fact; a failure renders the failure label, not a fabricated "done").
fn status_text(status: DirectoryScanStatus) -> &'static str {
    match status {
        DirectoryScanStatus::Scanning => t("disk.usage_scanning"),
        DirectoryScanStatus::Completed => t("disk.usage_completed"),
        DirectoryScanStatus::Cancelled => t("disk.usage_cancelled"),
        DirectoryScanStatus::Failed(_) => t("disk.usage_failed"),
    }
}

/// Honest cumulative counters: files/dirs counted, the typed byte sum, and
/// the partial / unreadable / capped markers — each failure dimension is a
/// separate fact, never folded into a fabricated "complete" number.
fn totals_text(snapshot: &DirectoryUsageSnapshot) -> String {
    let totals = &snapshot.totals;
    let mut line = format!(
        "{} {} · {} {}",
        totals.files_counted,
        t("disk.usage_files"),
        totals.directories_visited,
        t("disk.usage_dirs"),
    );
    if let Some(counted) = totals.bytes_counted.current_value() {
        line.push_str(" · ");
        line.push_str(&bytes(*counted));
    }
    if matches!(
        totals.bytes_counted.availability(),
        ScalarAvailability::Partial(_)
    ) {
        line.push_str(" · ");
        line.push_str(t("disk.usage_partial"));
    }
    if totals.unreadable_directories > 0 {
        line.push_str(&format!(
            " · {} {}",
            totals.unreadable_directories,
            t("disk.usage_unreadable_dirs")
        ));
    }
    if totals.capped {
        line.push_str(" · ");
        line.push_str(t("disk.usage_capped"));
    }
    line
}

/// The display label for one report entry: the scan root for the root entry,
/// the relative path otherwise.
fn entry_label(entry: &DirectoryUsageEntry, root: &str) -> String {
    if entry.path.is_empty() {
        root.to_string()
    } else {
        entry.path.clone()
    }
}

/// An unreadable subtree renders the danger-marked unreadable label — never
/// a fabricated "0 B" — while a measured zero keeps its real value.
fn entry_is_unreadable(entry: &DirectoryUsageEntry) -> bool {
    entry.unreadable.is_some()
}

/// The entry's size cell: the measured byte count, the typed unreadable
/// label, or an honest dash when the observation is simply absent.
fn entry_size_text(entry: &DirectoryUsageEntry) -> String {
    match entry.size_bytes.current_value() {
        Some(counted) => bytes(*counted),
        None if entry_is_unreadable(entry) => t("disk.usage_unreadable").to_string(),
        None => missing_value(),
    }
}

/// How many entries the collapsed panel renders (the bounded prefix of the
/// already-sorted largest-first feed).
fn visible_entries(total: usize) -> usize {
    total.min(MAX_VISIBLE_ENTRIES)
}

/// Render the directory-usage panel for one disk: the typed status header,
/// the scan/cancel action, and — once a snapshot targeting this disk landed —
/// the bounded largest-first entry report plus the honest totals. A shared
/// snapshot for a different mount renders the idle hint (targeting rule), and
/// a disk with no reported mounts renders the no-mounts hint instead of an
/// action that could only scan a fabricated root.
pub(super) fn usage_panel<'a>(
    app: &'a crate::IcedApp,
    disk: &'a DiskMetrics,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let own = app
        .shell
        .projection()
        .directory_usage
        .as_ref()
        .filter(|snapshot| snapshot_targets_disk(snapshot, disk));

    // Header: the analysis title with the typed status on the right.
    let header_right = own.map_or_else(
        || text("").size(f32::from(tokens::FONT_11)),
        |snapshot| text(status_text(snapshot.status)).size(f32::from(tokens::FONT_11)),
    );
    let header = row![
        text(t("disk.usage_analysis")).size(f32::from(tokens::FONT_13)),
        text(" · ").size(f32::from(tokens::FONT_11)),
        header_right,
    ]
    .spacing(4);

    let mut body: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = vec![header.into()];

    if !has_reported_mount(disk) {
        body.push(
            text(t("disk.usage_no_mounts"))
                .size(f32::from(tokens::FONT_12))
                .into(),
        );
        return panel_container(theme_snapshot, body);
    }

    // The action button: Scan on an idle/terminal slot, Cancel while this
    // disk's scan is running (the conditional destructive affordance).
    let scanning = own.is_some_and(|snapshot| snapshot.status == DirectoryScanStatus::Scanning);
    let (target, label) = if scanning {
        (
            crate::app::FocusTarget::DirectoryUsageCancel,
            t("disk.usage_cancel"),
        )
    } else {
        (
            crate::app::FocusTarget::DirectoryUsageScan,
            t("disk.usage_scan"),
        )
    };
    let action = if scanning {
        focus::button(
            theme_snapshot,
            target,
            label,
            Message::ToggleDirectoryUsageScan,
            true,
        )
    } else {
        focus::ghost_button(
            theme_snapshot,
            target,
            label,
            Message::ToggleDirectoryUsageScan,
        )
    };
    // The idle hint or the scanned root path — the path is self-describing.
    let root_line = own.map_or_else(
        || {
            text(t("disk.usage_idle"))
                .size(f32::from(tokens::FONT_12))
                .width(Length::Fill)
        },
        |snapshot| {
            text(snapshot.root.clone())
                .size(f32::from(tokens::FONT_12))
                .width(Length::Fill)
                .wrapping(iced::widget::text::Wrapping::Glyph)
        },
    );
    body.push(
        column![action, root_line]
            .spacing(4)
            .width(Length::Fill)
            .into(),
    );

    let Some(snapshot) = own else {
        return panel_container(theme_snapshot, body);
    };

    body.push(
        text(totals_text(snapshot))
            .size(f32::from(tokens::FONT_11))
            .into(),
    );

    let mut list: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = Vec::new();
    for entry in snapshot
        .entries
        .iter()
        .take(visible_entries(snapshot.entries.len()))
    {
        let danger = entry_is_unreadable(entry);
        let size_color = if danger {
            taskmanager_theme::iced::color(theme_snapshot.danger)
        } else {
            taskmanager_theme::iced::color(theme_snapshot.palette().fg)
        };
        let indent = Length::Fixed(12.0 * entry.depth.min(MAX_INDENT_DEPTH) as f32);
        list.push(
            row![
                container(column![]).width(indent),
                text(entry_label(entry, &snapshot.root))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fill),
                text(entry_size_text(entry))
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_theme| {
                        iced::widget::text::Style {
                            color: Some(size_color),
                        }
                    }),
            ]
            .spacing(6)
            .into(),
        );
    }
    let hidden = snapshot.entries.len() - visible_entries(snapshot.entries.len());
    if hidden > 0 {
        list.push(
            text(format!("+{} {}", hidden, t("disk.usage_more_entries")))
                .size(f32::from(tokens::FONT_11))
                .into(),
        );
    }
    body.push(column(list).spacing(2).into());

    panel_container(theme_snapshot, body)
}

/// Wrap the panel body in the shared panel chrome.
fn panel_container<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    body: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    container(column(body).spacing(6).width(Length::Fill))
        .padding(8)
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/directory_usage_tests.rs"]
mod tests;
