//! Versioned, strict import/export for user-created process view presets.
//!
//! This module deliberately owns JSON conversion but no filesystem access. The
//! GPUI surface can move the resulting text through an injected boundary (the
//! clipboard today) without doing blocking file I/O during render.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::gpui_app::processes_view::rows::{SortCol, is_hideable};
use taskmanager_application::ProcessViewPresetConfig;
use taskmanager_shell::ProcessStatusFilter;

use super::{DashboardState, SavedViewPreset};

pub const SAVED_VIEW_TRANSFER_FORMAT: &str = "taskmanager.saved-process-views";
pub const SAVED_VIEW_TRANSFER_VERSION: u64 = 1;

const MAX_TRANSFER_BYTES: usize = 1_048_576;
const MAX_TRANSFER_PRESETS: usize = 1_000;
const MAX_PRESET_NAME_CHARS: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedViewTransferError {
    TooLarge,
    InvalidDocument,
    UnsupportedFormat,
    UnsupportedVersion { found: u64 },
    TooManyPresets,
    InvalidPreset { index: usize },
    IdSpaceExhausted,
    NameSpaceExhausted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransferDocument {
    format: String,
    version: u64,
    presets: Vec<ProcessViewPresetConfig>,
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
    if presets.len() > MAX_TRANSFER_PRESETS {
        return Err(SavedViewTransferError::TooManyPresets);
    }
    serde_json::to_string_pretty(&TransferDocument {
        format: SAVED_VIEW_TRANSFER_FORMAT.to_string(),
        version: SAVED_VIEW_TRANSFER_VERSION,
        presets,
    })
    .map_err(|_| SavedViewTransferError::InvalidDocument)
}

/// Parse and append a transfer atomically. Existing presets, especially the
/// built-ins, are never replaced. Exact-name conflicts are renamed in import
/// order using `name (2)`, `name (3)`, and so on.
pub fn import_saved_views_json(
    dashboard: &mut DashboardState,
    json: &str,
) -> Result<SavedViewImportSummary, SavedViewTransferError> {
    if json.len() > MAX_TRANSFER_BYTES {
        return Err(SavedViewTransferError::TooLarge);
    }
    let document: TransferDocument =
        serde_json::from_str(json).map_err(|_| SavedViewTransferError::InvalidDocument)?;
    if document.format != SAVED_VIEW_TRANSFER_FORMAT {
        return Err(SavedViewTransferError::UnsupportedFormat);
    }
    if document.version != SAVED_VIEW_TRANSFER_VERSION {
        return Err(SavedViewTransferError::UnsupportedVersion {
            found: document.version,
        });
    }
    if document.presets.len() > MAX_TRANSFER_PRESETS {
        return Err(SavedViewTransferError::TooManyPresets);
    }

    // Validate the complete payload before changing live state.
    let mut imported = document
        .presets
        .into_iter()
        .enumerate()
        .map(|(index, preset)| preset_from_wire(preset, index))
        .collect::<Result<Vec<_>, _>>()?;
    let ids = allocate_ids(dashboard, imported.len())?;

    let mut names: HashSet<String> = dashboard
        .saved_views
        .iter()
        .map(SavedViewPreset::display_name)
        .collect();
    let mut renamed = 0;
    for preset in &mut imported {
        let original = preset.custom_name.clone();
        let unique = unique_name(&original, &names)?;
        if unique != original {
            renamed += 1;
            preset.custom_name = unique;
        }
        names.insert(preset.custom_name.clone());
    }

    let imported_count = imported.len();
    for (mut preset, id) in imported.into_iter().zip(ids.ids) {
        preset.id = id;
        dashboard.saved_views.push(preset);
    }
    dashboard.next_saved_view_id = ids.next_id;
    Ok(SavedViewImportSummary {
        imported: imported_count,
        renamed,
    })
}

struct AllocatedIds {
    ids: Vec<u64>,
    next_id: u64,
}

fn allocate_ids(
    dashboard: &DashboardState,
    count: usize,
) -> Result<AllocatedIds, SavedViewTransferError> {
    let occupied: HashSet<u64> = dashboard
        .saved_views
        .iter()
        .map(|preset| preset.id)
        .collect();
    let mut ids = Vec::with_capacity(count);
    let mut candidate = dashboard.next_saved_view_id;
    while ids.len() < count {
        if !occupied.contains(&candidate) {
            ids.push(candidate);
        }
        candidate = candidate
            .checked_add(1)
            .ok_or(SavedViewTransferError::IdSpaceExhausted)?;
    }
    Ok(AllocatedIds {
        ids,
        next_id: candidate,
    })
}

fn unique_name(base: &str, occupied: &HashSet<String>) -> Result<String, SavedViewTransferError> {
    if !occupied.contains(base) {
        return Ok(base.to_string());
    }
    // There are `occupied.len()` names, so testing one more suffix than that
    // must find a free candidate (pigeonhole principle).
    for offset in 0..=occupied.len() {
        let index = u64::try_from(offset)
            .ok()
            .and_then(|offset| 2_u64.checked_add(offset))
            .ok_or(SavedViewTransferError::NameSpaceExhausted)?;
        let suffix = format!(" ({index})");
        let stem_len = MAX_PRESET_NAME_CHARS.saturating_sub(suffix.chars().count());
        let stem: String = base.chars().take(stem_len).collect();
        let candidate = format!("{stem}{suffix}");
        if !occupied.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(SavedViewTransferError::NameSpaceExhausted)
}

fn valid_transfer_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= MAX_PRESET_NAME_CHARS
        && !name.chars().any(char::is_control)
}

fn wire_from_preset(
    preset: &SavedViewPreset,
    index: usize,
) -> Result<ProcessViewPresetConfig, SavedViewTransferError> {
    let name = preset
        .user_name()
        .filter(|name| valid_transfer_name(name))
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
    if !valid_transfer_name(&preset.name) {
        return Err(SavedViewTransferError::InvalidPreset { index });
    }
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
    if !valid_transfer_name(name) {
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
