//! NPU hardware accelerator detail section for the TUI Performance page.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use taskmanager_application::i18n::t;
use taskmanager_core::core::npu::{NpuEngineKind, NpuInventorySnapshot};
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::{TuiApp, TuiTheme};

pub(super) fn render_npu_section(
    frame: &mut Frame<'_>,
    _app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    inventory: Option<&NpuInventorySnapshot>,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let Some(inventory) = inventory else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(format!(" {} ", t("npu.title")));
        let text = Line::from(vec![Span::styled(
            t("common.collecting_telemetry"),
            Style::default().fg(theme.dim),
        )]);
        frame.render_widget(Paragraph::new(vec![text]).block(block), area);
        return;
    };

    if !inventory.is_success() || inventory.devices.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(format!(" {} ", t("npu.title")));
        let text = Line::from(vec![Span::styled(
            t("npu.engine_unknown"),
            Style::default().fg(theme.dim),
        )]);
        frame.render_widget(Paragraph::new(vec![text]).block(block), area);
        return;
    }

    let mut lines = Vec::new();
    for (index, device) in inventory.devices.iter().enumerate() {
        let title = format!(
            "{} {} · {}",
            t("npu.title"),
            index,
            device.device_id.as_str()
        );
        lines.push(Line::from(vec![Span::styled(
            title,
            Style::default().fg(theme.accent),
        )]));
        if let Some(brand) = &device.brand {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", t("npu.device_title")),
                    Style::default().fg(theme.dim),
                ),
                Span::raw(brand.clone()),
            ]));
        }
        if let Some(driver) = &device.driver {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", t("common.driver")),
                    Style::default().fg(theme.dim),
                ),
                Span::raw(driver.clone()),
            ]));
        }
        let util_text = device
            .utilization_pct
            .current_value()
            .map_or_else(missing_value, |pct| format!("{pct:>5.1}%"));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}: ", t("common.utilization")),
                Style::default().fg(theme.dim),
            ),
            Span::styled(util_text, Style::default().fg(theme.accent)),
        ]));

        let ded_bytes = device
            .memory
            .dedicated_total_bytes
            .current_value()
            .copied()
            .map_or_else(missing_value, bytes);
        let shared_bytes = device
            .memory
            .shared_total_bytes
            .current_value()
            .copied()
            .map_or_else(missing_value, bytes);
        let sram_bytes = device
            .memory
            .sram_total_bytes
            .current_value()
            .copied()
            .map_or_else(missing_value, bytes);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}: ", t("npu.dedicated_memory")),
                Style::default().fg(theme.dim),
            ),
            Span::raw(ded_bytes),
            Span::styled(
                format!(" · {}: ", t("npu.shared_memory")),
                Style::default().fg(theme.dim),
            ),
            Span::raw(shared_bytes),
            Span::styled(
                format!(" · {}: ", t("npu.sram")),
                Style::default().fg(theme.dim),
            ),
            Span::raw(sram_bytes),
        ]));

        if !device.engines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}:", t("gpu.per_engine_title")),
                Style::default().fg(theme.dim),
            )]));
            for engine in &device.engines {
                let label = match engine.kind {
                    NpuEngineKind::Compute => t("npu.engine_compute"),
                    NpuEngineKind::Matrix => t("npu.engine_matrix"),
                    NpuEngineKind::Vector => t("npu.engine_vector"),
                    NpuEngineKind::Video => t("npu.engine_video"),
                    NpuEngineKind::Copy => t("npu.engine_copy"),
                    NpuEngineKind::Unknown => t("npu.engine_unknown"),
                };
                let eng_pct = engine
                    .utilization_pct
                    .current_value()
                    .map_or_else(missing_value, |pct| format!("{pct:>5.1}%"));
                lines.push(Line::from(vec![
                    Span::styled(format!("    {label}: "), Style::default().fg(theme.dim)),
                    Span::raw(eng_pct),
                ]));
            }
        }
        lines.push(Line::default());
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(format!(" {} ", t("npu.title")));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
