//! Network, GPU, Battery, and Fan statistics rail cards for Performance.

use super::device_stats::{capped_stats_rows, stat_text_row};
use crate::pages::performance::metrics::{
    battery_stat_rows, fan_stat_rows, gpu_stat_rows, network_stat_rows,
};
use crate::palette::UiPalette;
use bevy::scene::Scene;
use taskmanager_shell::ShellApp;

pub(super) fn network_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows = network_stat_rows(shell)
        .into_iter()
        .map(|row| stat_text_row(row.label, row.value, palette))
        .collect();
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}

pub(super) fn gpu_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows = gpu_stat_rows(shell)
        .into_iter()
        .map(|row| stat_text_row(row.label, row.value, palette))
        .collect();
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}

pub(super) fn battery_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows = battery_stat_rows(shell)
        .into_iter()
        .map(|row| stat_text_row(row.label, row.value, palette))
        .collect();
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}

pub(super) fn fan_stats_card(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let rows = fan_stat_rows(shell)
        .into_iter()
        .map(|row| stat_text_row(row.label, row.value, palette))
        .collect();
    Box::new(super::summary_card_scene(
        capped_stats_rows(rows, palette),
        palette,
    ))
}
