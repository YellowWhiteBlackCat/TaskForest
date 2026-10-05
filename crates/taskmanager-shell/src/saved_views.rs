//! Toolkit-neutral saved process views and their canonical configuration projection.

use crate::{ProcessStatusFilter, SortCol};
use std::collections::HashSet;
use taskmanager_application::i18n::t;
use taskmanager_core::core::config::{
    ProcessViewPresetConfig, SavedViewTransferError, allocate_saved_view_ids,
    export_saved_views_document, import_saved_views_document, resolve_saved_view_import_names,
    saved_view_name_is_portable, unique_saved_view_name,
};
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

pub fn feedback_text(feedback: SavedViewTransferFeedback) -> String {
    match feedback {
        SavedViewTransferFeedback::ExportCopied => t("hint.copied").into(),
        SavedViewTransferFeedback::ExportFailed => t("saved_views.export_failed").into(),
        SavedViewTransferFeedback::Imported(summary) => t("saved_views.import_success")
            .replace("{count}", &summary.imported.to_string())
            .replace("{renamed}", &summary.renamed.to_string()),
        SavedViewTransferFeedback::ClipboardEmpty => t("saved_views.clipboard_empty").into(),
        SavedViewTransferFeedback::ImportInvalid => t("saved_views.import_invalid").into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedViewPreset {
    pub id: u64,
    pub name_key: Option<&'static str>,
    pub custom_name: String,
    pub built_in: bool,
    pub filter: ProcessStatusFilter,
    pub sort_col: SortCol,
    pub sort_asc: bool,
    pub hidden_cols: HashSet<SortCol>,
}

impl SavedViewPreset {
    #[must_use]
    pub fn built_in(
        id: u64,
        name_key: &'static str,
        filter: ProcessStatusFilter,
        sort_col: SortCol,
        sort_asc: bool,
    ) -> Self {
        Self {
            id,
            name_key: Some(name_key),
            custom_name: String::new(),
            built_in: true,
            filter,
            sort_col,
            sort_asc,
            hidden_cols: HashSet::new(),
        }
    }

    #[must_use]
    pub fn restored(
        name: String,
        filter: ProcessStatusFilter,
        sort_col: SortCol,
        sort_asc: bool,
        hidden_cols: HashSet<SortCol>,
    ) -> Self {
        Self {
            id: 0,
            name_key: None,
            custom_name: name,
            built_in: false,
            filter,
            sort_col,
            sort_asc,
            hidden_cols,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.name_key
            .map(t)
            .unwrap_or(self.custom_name.as_str())
            .to_string()
    }

    #[must_use]
    pub fn is_user_saved(&self) -> bool {
        !self.built_in && self.name_key.is_none()
    }

    #[must_use]
    pub fn user_name(&self) -> Option<&str> {
        self.is_user_saved().then_some(self.custom_name.as_str())
    }
}

#[must_use]
pub fn default_built_in_presets() -> Vec<SavedViewPreset> {
    vec![
        SavedViewPreset::built_in(
            1,
            "saved_views.cpu_hotspots",
            ProcessStatusFilter::All,
            SortCol::Cpu,
            false,
        ),
        SavedViewPreset::built_in(
            2,
            "saved_views.running_tree",
            ProcessStatusFilter::Running,
            SortCol::Cpu,
            false,
        ),
        SavedViewPreset::built_in(
            3,
            "saved_views.memory_heavy",
            ProcessStatusFilter::All,
            SortCol::Memory,
            false,
        ),
    ]
}

pub fn export_saved_views_json(
    presets: &[SavedViewPreset],
) -> Result<String, SavedViewTransferError> {
    let wire_presets: Vec<_> = presets
        .iter()
        .filter(|preset| preset.is_user_saved())
        .enumerate()
        .map(|(index, preset)| wire_from_preset(preset, index))
        .collect::<Result<_, _>>()?;
    export_saved_views_document(&wire_presets)
}

pub fn import_saved_views_json(
    existing: &mut Vec<SavedViewPreset>,
    next_id: &mut u64,
    json: &str,
) -> Result<SavedViewImportSummary, SavedViewTransferError> {
    let imported = import_saved_views_document(json)?
        .into_iter()
        .enumerate()
        .map(|(index, preset)| preset_from_wire(preset, index))
        .collect::<Result<Vec<_>, _>>()?;
    let imported_count = imported.len();

    // Names are resolved for the whole batch at once, so a document holding a
    // name twice still yields distinct presets.
    let names: HashSet<String> = existing.iter().map(SavedViewPreset::display_name).collect();
    let resolved = resolve_saved_view_import_names(
        &names,
        imported
            .iter()
            .map(|preset| preset.custom_name.clone())
            .collect(),
    )?;
    let renamed = resolved.renamed;
    let imported: Vec<SavedViewPreset> = imported
        .into_iter()
        .zip(resolved.names)
        .map(|(mut preset, name)| {
            preset.custom_name = name;
            preset
        })
        .collect();

    // Ids skip the live presets instead of trusting the caller's counter, so a
    // drifted counter can never collide two views.
    let occupied: HashSet<u64> = existing.iter().map(|preset| preset.id).collect();
    let allocation = allocate_saved_view_ids(&occupied, *next_id, imported.len())?;
    for (mut preset, id) in imported.into_iter().zip(allocation.ids) {
        preset.id = id;
        existing.push(preset);
    }
    *next_id = allocation.next_id;

    Ok(SavedViewImportSummary {
        imported: imported_count,
        renamed,
    })
}

fn wire_from_preset(
    preset: &SavedViewPreset,
    index: usize,
) -> Result<ProcessViewPresetConfig, SavedViewTransferError> {
    if preset.hidden_cols.contains(&SortCol::Name) {
        return Err(SavedViewTransferError::InvalidPreset { index });
    }
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
    mut preset: ProcessViewPresetConfig,
    index: usize,
) -> Result<SavedViewPreset, SavedViewTransferError> {
    // Canonicalize the previously published PSS spelling only at external ingress.
    if preset.sort == "PSS" {
        preset.sort = "MemoryPss".into();
    }
    for column in &mut preset.hidden_columns {
        if column == "PSS" {
            *column = "MemoryPss".into();
        }
    }
    if !saved_view_name_is_portable(&preset.name) {
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

pub fn filter_token(filter: ProcessStatusFilter) -> &'static str {
    match filter {
        ProcessStatusFilter::All => "All",
        ProcessStatusFilter::Running => "Running",
        ProcessStatusFilter::Sleeping => "Sleeping",
        ProcessStatusFilter::Stopped => "Stopped",
        ProcessStatusFilter::Zombie => "Zombie",
        ProcessStatusFilter::Other => "Other",
    }
}

pub fn filter_from_token(token: &str) -> Option<ProcessStatusFilter> {
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

pub fn sort_token(sort: SortCol) -> &'static str {
    match sort {
        SortCol::Name => "Name",
        SortCol::User => "User",
        SortCol::Pid => "PID",
        SortCol::Threads => "Threads",
        SortCol::StartTime => "StartTime",
        SortCol::State => "Status",
        SortCol::Cpu => "CPU",
        SortCol::Memory => "Memory",
        SortCol::Pss => "MemoryPss",
        SortCol::Swap => "Swap",
        SortCol::DiskRead => "DiskRead",
        SortCol::DiskWrite => "DiskWrite",
        SortCol::Network => "Network",
        SortCol::CpuTime => "CPUTime",
        SortCol::Fds => "FDs",
        SortCol::Nice => "Nice",
    }
}

pub fn sort_from_token(token: &str) -> Option<SortCol> {
    match token {
        "Name" => Some(SortCol::Name),
        "User" => Some(SortCol::User),
        "PID" => Some(SortCol::Pid),
        "Threads" => Some(SortCol::Threads),
        "StartTime" => Some(SortCol::StartTime),
        "Status" => Some(SortCol::State),
        "CPU" => Some(SortCol::Cpu),
        "Memory" => Some(SortCol::Memory),
        "MemoryPss" => Some(SortCol::Pss),
        "Swap" => Some(SortCol::Swap),
        "DiskRead" => Some(SortCol::DiskRead),
        "DiskWrite" => Some(SortCol::DiskWrite),
        "Network" => Some(SortCol::Network),
        "CPUTime" => Some(SortCol::CpuTime),
        "FDs" => Some(SortCol::Fds),
        "Nice" => Some(SortCol::Nice),
        _ => None,
    }
}

pub fn hidden_tokens(hidden: &HashSet<SortCol>) -> Vec<String> {
    let mut tokens: Vec<_> = hidden
        .iter()
        .copied()
        .map(sort_token)
        .map(str::to_string)
        .collect();
    tokens.sort();
    tokens
}

pub fn hidden_from_tokens(tokens: &[String]) -> Option<HashSet<SortCol>> {
    let mut cols = HashSet::with_capacity(tokens.len());
    for token in tokens {
        let column = sort_from_token(token)?;
        if column == SortCol::Name || !cols.insert(column) {
            return None;
        }
    }
    Some(cols)
}

pub fn preset_to_config(preset: &SavedViewPreset) -> Option<ProcessViewPresetConfig> {
    wire_from_preset(preset, 0).ok()
}

pub fn preset_from_config(config: &ProcessViewPresetConfig) -> Option<SavedViewPreset> {
    preset_from_wire(config.clone(), 0).ok()
}

/// User-created views stay immediately reachable before the built-in suggestions.
pub fn review_rows(presets: &[SavedViewPreset]) -> impl Iterator<Item = &SavedViewPreset> {
    presets
        .iter()
        .filter(|preset| !preset.built_in)
        .chain(presets.iter().filter(|preset| preset.built_in))
}

/// Preserve live identities across immutable configuration publications.
pub fn restore_saved_views(
    existing: &mut Vec<SavedViewPreset>,
    next_id: &mut u64,
    configs: &[ProcessViewPresetConfig],
) -> Result<(), SavedViewTransferError> {
    if existing
        .iter()
        .filter_map(preset_to_config)
        .collect::<Vec<_>>()
        == configs
    {
        return Ok(());
    }
    let mut restored: Vec<_> = configs.iter().filter_map(preset_from_config).collect();
    let occupied: HashSet<_> = existing.iter().map(|entry| entry.id).collect();
    let allocation = allocate_saved_view_ids(&occupied, *next_id, restored.len())?;
    let mut assigned = HashSet::new();
    for (entry, allocated) in restored.iter_mut().zip(allocation.ids) {
        entry.id = existing
            .iter()
            .find(|old| old.user_name() == entry.user_name() && !assigned.contains(&old.id))
            .map_or(allocated, |old| old.id);
        assigned.insert(entry.id);
    }
    existing.retain(|entry| entry.built_in);
    existing.extend(restored);
    *next_id = allocation.next_id;
    Ok(())
}

pub fn save_current_view(
    existing: &mut Vec<SavedViewPreset>,
    next_id: &mut u64,
    mut preset: SavedViewPreset,
) -> Result<u64, SavedViewTransferError> {
    let names: HashSet<_> = existing.iter().map(SavedViewPreset::display_name).collect();
    let name = unique_saved_view_name(&preset.custom_name, &names)?;
    preset.custom_name = name;
    // Validate the same vocabulary used by clipboard ingress before modifying state.
    let config = wire_from_preset(&preset, existing.len())?;
    let _ = preset_from_wire(config, existing.len())?;
    let occupied = existing.iter().map(|entry| entry.id).collect();
    let allocation = allocate_saved_view_ids(&occupied, *next_id, 1)?;
    let id = allocation
        .ids
        .first()
        .copied()
        .ok_or(SavedViewTransferError::IdSpaceExhausted)?;
    preset.id = id;
    existing.push(preset);
    *next_id = allocation.next_id;
    Ok(id)
}

#[cfg(test)]
#[path = "../tests/headless/saved_views_tests.rs"]
mod tests;
