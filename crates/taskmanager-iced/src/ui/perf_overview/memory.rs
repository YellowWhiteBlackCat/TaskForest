//! Memory composition bar, legend, and swap visualization for the Iced Performance view.

use iced::Element;
use iced::Length;
use iced::widget::{column, container, row, text};
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::theme;

/// The memory-composition bar, legend, and swap bar for the Memory Performance
/// view. Renders the shared [`taskmanager_shell::memory`] breakdown; only the
/// color mapping and the bar widgets are iced-specific.
pub(super) fn memory_composition_block(
    memory: &MemoryMetrics,
    theme: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let observed = super::projection::MemoryObservation::from(memory);
    let total = observed.total_bytes.unwrap_or(0);
    let segments = taskmanager_shell::memory::memory_segments(memory);
    let nonzero: Vec<_> = segments.iter().copied().filter(|s| s.bytes > 0).collect();

    let readout = match (observed.used_bytes, observed.total_bytes) {
        (Some(used), Some(total)) => format!(
            "{} {} · {} {}",
            t("mem.in_use"),
            taskmanager_shell::presentation::bytes(used),
            taskmanager_shell::presentation::bytes(total),
            t("mem.total"),
        ),
        _ => taskmanager_shell::presentation::missing_value(),
    };
    let header = row!(
        text(t("mem.composition")).size(f32::from(tokens::FONT_14)),
        text(readout).size(f32::from(tokens::FONT_12))
    )
    .spacing(12);

    let bar = composition_bar(&segments, theme);
    let legend: Vec<Element<'static, Message, iced::Theme, iced::Renderer>> = nonzero
        .iter()
        .map(|seg| legend_row(*seg, total, theme))
        .collect();

    let mut block = column![header, bar, column(legend).spacing(4)].spacing(8);
    if let Some(swap) = taskmanager_shell::memory::swap_breakdown(memory) {
        block = block.push(swap_bar_view(&swap, theme));
    }
    if let Some(comp_card) = compression_card_view(memory, theme) {
        block = block.push(comp_card);
    }
    block.into()
}

/// One semantic segment kind → the resolved iced color. Every frontend derives
/// these from the same `taskmanager-theme` tokens, so the bar matches the
/// gpui/tui composition bar.
pub(crate) fn segment_color(
    kind: taskmanager_shell::memory::MemSegmentKind,
    theme: &taskmanager_theme::Theme,
) -> iced::Color {
    use taskmanager_shell::memory::MemSegmentKind;
    let token = match kind {
        MemSegmentKind::ZfsArc => {
            // Reclaimable like the page cache, dimmed so the ARC legend
            // entry never blurs into "Cache + Buffers" (gpui renders the
            // same tint).
            let mut arc = crate::theme_binding::color(theme.disk);
            arc.a = 0.55;
            return arc;
        }
        MemSegmentKind::Active | MemSegmentKind::InUse => theme.memory,
        MemSegmentKind::Inactive => theme.accent,
        MemSegmentKind::Cache => theme.disk,
        MemSegmentKind::Free | MemSegmentKind::Available => theme.fg_dim,
        MemSegmentKind::Other => theme.shade,
    };
    crate::theme_binding::color(token)
}

/// Horizontal stacked proportion bar: a shade track with one fill per non-zero
/// segment, sized by its share of the segment-byte sum (matching gpui's
/// share-sum normalization).
fn composition_bar(
    segments: &[taskmanager_shell::memory::MemSegment],
    theme: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let drawn: Vec<(iced::Color, u64)> = segments
        .iter()
        .copied()
        .filter(|s| s.bytes > 0)
        .map(|s| (segment_color(s.kind, theme), s.bytes))
        .collect();
    let sum: u64 = drawn.iter().map(|(_, bytes)| *bytes).sum();
    let track = crate::theme_binding::color(theme.shade);
    let mut bar = row![].height(Length::Fill).width(Length::Fill);
    for (color, bytes) in drawn {
        let portion = (((bytes as f64 / sum.max(1) as f64) * 1000.0).round() as u16).max(1);
        bar = bar.push(
            container(column![])
                .width(Length::FillPortion(portion))
                .height(Length::Fill)
                .style(move |_| theme::fill_style(color)),
        );
    }
    container(bar)
        .height(Length::Fixed(10.0))
        .style(move |_| theme::fill_style(track))
        .into()
}

/// One legend row: a colored swatch, the localized label, the percent share,
/// and the byte count.
fn legend_row(
    seg: taskmanager_shell::memory::MemSegment,
    total: u64,
    theme: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let color = segment_color(seg.kind, theme);
    let pct = if total == 0 {
        0.0
    } else {
        (seg.bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    };
    row!(
        container(column![])
            .width(Length::Fixed(10.0))
            .height(Length::Fixed(10.0))
            .style(move |_| theme::fill_style(color)),
        container(text(seg.label.to_string()).size(f32::from(tokens::FONT_12))).width(Length::Fill),
        text(format!("{pct:>4.0}%")).size(f32::from(tokens::FONT_12)),
        text(taskmanager_shell::presentation::bytes(seg.bytes)).size(f32::from(tokens::FONT_12)),
    )
    .spacing(8)
    .into()
}

/// The secondary swap bar: a label line ("Swap X / Y (NN%) · zram Z · zram
/// RAM used R · zswap on") and a two-segment used/free proportion bar
/// beneath it.
fn swap_bar_view(
    swap: &taskmanager_shell::memory::SwapBreakdown,
    theme: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let pct = if swap.total_bytes == 0 {
        0.0
    } else {
        swap.used_bytes as f64 / swap.total_bytes as f64 * 100.0
    };
    let mut label = format!(
        "{}  {} / {}  ({pct:.0}%)",
        t("mem.swap"),
        taskmanager_shell::presentation::bytes(swap.used_bytes),
        taskmanager_shell::presentation::bytes(swap.total_bytes),
    );
    if let Some(zram) = swap.zram_bytes.filter(|z| *z > 0) {
        label.push_str(&format!(
            "   ·   {} {}",
            t("mem.zram_swap"),
            taskmanager_shell::presentation::bytes(zram)
        ));
    }
    // The RAM the store actually consumes (`mm_stat` `mem_used_total`):
    // distinct from the swap-used view above and the compressed size below.
    if let Some(ram) = swap.zram_memory_used_bytes.filter(|v| *v > 0) {
        label.push_str(&format!(
            "   ·   {} {}",
            t("mem.zram_ram_used"),
            taskmanager_shell::presentation::bytes(ram)
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
                taskmanager_shell::presentation::bytes(original),
                t("mem.compression_compressed"),
                taskmanager_shell::presentation::bytes(compressed),
            ));
        }
    }
    if swap.zswap_on {
        label.push_str(&format!("   ·   {}", t("mem.zswap")));
    }

    let used_portion = if swap.total_bytes == 0 {
        0
    } else {
        ((swap.used_bytes as f64 / swap.total_bytes as f64) * 1000.0).round() as u16
    };
    let free_portion = 1000u16.saturating_sub(used_portion);
    let mut bar = row![].height(Length::Fill).width(Length::Fill);
    if used_portion > 0 {
        let used_color = crate::theme_binding::color(theme.network);
        bar = bar.push(
            container(column![])
                .width(Length::FillPortion(used_portion))
                .height(Length::Fill)
                .style(move |_| theme::fill_style(used_color)),
        );
    }
    if free_portion > 0 {
        let free_color = crate::theme_binding::color(theme.shade);
        bar = bar.push(
            container(column![])
                .width(Length::FillPortion(free_portion))
                .height(Length::Fill)
                .style(move |_| theme::fill_style(free_color)),
        );
    }
    column![
        text(label).size(f32::from(tokens::FONT_12)),
        container(bar).height(Length::Fixed(6.0))
    ]
    .spacing(4)
    .into()
}

/// Specialized memory compression and savings card for active ZRAM/OS memory compression.
pub(crate) fn compression_card_view(
    memory: &MemoryMetrics,
    theme_snapshot: &taskmanager_theme::Theme,
) -> Option<Element<'static, Message, iced::Theme, iced::Renderer>> {
    let observed = super::projection::MemoryObservation::from(memory);
    let comp_used = observed.compressed_memory_used_bytes;
    let swap_used = observed.compressed_swap_used_bytes;

    if comp_used.is_none() && swap_used.is_none() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(used) = comp_used {
        parts.push(format!(
            "{}: {}",
            t("mem.compressed"),
            taskmanager_shell::presentation::bytes(used)
        ));
    }
    if let Some(swap) = swap_used {
        parts.push(format!(
            "{}: {}",
            t("mem.zram_swap"),
            taskmanager_shell::presentation::bytes(swap)
        ));
    }
    // The RAM the store consumes, metadata included (`mm_stat`
    // `mem_used_total`) — the cost line the swap-used view cannot show.
    if let Some(ram) = observed
        .compressed_swap_memory_used_bytes
        .filter(|v| *v > 0)
    {
        parts.push(format!(
            "{}: {}",
            t("mem.zram_ram_used"),
            taskmanager_shell::presentation::bytes(ram)
        ));
    }
    if let (Some(original), Some(compressed), Some(ratio)) = (
        observed.compressed_swap_original_bytes,
        observed.compressed_swap_compressed_bytes,
        observed.compressed_swap_compression_ratio,
    ) {
        parts.push(format!(
            "{} {ratio:.1}:1 · {} {} → {} {}",
            t("mem.compression_ratio"),
            t("mem.compression_original"),
            taskmanager_shell::presentation::bytes(original),
            t("mem.compression_compressed"),
            taskmanager_shell::presentation::bytes(compressed),
        ));
    }
    if observed.compressed_swap_cache_enabled == Some(true) {
        parts.push(t("mem.zswap").to_string());
    }

    if parts.is_empty() {
        return None;
    }

    let text_content = parts.join("   ·   ");
    let bg_color = crate::theme_binding::color(theme_snapshot.shade);
    let border_color = crate::theme_binding::color(theme_snapshot.palette().border);
    let muted_color = theme::muted_text_color(theme_snapshot);

    Some(
        container(
            row![
                text(text_content)
                    .size(f32::from(tokens::FONT_11))
                    .style(move |_| text::Style {
                        color: Some(muted_color),
                    }),
            ]
            .padding([4, 8])
            .align_y(iced::Alignment::Center),
        )
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(bg_color)),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        })
        .into(),
    )
}
