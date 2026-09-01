//! Saved process view presets and clipboard JSON transfer for Iced.
//!
//! The transfer protocol itself (format tag, version, size ceilings, the
//! strict error vocabulary, and the document shape) is owned by
//! `taskmanager-core`'s saved-view transfer contract; this module only maps
//! the local [`SavedViewPreset`] through the neutral
//! [`ProcessViewPresetConfig`], keeping the filter/sort/column token
//! vocabulary on the frontend side. It also renders the ribbon widget for
//! one-click preset switching.

use std::collections::HashSet;

use iced::widget::{container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::config::{
    ProcessViewPresetConfig, SavedViewTransferError, allocate_saved_view_ids,
    export_saved_views_document, import_saved_views_document, resolve_saved_view_import_names,
    saved_view_name_is_portable,
};
use taskmanager_shell::SortCol;
use taskmanager_theme::tokens;

use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_shell::ProcessStatusFilter;

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
        SortCol::Pss => "PSS",
        SortCol::Swap => "Swap",
        SortCol::DiskRead => "DiskRead",
        SortCol::DiskWrite => "DiskWrite",
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
        "PSS" => Some(SortCol::Pss),
        "Swap" => Some(SortCol::Swap),
        "DiskRead" => Some(SortCol::DiskRead),
        "DiskWrite" => Some(SortCol::DiskWrite),
        "CPUTime" => Some(SortCol::CpuTime),
        "FDs" => Some(SortCol::Fds),
        "Nice" => Some(SortCol::Nice),
        _ => None,
    }
}

fn hidden_tokens(hidden: &HashSet<SortCol>) -> Vec<String> {
    let mut tokens: Vec<_> = hidden
        .iter()
        .copied()
        .map(sort_token)
        .map(str::to_string)
        .collect();
    tokens.sort();
    tokens
}

fn hidden_from_tokens(tokens: &[String]) -> Option<HashSet<SortCol>> {
    let mut cols = HashSet::with_capacity(tokens.len());
    for token in tokens {
        cols.insert(sort_from_token(token)?);
    }
    Some(cols)
}

#[derive(Clone, Copy, Debug)]
pub struct PresetsRibbonState {
    pub filter: ProcessStatusFilter,
    pub sort: SortCol,
    pub ascending: bool,
    pub feedback: Option<SavedViewTransferFeedback>,
    pub compact: bool,
}

/// Render the Presets Ribbon bar above the process table.
pub fn presets_ribbon<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    presets: &'a [SavedViewPreset],
    state: PresetsRibbonState,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        text(t("saved_views.title"))
            .size(f32::from(tokens::FONT_12))
            .color(theme::muted_text_color(theme_snapshot))
            .into(),
    ];

    for preset in presets {
        let is_active = preset.filter == state.filter
            && preset.sort_col == state.sort
            && preset.sort_asc == state.ascending;

        let btn = focus::choice_pill(
            theme_snapshot,
            FocusTarget::SavedViewPreset(preset.id),
            preset.display_name(),
            is_active,
            Message::ApplySavedView(preset.id),
        );
        items.push(btn);
    }

    // Save Current, Export, Import action buttons
    let save_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewSaveCurrent,
        t("saved_views.save_current").to_string(),
        Message::SaveCurrentProcessView,
        false,
    );
    let export_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewExport,
        t("common.export").to_string(),
        Message::ExportSavedViews,
        false,
    );
    let import_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewImport,
        t("common.import").to_string(),
        Message::ImportSavedViews,
        false,
    );

    items.extend([save_btn, export_btn, import_btn]);

    if let Some(fb) = state.feedback {
        let (msg, is_error) = match fb {
            SavedViewTransferFeedback::ExportCopied => {
                (t("first_run.copy_output").to_string(), false)
            }
            SavedViewTransferFeedback::ExportFailed => {
                (t("saved_views.export_failed").to_string(), true)
            }
            SavedViewTransferFeedback::Imported(s) => (
                t("saved_views.import_success")
                    .replace("{count}", &s.imported.to_string())
                    .replace("{renamed}", &s.renamed.to_string()),
                false,
            ),
            SavedViewTransferFeedback::ClipboardEmpty => {
                (t("saved_views.clipboard_empty").to_string(), true)
            }
            SavedViewTransferFeedback::ImportInvalid => {
                (t("saved_views.import_invalid").to_string(), true)
            }
        };
        let color = if is_error {
            crate::theme_binding::color(theme_snapshot.palette().danger)
        } else {
            crate::theme_binding::color(theme_snapshot.palette().accent)
        };
        items.push(
            text(msg)
                .size(f32::from(tokens::FONT_11))
                .color(color)
                .into(),
        );
    }

    if state.compact {
        // The preset names plus Save/Export/Import are wider than the compact
        // viewport. Wrap the finite action vocabulary into complete rows so
        // no button is presented as a clipped partial label.
        crate::ui::chunked_rows(items, 4)
    } else {
        container(row(items).spacing(6).align_y(iced::Alignment::Center))
            .padding([4, 8])
            .width(Length::Fill)
            .into()
    }
}

#[cfg(test)]
#[path = "../tests/gui/saved_views_tests.rs"]
mod tests;
