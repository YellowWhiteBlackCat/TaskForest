//! Versioned, strict import/export for user-created process view presets.
//!
//! The transfer PROTOCOL — format tag, version, size ceilings, error
//! vocabulary, document shape, and the collision-renaming rules — is
//! core-owned: `taskmanager_core::core::config` is its single source, shared
//! with the Iced dashboard. This module only maps GPUI's local preset
//! vocabulary (`ProcessStatusFilter` / `SortCol` / hideable columns) through
//! the neutral [`ProcessViewPresetConfig`] and drives the core functions over
//! the dashboard state. It still owns no filesystem access: the GPUI surface
//! moves the resulting text through an injected boundary (the clipboard
//! today) without doing blocking file I/O during render.

use std::collections::HashSet;

use taskmanager_core::core::config::{
    ProcessViewPresetConfig, SavedViewTransferError, allocate_saved_view_ids,
    export_saved_views_document, import_saved_views_document, resolve_saved_view_import_names,
    saved_view_name_is_portable,
};

use crate::gpui_app::processes_view::rows::is_hideable;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_shell::SortCol;

use super::{DashboardState, SavedViewPreset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedViewImportSummary {
    pub imported: usize,
    pub renamed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedViewTransferFeedback {
    ExportCopied,
    ExportFailed,
    Imported(SavedViewImportSummary),
    ClipboardEmpty,
    ImportInvalid,
}

pub fn export_saved_views_json(
    dashboard: &DashboardState,
) -> Result<String, SavedViewTransferError> {
    let presets: Vec<_> = dashboard
        .saved_views
        .iter()
        .filter(|preset| preset.is_user_saved())
        .enumerate()
        .map(|(index, preset)| wire_from_preset(preset, index))
        .collect::<Result<_, _>>()?;
    export_saved_views_document(&presets)
}

/// Parse and append a transfer atomically. Existing presets, especially the
/// built-ins, are never replaced. Exact-name conflicts are renamed in import
/// order using `name (2)`, `name (3)`, and so on.
pub fn import_saved_views_json(
    dashboard: &mut DashboardState,
    json: &str,
) -> Result<SavedViewImportSummary, SavedViewTransferError> {
    // Core parses and validates the whole document before GPUI touches live
    // state; the frontend column/sort/filter vocabulary maps afterwards.
    let document = import_saved_views_document(json)?;
    let mut imported = document
        .into_iter()
        .enumerate()
        .map(|(index, preset)| preset_from_wire(preset, index))
        .collect::<Result<Vec<_>, _>>()?;

    let occupied_ids: HashSet<u64> = dashboard
        .saved_views
        .iter()
        .map(|preset| preset.id)
        .collect();
    let ids = allocate_saved_view_ids(&occupied_ids, dashboard.next_saved_view_id, imported.len())?;

    let occupied_names: HashSet<String> = dashboard
        .saved_views
        .iter()
        .map(SavedViewPreset::display_name)
        .collect();
    let requested = imported
        .iter()
        .map(|preset| preset.custom_name.clone())
        .collect();
    let names = resolve_saved_view_import_names(&occupied_names, requested)?;
    for (preset, name) in imported.iter_mut().zip(&names.names) {
        preset.custom_name = name.clone();
    }

    let imported_count = imported.len();
    for (mut preset, id) in imported.into_iter().zip(ids.ids) {
        preset.id = id;
        dashboard.saved_views.push(preset);
    }
    dashboard.next_saved_view_id = ids.next_id;
    Ok(SavedViewImportSummary {
        imported: imported_count,
        renamed: names.renamed,
    })
}

fn wire_from_preset(
    preset: &SavedViewPreset,
    index: usize,
) -> Result<ProcessViewPresetConfig, SavedViewTransferError> {
    let name = preset
        .user_name()
        .filter(|name| saved_view_name_is_portable(name))
        .ok_or(SavedViewTransferError::InvalidPreset { index })?;
    Ok(ProcessViewPresetConfig::new(
        name.to_string(),
        filter_token(preset.filter).to_string(),
        sort_token(preset.sort_col).to_string(),
        preset.sort_asc,
        hidden_tokens(&preset.hidden_cols),
    ))
}

fn preset_from_wire(
    preset: ProcessViewPresetConfig,
    index: usize,
) -> Result<SavedViewPreset, SavedViewTransferError> {
    Ok(SavedViewPreset::restored(
        preset.name,
        filter_from_token(&preset.filter).ok_or(SavedViewTransferError::InvalidPreset { index })?,
        sort_from_token(&preset.sort).ok_or(SavedViewTransferError::InvalidPreset { index })?,
        preset.sort_asc,
        hidden_from_tokens(&preset.hidden_columns)
            .ok_or(SavedViewTransferError::InvalidPreset { index })?,
    ))
}

pub(crate) fn preset_to_config(preset: &SavedViewPreset) -> Option<ProcessViewPresetConfig> {
    let name = preset.user_name()?;
    Some(ProcessViewPresetConfig::new(
        name.to_string(),
        filter_token(preset.filter).to_string(),
        sort_token(preset.sort_col).to_string(),
        preset.sort_asc,
        hidden_tokens(&preset.hidden_cols),
    ))
}

pub(crate) fn preset_from_config(config: &ProcessViewPresetConfig) -> Option<SavedViewPreset> {
    let name = config.name.trim();
    if !saved_view_name_is_portable(name) {
        return None;
    }
    Some(SavedViewPreset::restored(
        name.to_string(),
        filter_from_token(&config.filter)?,
        sort_from_token(&config.sort)?,
        config.sort_asc,
        hidden_from_tokens(&config.hidden_columns)?,
    ))
}

pub(crate) fn filter_token(filter: ProcessStatusFilter) -> &'static str {
    match filter {
        ProcessStatusFilter::All => "All",
        ProcessStatusFilter::Running => "Running",
        ProcessStatusFilter::Sleeping => "Sleeping",
        ProcessStatusFilter::Stopped => "Stopped",
        ProcessStatusFilter::Zombie => "Zombie",
        ProcessStatusFilter::Other => "Other",
    }
}

pub(crate) fn filter_from_token(token: &str) -> Option<ProcessStatusFilter> {
    match token {
        "All" => Some(ProcessStatusFilter::All),
        "Running" => Some(ProcessStatusFilter::Running),
        "Sleeping" => Some(ProcessStatusFilter::Sleeping),
        "Stopped" => Some(ProcessStatusFilter::Stopped),
        "Zombie" => Some(ProcessStatusFilter::Zombie),
        "Other" => Some(ProcessStatusFilter::Other),
        _ => None,
    }
}

pub(crate) fn sort_token(sort: SortCol) -> &'static str {
    match sort {
        SortCol::Name => "Name",
        SortCol::User => "User",
        SortCol::Pid => "PID",
        SortCol::Threads => "Threads",
        SortCol::StartTime => "StartTime",
        SortCol::State => "Status",
        SortCol::Cpu => "CPU",
        SortCol::Memory => "Memory",
        SortCol::Swap => "Swap",
        SortCol::DiskRead => "DiskRead",
        SortCol::DiskWrite => "DiskWrite",
        SortCol::CpuTime => "CPUTime",
        SortCol::Fds => "FDs",
        SortCol::Nice => "Nice",
        // The shell superset column GPUI never surfaces; the token keeps the
        // match exhaustive. `sort_from_token` refuses "PSS", so a persisted
        // layout can never round-trip it into GPUI.
        SortCol::Pss => "PSS",
    }
}

pub(crate) fn sort_from_token(token: &str) -> Option<SortCol> {
    match token {
        "Name" => Some(SortCol::Name),
        "User" => Some(SortCol::User),
        "PID" => Some(SortCol::Pid),
        "Threads" => Some(SortCol::Threads),
        "StartTime" => Some(SortCol::StartTime),
        "Status" => Some(SortCol::State),
        "CPU" => Some(SortCol::Cpu),
        "Memory" => Some(SortCol::Memory),
        "Swap" => Some(SortCol::Swap),
        "DiskRead" => Some(SortCol::DiskRead),
        "DiskWrite" => Some(SortCol::DiskWrite),
        "CPUTime" => Some(SortCol::CpuTime),
        "FDs" => Some(SortCol::Fds),
        "Nice" => Some(SortCol::Nice),
        _ => None,
    }
}

pub(crate) fn hidden_tokens(hidden: &HashSet<SortCol>) -> Vec<String> {
    let mut tokens: Vec<_> = hidden
        .iter()
        .filter(|column| is_hideable(**column))
        .map(|column| sort_token(*column).to_string())
        .collect();
    tokens.sort();
    tokens
}

/// All-or-nothing parsing prevents a newer/corrupt preset from being silently
/// restored with different column semantics. Duplicate columns are invalid too.
pub(crate) fn hidden_from_tokens(tokens: &[String]) -> Option<HashSet<SortCol>> {
    let mut hidden = HashSet::with_capacity(tokens.len());
    for token in tokens {
        let column = sort_from_token(token).filter(|column| is_hideable(*column))?;
        if !hidden.insert(column) {
            return None;
        }
    }
    Some(hidden)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_dashboard_saved_view_transfer_tests.rs"]
mod tests;
