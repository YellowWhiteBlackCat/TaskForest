//! Strict Config ↔ GPUI mappings for process state and user-saved views.

use std::collections::HashMap;

use gpui::Pixels;

use super::graph_options::normalize_graph_data_points;
use super::{PROC_COL_MAX_WIDTH, PROC_COL_MIN_WIDTH, RootView, page_token};
use crate::gpui_app::dashboard::DashboardState;
use crate::gpui_app::processes_view::rows::is_resizable;
use taskforest_core::core::config::{ColumnWidthConfig, Config, ProcessViewPresetConfig};
use taskmanager_shell::SortCol;
use taskmanager_shell::SortDir;
use taskmanager_shell::saved_views::{
    hidden_from_tokens, hidden_tokens, preset_to_config, restore_saved_views, sort_from_token,
    sort_token,
};
use taskmanager_theme::Skin;

fn saved_views_to_config(dashboard: &DashboardState) -> Vec<ProcessViewPresetConfig> {
    dashboard
        .saved_views
        .iter()
        .filter_map(preset_to_config)
        .collect()
}

/// Serialize the live `col_widths` map to the opaque-token config form
/// ([`ColumnWidthConfig`]). Only resizable columns are emitted — the resize
/// handle is never mounted on `Name` so it can never reach this map, but the
/// `is_resizable` filter is defensive against a future caller inserting it.
/// Output is sorted by token so an unchanged layout serializes byte-identically
/// across runs (HashMap iteration order is random), mirroring `hidden_tokens`.
fn col_widths_to_config(widths: &HashMap<SortCol, Pixels>) -> Vec<ColumnWidthConfig> {
    let mut entries: Vec<_> = widths
        .iter()
        .filter(|(col, _)| is_resizable(**col))
        .map(|(col, px)| ColumnWidthConfig {
            column: sort_token(*col).to_string(),
            width: f32::from(*px),
        })
        .collect();
    entries.sort_by(|a, b| a.column.cmp(&b.column));
    entries
}

/// Parse persisted column widths back into the live map. Graceful on every
/// failure class: unknown column tokens, non-resizable columns (e.g. a stale
/// `Name` entry), non-finite widths, below-floor slivers, and duplicate tokens
/// are dropped individually — never a panic, never fabricated state. A
/// below-floor width drops to that column's built-in `default_width` (the entry
/// is skipped, so the empty-map default path applies); an oversized width
/// clamps to [`PROC_COL_MAX_WIDTH`], mirroring the drag clamp's bounds so a
/// hand-edited config cannot blow out the table.
fn col_widths_from_config(configs: &[ColumnWidthConfig]) -> HashMap<SortCol, Pixels> {
    let mut widths = HashMap::new();
    for entry in configs {
        let Some(col) = sort_from_token(&entry.column).filter(|c| is_resizable(*c)) else {
            continue;
        };
        if !entry.width.is_finite() || entry.width < PROC_COL_MIN_WIDTH {
            continue;
        }
        let clamped = entry.width.min(PROC_COL_MAX_WIDTH);
        // First occurrence wins on a duplicate token (the save side is already
        // deduplicated + sorted, so this only matters for a hand-edited file).
        widths.entry(col).or_insert(Pixels::from(clamped));
    }
    widths
}

pub(super) fn apply_process_config(view: &mut RootView, config: &Config) {
    if let Some(sort) = sort_from_token(&config.process_sort_col) {
        // Absolute restore at the persistence edge (the interactive click
        // conventions stay in the shell reducer).
        view.set_process_sort(
            sort,
            if config.process_sort_asc {
                SortDir::Asc
            } else {
                SortDir::Desc
            },
        );
    }
    if let Some(hidden) = hidden_from_tokens(&config.process_hidden_columns)
        && (config.process_hidden_columns_configured || !hidden.is_empty())
    {
        // Current writers distinguish an explicit empty set (show every
        // column) from an old payload that never recorded this axis.
        view.processes_state.hidden_cols = hidden;
    }
    // Restore user-resized column widths. An empty / old / corrupt
    // `process_col_widths` yields an empty map → every column falls back to
    // its built-in `default_width` (the pre-persistence byte-identical
    // layout), so a missing or hand-edited field never fabricates widths.
    view.processes_state.col_widths = col_widths_from_config(&config.process_col_widths);
    let _ = restore_saved_views(
        &mut view.dashboard.saved_views,
        &mut view.dashboard.next_saved_view_id,
        &config.saved_process_views,
    );
    // The GPUI frontend does not consume the motion preference this wave; it
    // retains the loaded token verbatim so its periodic saves echo — never
    // clobber — a value another frontend (or a hand edit) recorded.
    view.motion_token = config.motion.clone();
}

pub(super) fn config_from_view(view: &RootView) -> Config {
    let presentation = view.presentation_snapshot();
    let appearance = presentation.appearance;
    let devices = presentation.devices;
    let units = presentation.units;
    let graphs = presentation.graphs;
    let sidebar = presentation.sidebar;
    let refresh_ms = u64::try_from(
        view.telemetry_refresh_policy
            .interval()
            .duration()
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    // The persisted font tokens come from the theme owner's codec: a bundled
    // choice writes its own family and the host choice writes the explicit
    // marker, so one config file reads back as the same faces in every product.
    let (ui_font, mono_font) = appearance.font.config_tokens();
    Config {
        // Empty means "follow native desktop family". Persisting the
        // currently resolved skin here would turn an automatic first launch
        // into an accidental explicit override on the next restart.
        skin: appearance
            .skin
            .map(Skin::config_token)
            .unwrap_or_default()
            .to_string(),
        // Persist the preference token, not only the currently resolved
        // palette. `System` must remain System across restarts; old files with
        // Light/Dark remain valid explicit overrides.
        mode: appearance.color_scheme.to_string(),
        hc: appearance.high_contrast,
        ui_font: ui_font.to_string(),
        mono_font: mono_font.to_string(),
        density: appearance.density.config_token().to_string(),
        ui_size: appearance.ui_size.config_token().to_string(),
        text_rendering: appearance.text_rendering.to_string(),
        startup_page: presentation.startup_page.to_string(),
        // Persist the normalized window-frame token. System keeps the empty
        // sentinel so an untouched preference round-trips byte-identically
        // with pre-preference config files.
        window_decorations: presentation.window_decorations.to_string(),
        show_cpu: devices.cpu,
        show_memory: devices.memory,
        show_disks: devices.disks,
        show_network: devices.network,
        show_network_wired: devices.network_wired,
        show_network_wireless: devices.network_wireless,
        show_network_vpn: devices.network_vpn,
        show_network_virtual: devices.network_virtual,
        show_network_other: devices.network_other,
        show_gpus: devices.gpus,
        memory_use_bytes: units.memory_use_bytes,
        memory_use_base2: units.memory_use_base2,
        drive_use_bytes: units.drive_use_bytes,
        drive_use_base2: units.drive_use_base2,
        network_use_bytes: units.network_use_bytes,
        network_use_base2: units.network_use_base2,
        graph_data_points: normalize_graph_data_points(graphs.data_points),
        sliding_graphs: graphs.sliding,
        network_dynamic_scaling: graphs.network_dynamic_scaling,
        sidebar_order: sidebar.order,
        sidebar_device_overrides: sidebar.device_overrides,
        gray_zero_values: presentation.gray_zero_values,
        notify_enabled: view.projection().alert_center.policy().enabled,
        notify_quiet_hours: view
            .projection()
            .alert_center
            .policy()
            .quiet_hours
            .map(|hours| (hours.start_minutes, hours.end_minutes)),
        refresh_ms,
        last_page: page_token(view.page).to_string(),
        process_sort_col: sort_token(view.process_sort().0).to_string(),
        process_sort_asc: matches!(view.process_sort().1, SortDir::Asc),
        process_hidden_columns: hidden_tokens(&view.processes_state.hidden_cols),
        process_hidden_columns_configured: true,
        process_col_widths: col_widths_to_config(&view.processes_state.col_widths),
        sidebar_width: f32::from(sidebar.width),
        saved_process_views: saved_views_to_config(&view.dashboard),
        // Persist only an explicit user choice. With no choice, the next
        // startup may follow the host's native locale instead of freezing a
        // transient auto-detected language into the config file.
        language: appearance
            .language
            .map(|language| language.code().to_owned()),
        // Roadmap #4: no Settings toggle exists yet, so the writer echoes the
        // flag this run loaded — a manually enabled config.json stays enabled
        // across the periodic saves instead of being clobbered back to false.
        history_persistence: view.history_runtime.enabled_next_start(),
        // Same echo discipline for the motion preference: GPUI renders no
        // motion switch this wave, so the writer returns the loaded token
        // instead of freezing Normal over a recorded Reduced/None choice.
        motion: view.motion_token.clone(),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_persistence_tests.rs"]
mod tests;
