//! Per-disk detail block for the Performance page.
//!
//! Reads the live snapshot's `disks` vector through the typed `Option`-returning
//! accessors so an unavailable field renders an honest dash instead of a
//! fabricated zero. Read-only consume of `taskmanager_core::core::metrics::DiskMetrics`;
//! this crate never mutates the shared snapshot shape. The accessor names mirror
//! `crates/taskmanager-gpui/src/gpui_app/perf_views/disk_stats.rs` so the two frontends agree on what
//! "unavailable" means for each disk field.
//!
//! Render contract: the Performance resource selector hands this section the
//! full content area of the Disk tab; the section renders nothing for a
//! zero-height area and an honest empty panel for an empty vector, so a cold
//! host never reads as a fabricated idle disk. Each disk block carries its OWN
//! two-row read/write throughput trend plus a single-row active-time
//! percentage trend (the split-direction and activity windows from that disk's
//! own `LiveGraphHistory`); the resource tab deliberately keeps the
//! per-device history authoritative.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{
    MISSING_VALUE, device_status_i18n_key, effective_smart_status, has_smart_fields, missing_value,
    smart_section_visible,
};
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;

/// Render the per-disk detail section into `area`. A zero-height area (the
/// small-terminal case where no panel was allocated) renders nothing. Each disk
/// block carries its OWN two-row read/write throughput trend and one-row
/// active-time percentage trend (that disk's split-direction and activity
/// windows from the shared `LiveGraphHistory`), so per-device
/// is the point; the system-wide headline is omitted. An empty disk vector renders an honest empty panel — never a
/// fabricated idle disk.
pub(super) fn render_disk_section(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    disks: &[DiskMetrics],
) {
    if area.height == 0 {
        return;
    }
    if disks.is_empty() {
        super::render_empty_panel(
            frame,
            theme,
            area,
            t("common.disk"),
            // No existing catalog key for "no disk at all"; kept English by the
            // i18n rule (do not edit locales) and listed in the task notes.
            "No disk telemetry available",
        );
        return;
    }
    let lines = disk_lines(
        disks,
        &app,
        theme,
        app.prefs.units[2],
        app.prefs.units[3],
        app.prefs.graph_points,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(super::panel(t("common.disk"), theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Cap on how many directory-usage entries the panel renders (the core's
/// `report_entries` already sorts largest-first and truncates at
/// `MAX_DIRECTORY_SCAN_REPORTED`; the terminal panel only has room for a
/// handful, so this is a pure presentational cap on top of the bounded feed).
const MAX_DIRECTORY_USAGE_ROWS: usize = 8;
/// Cap on the depth-based indentation so a deeply nested entry cannot push its
/// size column off-screen (the core permits up to `MAX_DIRECTORY_SCAN_DEPTH`).
const MAX_DIRECTORY_USAGE_INDENT: u32 = 6;

/// Render the directory-usage projection panel. Routed only
/// under the Disk device. A stashed `Update` snapshot renders a titled panel
/// with the largest entries (depth-indented path + size; a danger dash for an
/// unreadable subtree instead of a fabricated zero), the cumulative totals, and
/// the scan status Debug-formatted inline (`format!("{:?}")`, matching the TUI
/// inline-English precedent — no `locales` edit). `None` renders an honest idle
/// line — never fabricated entries or counts.
///
/// Render-only consume of the SHARED `SystemProjectionStore::directory_usage` slot
/// (latest-wins from the platform batch fold): the scan lifecycle (start /
/// cancel) is toggled by the `d` key on the Disk device in the runtime, which
/// routes through the `PlatformEffect::DirectoryUsage` seam — not by this
/// renderer. The `DirectoryUsageSnapshot` / entry / totals types are
/// imported directly from the core owner module (BN-01 vocabulary) and read
/// through their public fields.
pub(super) fn render_directory_usage(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
) {
    if area.height == 0 {
        return;
    }
    // Inline English by the i18n rule (do not edit locales); listed in the
    // task notes alongside the "No disk telemetry available" precedent.
    let title = "Directory usage";
    let Some(snapshot) = app.projection().directory_usage.as_ref() else {
        super::render_empty_panel(frame, theme, area, title, "No directory scan projected");
        return;
    };
    let use_bytes = app.prefs.units[2];
    let use_base2 = app.prefs.units[3];
    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    // Scan root for context (core treats it as an opaque display path).
    lines.push(ratatui::text::Line::from(Span::styled(
        format!("Root: {}", snapshot.root),
        Style::new().fg(theme.dim),
    )));
    // Largest-first entries (the core already sorted+truncated the feed).
    for entry in snapshot.entries.iter().take(MAX_DIRECTORY_USAGE_ROWS) {
        let indent = "  ".repeat(entry.depth.min(MAX_DIRECTORY_USAGE_INDENT) as usize + 1);
        let path = if entry.path.is_empty() {
            "(root)".to_string()
        } else {
            entry.path.clone()
        };
        // An unreadable subtree renders a danger dash — never a fabricated 0 B
        // — while a confirmed empty directory keeps its measured "0 B".
        let size_span = if entry.unreadable.is_some() {
            Span::styled(MISSING_VALUE, Style::new().fg(theme.danger))
        } else {
            Span::raw(super::perf_data::directory_entry_size(
                entry, use_bytes, use_base2,
            ))
        };
        lines.push(ratatui::text::Line::from(vec![
            Span::raw(indent),
            Span::raw(path),
            Span::raw("  "),
            size_span,
        ]));
    }
    // Cumulative totals. `bytes_counted` is typed: `Partial(failure)` once any
    // directory was unreadable, so the dash-vs-number distinction survives here.
    let bytes_text = super::perf_data::directory_total_size(snapshot, use_bytes, use_base2);
    let capped_suffix = if snapshot.totals.capped {
        " · capped"
    } else {
        ""
    };
    lines.push(ratatui::text::Line::from(format!(
        "Totals: {} files · {} dirs · {} · {} unreadable{}",
        snapshot.totals.files_counted,
        snapshot.totals.directories_visited,
        bytes_text,
        snapshot.totals.unreadable_directories,
        capped_suffix,
    )));
    // Status Debug-formatted (Scanning / Completed / Cancelled / Failed(..)).
    lines.push(ratatui::text::Line::from(Span::styled(
        format!("Status: {:?}", snapshot.status),
        Style::new().fg(theme.dim),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .block(super::panel(title, theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Build one honest detail line set per disk. Each line's unavailable fields
/// resolve to "—" through the shared observers so a cold/unprobed device never
/// reads as a fabricated 0% / 0°C / 0 B. Each disk also gets its OWN two-row
/// read/write throughput trend (that disk's split-direction windows from its
/// `LiveGraphHistory`, one shared scale) right under its header, followed by a
/// single-row active-time percentage trend with its 0-100 summary; a direction
/// with <2 finite samples renders the dotted placeholder instead of a
/// fabricated flat line. Throughput rates honor the
/// applied drive unit pair (bytes/bits × base-2/base-10); fixed capacities
/// stay byte-counted (GPUI parity — rates honor units, fixed sizes do not).
fn disk_lines(
    disks: &[DiskMetrics],
    shell: &ShellApp,
    theme: TuiTheme,
    use_bytes: bool,
    use_base2: bool,
    graph_window: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = Vec::with_capacity(disks.iter().map(disk_body_line_count).sum());
    for disk in disks {
        let data = super::perf_data::disk_data(disk, use_bytes, use_base2);
        // Header: icon + name + provider free-form device type. An empty
        // disk_type renders nothing (honest absence, not a stray " · ").
        let kind_suffix = if disk.disk_type.is_empty() {
            String::new()
        } else {
            format!(" · {}", disk.disk_type)
        };
        lines.push(ratatui::text::Line::from(format!(
            "{} {}{}",
            theme.glyph(IconId::Disk),
            disk.name,
            kind_suffix,
        )));
        // Device health verdict (GPUI disk_stats first stat; shared
        // presentation single-source). The typed DeviceStatus vocabulary
        // carries every state — stale/degraded, permission-denied,
        // missing-tool, unsupported — which the SMART section's smart_status
        // variant below cannot express on its own.
        lines.push(ratatui::text::Line::from(format!(
            "  {} {}",
            t("device.status"),
            t(device_status_i18n_key(disk.device_state.status)),
        )));
        // Removable media (GPUI disk_stats tail row). The capability is only
        // named when the adapter PROVED removable media; an unresolved probe
        // renders nothing — never a fabricated Yes/No.
        if disk.media_removable() == Some(true) {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("disk.removable"),
                t("common.yes"),
            )));
        }
        // Per-disk throughput trend: this disk's own read and write windows
        // (the split-direction companions of the summed series, same stable
        // key the recorder uses) as two label-prefixed rows on ONE shared
        // scale, so the directions read as comparable amplitudes. Read keeps
        // the disk family accent; write rides the dim variant — the TUI
        // counterpart of the iced same-hue lift, with the label keeping the
        // rows distinguishable on monochrome terminals. A direction with <2
        // finite samples renders the dotted "collecting" placeholder; a
        // missing sample inside a live row renders a gap dot — never a
        // fabricated flat line or baseline block.
        let read_window = shell
            .history
            .disk_read_bytes_per_sec_for(&disk.device_id, disk.device_generation.get());
        let write_window = shell
            .history
            .disk_write_bytes_per_sec_for(&disk.device_id, disk.device_generation.get());
        let trend = super::sparkline::device_dual_trend_in(
            theme.terminal.glyphs,
            &read_window,
            &write_window,
            graph_window,
        );
        let label_width = super::text::cell_width(t("disk.read"))
            .max(super::text::cell_width(t("disk.write")))
            .max(super::text::cell_width(t("disk.active_time")));
        lines.push(super::sparkline::dual_trend_line(
            t("disk.read"),
            label_width,
            &trend.primary,
            Style::new().fg(theme.accent),
        ));
        lines.push(super::sparkline::dual_trend_line(
            t("disk.write"),
            label_width,
            &trend.secondary,
            Style::new().fg(theme.dim),
        ));
        // The total-throughput summary stays on the summed window: the two
        // direction rows carry the shape, this line carries the read+write
        // total statistics.
        let window = shell
            .history
            .disk_bytes_per_sec_for(&disk.device_id, disk.device_generation.get());
        if let Some(summary) = super::sparkline::device_summary_line_in(
            theme.terminal.glyphs,
            t("common.throughput"),
            &window,
            super::sparkline::DeviceSummaryUnit::BytesPerSecond,
        ) {
            lines.push(ratatui::text::Line::from(format!("  {summary}")));
        }
        // Active-time percentage trend from this disk's own activity ring —
        // the same generation-scoped window the recorder feeds, so the busy
        // curve rides beside (not inside) the throughput pair. Per-row
        // normalization keeps the shared sparkline semantics; the summary line
        // beneath carries the absolute 0-100 percentages.
        let active_window = shell
            .history
            .disk_active_time_pct_for(&disk.device_id, disk.device_generation.get());
        lines.push(super::sparkline::dual_trend_line(
            t("disk.active_time"),
            label_width,
            &super::sparkline::device_trend_in(theme.terminal.glyphs, &active_window, graph_window),
            Style::new().fg(theme.accent),
        ));
        if let Some(summary) = super::sparkline::device_summary_line_in(
            theme.terminal.glyphs,
            t("disk.active_time"),
            &active_window,
            super::sparkline::DeviceSummaryUnit::Percent,
        ) {
            lines.push(ratatui::text::Line::from(format!("  {summary}")));
        }
        // Read/write rate + active time. Each scalar is independently
        // unavailable; a confirmed measured zero stays visible while a provider
        // failure renders "—". Rates honor the applied unit pair.
        lines.push(ratatui::text::Line::from(format!(
            "  {} {}/s · {} {}/s · {} {}",
            t("disk.read"),
            data.read,
            t("disk.write"),
            data.write,
            t("disk.active_time"),
            data.active,
        )));
        // Response time + IOPS — optional latency/throughput counters a vendor
        // may omit independently, so the line renders only when at least one is
        // proven rather than printing an empty pair of dashes.
        if let Some((response, iops)) = data.response_iops {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("disk.response"),
                response,
                t("disk.iops"),
                iops,
            )));
        }
        // Top-level disk capacity + free space (the parent device, distinct from
        // per-partition children). Omitted when the provider supplies neither, so
        // a virtual/loop device prints no fake totals.
        if let Some((capacity, free)) = data.capacity_free {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("disk.capacity"),
                capacity,
                t("disk.free"),
                free,
            )));
        }
        // Filesystem type (btrfs/ext4/...), only when reported.
        if !disk.fs_type.trim().is_empty() {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {}",
                t("disk.filesystem"),
                disk.fs_type.trim(),
            )));
        }
        if disk
            .serial
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || disk
                .revision
                .as_deref()
                .is_some_and(|value| !value.is_empty())
        {
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("disk.serial"),
                disk.serial
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(MISSING_VALUE),
                t("disk.revision"),
                disk.revision
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(MISSING_VALUE),
            )));
        }
        // SMART health renders only when the provider actually supplied it:
        // temperature (with the critical threshold when the vendor exposes
        // one), normalized endurance-used, temperature history, power-on
        // hours, and — when the provider is available but scalars have not
        // arrived yet — an honest status verdict. A provider that could not
        // open (no admin, missing tool, unsupported) hides the whole section.
        if smart_section_visible(disk) {
            let temp =
                smart_temperature_readout(disk.smart_temperature_c, disk.smart_temp_critical_c);
            let used = super::observed_percentage(disk.smart_percent_used);
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {}",
                t("common.temperature"),
                temp,
                t("disk.endurance_used"),
                used,
            )));
            // SMART temperature history for this physical identity only.
            // Telemetry-store scopes it by device generation, so another disk
            // cannot leak into this detail trend after hot-plug or reorder.
            let temperature_history = shell
                .history
                .disk_temperature_c_for(&disk.device_id, disk.device_generation.get());
            if !temperature_history.is_empty() {
                lines.push(ratatui::text::Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        super::sparkline::device_trend_in(
                            theme.terminal.glyphs,
                            &temperature_history,
                            graph_window,
                        ),
                        Style::new().fg(theme.accent),
                    ),
                ]));
                if let Some(summary) = super::sparkline::device_summary_line_in(
                    theme.terminal.glyphs,
                    t("common.temperature"),
                    &temperature_history,
                    super::sparkline::DeviceSummaryUnit::Celsius,
                ) {
                    lines.push(ratatui::text::Line::from(format!("  {summary}")));
                }
            }
            // Power-on hours, only when the vendor exposes the counter.
            if let Some(hours) = disk.smart_power_on_hours {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {} h ({} d)",
                    t("disk.power_on"),
                    hours,
                    hours / 24,
                )));
            }
            // SMART status verdict: when the provider is available but exposed
            // no readout yet, surface the effective status
            // (Healthy/Stale/PermissionDenied/…) so the row reads honestly
            // instead of silently implying "no health to report". Mirrors
            // iced's disk_summary_lines SMART row and GPUI's disk_stats
            // (shared presentation single-source).
            if !has_smart_fields(disk) {
                lines.push(ratatui::text::Line::from(format!(
                    "  {} {}",
                    t("disk.smart_status"),
                    t(device_status_i18n_key(effective_smart_status(disk))),
                )));
            }
        }
        // Partition space: only when the snapshot carries children. An empty
        // partition list renders nothing here (honest absence, not a fabricated
        // "0 B free" line per slot). An unmounted partition is named so.
        for partition in &disk.partitions {
            let data = super::perf_data::partition_data(partition);
            let mount = if partition.mount_point.is_empty() {
                t("disk.unmounted").to_string()
            } else {
                partition.mount_point.clone()
            };
            lines.push(ratatui::text::Line::from(format!(
                "  {} {} · {} {} · {} {}",
                partition.name,
                mount,
                t("disk.capacity"),
                data.capacity,
                t("disk.free"),
                data.free,
            )));
        }
    }
    lines
}

/// The number of body lines one disk contributes: header + device status +
/// removable + two direction trends + throughput summary + active-time trend
/// and summary + rates (always nine), plus at most the response/IOPS,
/// capacity/free, filesystem, temperature history and power-on rows, plus the
/// SMART-status verdict when no SMART readout exists, plus one honest line
/// per reported partition. Kept as a loose upper bound for the line buffer
/// preallocation.
fn disk_body_line_count(disk: &DiskMetrics) -> usize {
    17 + disk.partitions.len()
}

/// SMART temperature with an optional critical threshold, matching the no-space
/// [`super::observed_temperature`] convention ("42°C" or "42 / 70°C"); "—" when
/// the vendor supplied no temperature at all.
fn smart_temperature_readout(temp: Option<f32>, critical: Option<f32>) -> String {
    match temp {
        Some(value) => match critical {
            Some(crit) if crit > 0.0 => format!("{:.0} / {:.0}°C", value, crit),
            _ => format!("{:.0}°C", value),
        },
        None => missing_value(),
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_disks_tests.rs"]
mod tests;
