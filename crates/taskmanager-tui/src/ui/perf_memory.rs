//! Memory-composition bar, legend, and swap bar for the Memory Performance
//! view.
//!
//! Renders the shared [`taskmanager_shell::memory`] breakdown as a stacked
//! proportion bar with a per-category legend, plus a secondary swap bar when
//! swap is configured. The breakdown math (which categories exist and their
//! byte counts) is shared and single-source; this module only maps a
//! segment's semantic [`MemSegmentKind`] onto a terminal color and draws the
//! bar — it never re-derives the breakdown.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_core::core::units::format_quantity_with;
use taskmanager_shell::memory::{self, MemSegmentKind, SwapBreakdown};
use taskmanager_shell::presentation::missing_value;
use taskmanager_ui_contract::IconId;

use crate::TuiApp;
use crate::TuiTheme;

use super::units::memory_text_pref;

/// Bytes in one mebibyte — the signed memory usage rate is published in
/// MiB/s and rendered through the shared byte ladder.
const MIB_BYTES: u64 = 1024 * 1024;

/// Below this absolute rate (MiB/s) the usage-rate row reports the shared
/// dash instead of a sub-tick value, mirroring the gpui row gate
/// (`memory_stats.rs`): a rate the kernel cannot distinguish from zero is
/// honest absence, not a fabricated `+0 MiB/s`.
const USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC: f32 = 0.05;

/// Map a shared segment kind onto the terminal palette color. Every frontend
/// derives these from the same `taskmanager-theme` tokens, so the bar matches
/// the gpui/iced composition bar.
fn segment_color(kind: MemSegmentKind, theme: TuiTheme) -> Color {
    use MemSegmentKind;
    match kind {
        MemSegmentKind::Active | MemSegmentKind::InUse => theme.memory,
        MemSegmentKind::Inactive => theme.accent,
        MemSegmentKind::Cache => theme.disk,
        MemSegmentKind::ZfsArc => theme.zfs_arc,
        MemSegmentKind::Free | MemSegmentKind::Available => theme.fg_dim,
        MemSegmentKind::Other => theme.shade,
    }
}

/// The vertical height the composition block needs for this snapshot — a
/// header row, the bar row, one legend row per non-zero segment, an
/// optional two-row swap bar, and the labelled memory-stats rows — or the
/// stats rows alone when there is no total memory to break down. Used by the
/// overview layout to reserve space before rendering.
#[must_use]
pub fn composition_height(memory: &MemoryMetrics) -> u16 {
    let base = match super::perf_data::memory_composition_data(memory) {
        Some(data) => {
            let nonzero = data.segments.iter().filter(|seg| seg.bytes > 0).count() as u16;
            let swap = if data.swap.is_some() { 2 } else { 0 };
            // header(1) + bar(1) + legend(nonzero) + swap(swap)
            2 + nonzero + swap
        }
        // No total to break down: the stats rows still render, so a host the
        // composition projection cannot describe keeps its labelled facts.
        None => 0,
    };
    base + stats_row_count(memory)
}

/// The number of stats rows this snapshot labels: the six fixed rows
/// (available, hardware reserved, speed, slots, committed, usage rate) stay
/// labelled with an honest dash when their fact is unavailable, while the
/// Buffers row keeps the gpui conditional-row semantics (only when the host
/// reports the counter).
fn stats_row_count(memory: &MemoryMetrics) -> u16 {
    6 + u16::from(memory.current_buffers_bytes().is_some())
}

/// The signed memory usage-rate readout: an explicit `+`/`-` sign (ASCII, so
/// every terminal profile keeps one cell per glyph) over the shared byte
/// ladder formatted as a per-second quantity.
fn signed_memory_rate_readout(rate_mib_per_sec: f32, use_bytes: bool, use_base2: bool) -> String {
    let sign = if rate_mib_per_sec < 0.0 { '-' } else { '+' };
    let magnitude_bytes = (f64::from(rate_mib_per_sec.abs()) * MIB_BYTES as f64).round() as u64;
    format!(
        "{sign}{}",
        format_quantity_with(magnitude_bytes, use_bytes, use_base2, true)
    )
}

fn memory_bytes_readout(value: u64, use_bytes: bool, use_base2: bool) -> String {
    memory_text_pref(value, use_bytes, use_base2)
}

/// The labelled stats row set (`perf_views/memory_stats.rs` parity): every
/// value routes through the same typed `MemoryMetrics` accessors the gpui
/// rows read, so an unavailable observation is the shared dash in both
/// frontends and a measured value is formatted once by the shared unit
/// ladder.
fn memory_stats_lines(
    memory: &MemoryMetrics,
    theme: TuiTheme,
    use_bytes: bool,
    use_base2: bool,
) -> Vec<Line<'static>> {
    let readout = |value: u64| memory_bytes_readout(value, use_bytes, use_base2);
    let slots = match (memory.current_slots_used(), memory.current_slots_total()) {
        (Some(used), Some(total)) => format!("{used} / {total}"),
        _ => missing_value(),
    };
    // The gpui committed readout only reports the pair once a real limit
    // exists; a committed counter without a limit stays a dash.
    let committed = match (
        memory.current_committed_bytes(),
        memory.current_commit_limit_bytes(),
    ) {
        (Some(committed), Some(limit)) if limit > 0 => {
            format!("{} / {}", readout(committed), readout(limit))
        }
        _ => missing_value(),
    };
    let usage_rate = memory
        .current_used_rate_mib_per_sec()
        .filter(|rate| rate.abs() >= USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC)
        .map_or_else(missing_value, |rate| {
            signed_memory_rate_readout(rate, use_bytes, use_base2)
        });

    let mut rows: Vec<(&str, String)> = vec![
        (
            t("mem.available"),
            memory
                .projected_available_bytes()
                .map_or_else(missing_value, readout),
        ),
        (
            t("mem.hardware_reserved"),
            memory
                .current_hardware_reserved_bytes()
                .map_or_else(missing_value, readout),
        ),
        (
            t("common.speed"),
            memory
                .current_speed_mhz()
                .map_or_else(missing_value, |speed| format!("{speed} MT/s")),
        ),
        (t("mem.slots"), slots),
        (t("mem.committed"), committed),
        (t("mem.usage_rate"), usage_rate),
    ];
    if let Some(buffers) = memory.current_buffers_bytes() {
        rows.push((t("mem.buffers"), readout(buffers)));
    }
    rows.into_iter()
        .map(|(label, value)| stats_line(label, value, theme))
        .collect()
}

/// One labelled stats row: a dim label plus the bright value, single-spaced
/// so the row reads as one fact pair at any width.
fn stats_line(label: &str, value: String, theme: TuiTheme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label} "), Style::new().fg(theme.dim)),
        Span::styled(value, Style::new().fg(theme.color(Color::White))),
    ])
}

/// Render the memory-composition block (header + stacked bar + legend + swap
/// bar + labelled stats rows) into `area`. Called only on the Memory
/// Performance view, and only when the overview reserved a non-zero
/// [`composition_height`].
pub fn render_memory_composition(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    memory_snapshot: &MemoryMetrics,
    theme: TuiTheme,
    area: Rect,
) {
    let data = super::perf_data::memory_composition_data(memory_snapshot);
    // The applied unit matrix (bytes/bits × base-2/base-10) resolves once at
    // render entry and flows down into every formatted label.
    let use_bytes = app.prefs.units[0];
    let use_base2 = app.prefs.units[1];
    let stats_rows = stats_row_count(memory_snapshot);
    let base_height = area.height.saturating_sub(stats_rows);
    let [base_area, stats_area] = Layout::vertical([
        Constraint::Length(base_height),
        Constraint::Length(stats_rows),
    ])
    .areas(area);

    if let Some(data) = data {
        let total = data.total;
        let used = data.used;
        let segments = data.segments;
        let nonzero: Vec<_> = segments.iter().copied().filter(|s| s.bytes > 0).collect();
        let swap = data.swap;

        // header(1) + bar(1) + legend(nonzero) + swap(0|2)
        let mut constraints = vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(nonzero.len() as u16),
        ];
        if swap.is_some() {
            constraints.push(Constraint::Length(2));
        }
        let rects = Layout::vertical(&constraints).split(base_area);

        // Header: icon + "Composition" on the left, "In use X · Y total" on the
        // right, expressed as one styled line. A known total does not imply that
        // the current used counter was also collected.
        let used_label = used.map_or_else(missing_value, |value| {
            memory_text_pref(value, use_bytes, use_base2)
        });
        let header = Line::from(vec![
            Span::styled(
                format!(" {} ", theme.glyph(IconId::Memory)),
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                t("mem.composition"),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                format!("{} {}", t("mem.in_use"), used_label),
                Style::new().fg(theme.fg_dim),
            ),
            Span::raw("  ·  "),
            Span::styled(
                format!(
                    "{} {}",
                    memory_text_pref(total, use_bytes, use_base2),
                    t("mem.total")
                ),
                Style::new().fg(theme.fg_dim),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), rects[0]);

        render_stacked_bar(frame, rects[1], &segments, theme);

        let legend = Paragraph::new(
            nonzero
                .iter()
                .map(|seg| {
                    legend_line(
                        *seg,
                        total,
                        segment_color(seg.kind, theme),
                        theme,
                        use_bytes,
                        use_base2,
                    )
                })
                .collect::<Vec<_>>(),
        );
        frame.render_widget(legend, rects[2]);

        if let Some(swap) = swap {
            let [label_area, bar_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(rects[3]);
            render_swap_bar(
                frame, label_area, bar_area, &swap, theme, use_bytes, use_base2,
            );
        }
    }

    // The labelled stats rows render beneath the composition bar (or at the
    // block's top when no total exists to break down).
    if stats_area.height > 0 {
        frame.render_widget(
            Paragraph::new(memory_stats_lines(
                memory_snapshot,
                theme,
                use_bytes,
                use_base2,
            )),
            stats_area,
        );
    }
}

/// One legend row: a colored swatch, the localized label, the percent share,
/// and the byte count.
fn legend_line(
    seg: memory::MemSegment,
    total: u64,
    color: Color,
    theme: TuiTheme,
    use_bytes: bool,
    use_base2: bool,
) -> Line<'static> {
    let pct = if total == 0 {
        0.0
    } else {
        (seg.bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    };
    Line::from(vec![
        Span::styled("█ ", Style::new().fg(color)),
        Span::styled(
            seg.label.to_string(),
            Style::new().fg(theme.color(Color::White)),
        ),
        Span::raw("   "),
        Span::styled(format!("{pct:>4.0}%"), Style::new().fg(theme.fg_dim)),
        Span::raw("  "),
        Span::styled(
            memory_text_pref(seg.bytes, use_bytes, use_base2),
            Style::new().fg(theme.fg_dim),
        ),
    ])
}

/// Render the horizontal stacked proportion bar: the reserved shade track
/// first, then one fill per non-zero segment sized by its share of the
/// segment-byte sum (matching gpui's share-sum normalization).
fn render_stacked_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    segments: &[memory::MemSegment],
    theme: TuiTheme,
) {
    // Track fill first so sub-cell rounding leaks the reserved shade, not the
    // page backdrop.
    frame.render_widget(Block::new().style(Style::new().bg(theme.shade)), area);
    let drawn: Vec<(Color, u64)> = segments
        .iter()
        .copied()
        .filter(|s| s.bytes > 0)
        .map(|s| (segment_color(s.kind, theme), s.bytes))
        .collect();
    let sum: u64 = drawn.iter().map(|(_, bytes)| *bytes).sum();
    if sum == 0 {
        return;
    }
    let constraints: Vec<Constraint> = drawn
        .iter()
        .map(|(_, bytes)| {
            Constraint::Percentage((((*bytes as f64 / sum as f64) * 100.0).round() as u16).max(1))
        })
        .collect();
    let cells = Layout::horizontal(&constraints).split(area);
    for ((color, _), cell) in drawn.iter().zip(cells.iter()) {
        frame.render_widget(Block::new().style(Style::new().bg(*color)), *cell);
    }
}

/// Render the secondary swap bar: a label line ("Swap X / Y (NN%) · zram Z ·
/// zram RAM used R · zswap on") and a two-segment used/free proportion bar
/// beneath it.
fn render_swap_bar(
    frame: &mut Frame<'_>,
    label_area: Rect,
    bar_area: Rect,
    swap: &SwapBreakdown,
    theme: TuiTheme,
    use_bytes: bool,
    use_base2: bool,
) {
    let pct = if swap.total_bytes == 0 {
        0.0
    } else {
        (swap.used_bytes as f64 / swap.total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    };
    let mut label = format!(
        "{}  {} / {}  ({pct:.0}%)",
        t("mem.swap"),
        memory_text_pref(swap.used_bytes, use_bytes, use_base2),
        memory_text_pref(swap.total_bytes, use_bytes, use_base2),
    );
    if let Some(zram) = swap.zram_bytes.filter(|z| *z > 0) {
        label.push_str(&format!(
            "   ·   {} {}",
            t("mem.zram_swap"),
            memory_text_pref(zram, use_bytes, use_base2)
        ));
    }
    // The RAM the zram store actually consumes (`mm_stat` `mem_used_total`,
    // metadata included) — a distinct fact from both the swap-used view
    // above and the compressed size below, so its own segment.
    if let Some(ram) = swap.zram_memory_used_bytes.filter(|v| *v > 0) {
        label.push_str(&format!(
            "   ·   {} {}",
            t("mem.zram_ram_used"),
            memory_text_pref(ram, use_bytes, use_base2)
        ));
    }
    if let Some(ratio) = swap.zram_compression_ratio {
        label.push_str(&format!(
            "   ·   {} {ratio:.1}:1",
            t("mem.compression_ratio")
        ));
        if let (Some(original), Some(compressed)) =
            (swap.zram_original_bytes, swap.zram_compressed_bytes)
        {
            label.push_str(&format!(
                " · {} {} → {} {}",
                t("mem.compression_original"),
                memory_text_pref(original, use_bytes, use_base2),
                t("mem.compression_compressed"),
                memory_text_pref(compressed, use_bytes, use_base2),
            ));
        }
    }
    if swap.zswap_on {
        label.push_str(&format!("   ·   {}", t("mem.zswap")));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            label,
            Style::new().fg(theme.fg_dim),
        ))),
        label_area,
    );

    frame.render_widget(Block::new().style(Style::new().bg(theme.shade)), bar_area);
    let used_frac = if swap.total_bytes == 0 {
        0.0
    } else {
        swap.used_bytes as f64 / swap.total_bytes as f64
    };
    let used_pct = (used_frac * 100.0).round() as u16;
    let free_pct = 100u16.saturating_sub(used_pct);
    let mut constraints = Vec::new();
    if used_pct > 0 {
        constraints.push(Constraint::Percentage(used_pct));
    }
    if free_pct > 0 {
        constraints.push(Constraint::Percentage(free_pct));
    }
    if constraints.is_empty() {
        return;
    }
    let cells = Layout::horizontal(&constraints).split(bar_area);
    let mut idx = 0;
    if used_pct > 0 {
        frame.render_widget(
            Block::new().style(Style::new().bg(theme.network)),
            cells[idx],
        );
        idx += 1;
    }
    if free_pct > 0 {
        frame.render_widget(
            Block::new().style(Style::new().bg(theme.fg_dim)),
            cells[idx],
        );
    }
}
