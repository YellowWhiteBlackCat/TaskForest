//! Iced-native CPU-affinity editor.
//!
//! The semantic contract is shared with the shell and GPUI, but this surface
//! owns its own geometry: a bounded wrapped chip grid, Iced focus IDs, and the
//! modal's local loading/failure projection.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::ProcessAffinityState;
use taskmanager_application::i18n::t;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::{IcedApp, focus, theme};

/// Render the process-affinity editor from the frozen identity and the latest
/// correlated shell read. A missing or failed read never becomes a guessed
/// all-CPU mask.
pub(super) fn render(app: &IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let logical_cpu_count = app.logical_cpu_count();
    let target = app.affinity_target();
    let selected_count = app
        .process_presentation
        .affinity_cpus
        .as_ref()
        .map_or(0, |cpus| {
            cpus.iter()
                .filter(|cpu| {
                    usize::try_from(**cpu)
                        .ok()
                        .is_some_and(|index| index < logical_cpu_count)
                })
                .count()
        });

    let target_header: Element<'_, Message, iced::Theme, iced::Renderer> = target
        .map(|target| {
            row![
                text(if target.name.trim().is_empty() {
                    t("proc.unknown_process").to_owned()
                } else {
                    target.name.clone()
                })
                .size(f32::from(tokens::FONT_15)),
                text(format!("PID {}", target.pid))
                    .size(f32::from(tokens::FONT_12))
                    .color(theme::muted_text_color(theme_snapshot)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into()
        })
        .unwrap_or_else(|| text(t("empty.no_process_selected")).into());

    let failure = match app.shell.process_affinity_state() {
        ProcessAffinityState::Failed { failure, .. } => Some(*failure),
        _ => None,
    };
    let status = if let Some(failure) = failure {
        format!("Current affinity unavailable: {failure:?}")
    } else if app.process_presentation.affinity_cpus.is_none() {
        t("common.collecting_telemetry").to_owned()
    } else {
        format!(
            "{selected_count} / {logical_cpu_count} {}",
            t("proc.logical_cpus")
        )
    };

    let mask_str = if let Some(cpus) = app.process_presentation.affinity_cpus.as_ref() {
        let mut mask: u128 = 0;
        for cpu in cpus {
            if *cpu < 128 {
                mask |= 1u128 << *cpu;
            }
        }
        if logical_cpu_count <= 32 {
            format!("0x{:08X}", mask as u32)
        } else if logical_cpu_count <= 64 {
            format!("0x{:016X}", mask as u64)
        } else {
            format!("0x{:032X}", mask)
        }
    } else {
        taskmanager_shell::presentation::missing_value()
    };

    let preset_bar: Element<'_, Message, iced::Theme, iced::Renderer> =
        if app.process_presentation.affinity_cpus.is_some() && failure.is_none() {
            let mut presets = vec![
                focus::choice_pill(
                    theme_snapshot,
                    FocusTarget::ProcessAffinitySelectAll,
                    t("common.all").to_string(),
                    false,
                    Message::SelectAllProcessAffinity,
                ),
                focus::choice_pill(
                    theme_snapshot,
                    FocusTarget::ProcessAffinityClearAll,
                    t("common.none").to_string(),
                    false,
                    Message::ClearAllProcessAffinity,
                ),
                focus::choice_pill(
                    theme_snapshot,
                    FocusTarget::ProcessAffinityInvert,
                    t("common.invert").to_string(),
                    false,
                    Message::InvertProcessAffinity,
                ),
            ];
            if app
                .shell
                .projection()
                .hardware
                .as_ref()
                .is_some_and(|hw| hw.core_breakdown.total() > 0)
            {
                presets.push(focus::choice_pill(
                    theme_snapshot,
                    FocusTarget::ProcessAffinityPCores,
                    t("cpu.p_core").to_string(),
                    false,
                    Message::SelectProcessAffinityPCores,
                ));
                presets.push(focus::choice_pill(
                    theme_snapshot,
                    FocusTarget::ProcessAffinityECores,
                    t("cpu.e_core").to_string(),
                    false,
                    Message::SelectProcessAffinityECores,
                ));
            }
            row(presets).spacing(6).into()
        } else {
            column![].into()
        };

    let grid: Element<'_, Message, iced::Theme, iced::Renderer> =
        if app.process_presentation.affinity_cpus.is_some() && failure.is_none() {
            affinity_grid(app, theme_snapshot, logical_cpu_count)
        } else {
            container(text(status.clone()).size(f32::from(tokens::FONT_13)))
                .style(move |_| theme::panel_style(theme_snapshot))
                .padding(12)
                .width(Length::Fill)
                .into()
        };

    let apply: Element<'_, Message, iced::Theme, iced::Renderer> =
        if app.process_presentation.affinity_cpus.is_some() && failure.is_none() {
            focus::button(
                theme_snapshot,
                FocusTarget::ProcessAffinityApply,
                t("common.apply"),
                Message::ApplyProcessAffinity,
                false,
            )
        } else {
            column![].into()
        };

    let hex_readout = text(format!("{} {mask_str}", t("proc.affinity_mask")))
        .size(f32::from(tokens::FONT_12))
        .color(theme::muted_text_color(theme_snapshot));

    super::overlays::modal_overlay(
        theme_snapshot,
        t("dialog.cpu_affinity"),
        "Frozen process identity · choose logical CPUs · Esc closes",
        column![
            target_header,
            row![text(status).size(f32::from(tokens::FONT_12)), hex_readout]
                .spacing(16)
                .align_y(iced::Alignment::Center),
            preset_bar,
            scrollable(grid).height(Length::Fixed(320.0)),
            row![apply].width(Length::Fill),
        ]
        .spacing(8)
        .into(),
        app.modal_appear_progress(),
    )
}

fn affinity_grid<'a>(
    app: &'a IcedApp,
    theme_snapshot: &'a Theme,
    logical_cpu_count: usize,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    const COLUMNS: usize = 8;
    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
    let mut current: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();

    for cpu in 0..logical_cpu_count {
        let Ok(cpu_id) = u32::try_from(cpu) else {
            break;
        };
        let selected = app
            .process_presentation
            .affinity_cpus
            .as_ref()
            .is_some_and(|cpus| cpus.contains(&cpu_id));
        current.push(focus::choice_pill(
            theme_snapshot,
            FocusTarget::ProcessAffinityCpu(cpu_id),
            format!("CPU {cpu_id}"),
            selected,
            Message::ToggleProcessAffinityCpu(cpu_id),
        ));
        if current.len() == COLUMNS {
            rows.push(row(std::mem::take(&mut current)).spacing(4).into());
        }
    }
    if !current.is_empty() {
        rows.push(row(current).spacing(4).into());
    }

    column(rows).spacing(4).width(Length::Fill).into()
}
