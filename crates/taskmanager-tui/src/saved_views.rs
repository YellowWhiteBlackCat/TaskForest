//! Terminal saved-view cache and native intents over the shared model.

use crate::{TuiApp, TuiSurface, TuiSurfaceKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use taskmanager_application::{AppAction, AppPage, i18n::t};
use taskmanager_core::core::config::Config;
use taskmanager_shell::saved_views::{
    SavedViewPreset, SavedViewTransferFeedback, default_built_in_presets, export_saved_views_json,
    import_saved_views_json, preset_to_config, restore_saved_views, review_rows, save_current_view,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, SortDir};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum SavedViewInput {
    #[default]
    Browsing,
    AwaitingImport,
}

#[derive(Debug)]
pub(crate) struct SavedViewsState {
    pub(crate) rows: Vec<SavedViewPreset>,
    pub(crate) next_id: u64,
    pub(crate) feedback: Option<SavedViewTransferFeedback>,
}
impl Default for SavedViewsState {
    fn default() -> Self {
        Self {
            rows: default_built_in_presets(),
            next_id: 4,
            feedback: None,
        }
    }
}

impl TuiApp {
    pub(crate) fn restore_saved_views_config(&mut self, config: &Config) {
        if let Err(error) = restore_saved_views(
            &mut self.saved_views.rows,
            &mut self.saved_views.next_id,
            &config.saved_process_views,
        ) {
            self.saved_view_notice(error);
        }
    }
    fn persist_saved_views(&mut self) {
        let mut config = self.config_draft.clone();
        config.saved_process_views = self
            .saved_views
            .rows
            .iter()
            .filter_map(preset_to_config)
            .collect();
        self.commit_config_draft(config);
    }
    pub(crate) fn open_saved_views(&mut self) {
        if self.local_surface_kind() == Some(TuiSurfaceKind::Settings) {
            self.cancel_settings();
        }
        self.open_local_surface(TuiSurface::SavedViews {
            selected: self.saved_views.rows.first().map(|entry| entry.id),
            input: SavedViewInput::Browsing,
        });
    }
    pub(crate) fn saved_view_notice(&mut self, error: impl std::fmt::Display) {
        self.report_notice(
            FeedbackSource::Settings,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::UntilReplaced,
            format!("Saved view operation: {error}"),
        );
    }
    pub(crate) fn handle_saved_views_key(&mut self, key: KeyEvent) -> Option<String> {
        let Some(TuiSurface::SavedViews { selected, .. }) = self.local_surface() else {
            return None;
        };
        let ids: Vec<_> = review_rows(&self.saved_views.rows)
            .map(|entry| entry.id)
            .collect();
        let index = selected.and_then(|id| ids.iter().position(|entry| *entry == id));
        match key.code {
            KeyCode::Esc => self.close_local_overlays(),
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                let index = match key.code {
                    KeyCode::Up => index.unwrap_or(0).saturating_sub(1),
                    KeyCode::Down => index
                        .unwrap_or(0)
                        .saturating_add(1)
                        .min(self.saved_views.rows.len().saturating_sub(1)),
                    KeyCode::End => self.saved_views.rows.len().saturating_sub(1),
                    _ => 0,
                };
                let target = ids.get(index).copied();
                if let Some(TuiSurface::SavedViews { selected, .. }) = self.local_surface_mut() {
                    *selected = target;
                }
            }
            KeyCode::Enter => {
                if let Some(preset) = selected
                    .and_then(|id| self.saved_views.rows.iter().find(|entry| entry.id == id))
                    .cloned()
                {
                    self.close_local_overlays();
                    let _ = self.apply_action(AppAction::SelectPage(AppPage::Applications));
                    self.shell.set_process_status_filter(preset.filter);
                    self.shell.set_sort_column(preset.sort_col);
                    self.shell.process_sort.1 = if preset.sort_asc {
                        SortDir::Asc
                    } else {
                        SortDir::Desc
                    };
                    self.hidden_columns = preset.hidden_cols;
                }
            }
            KeyCode::F(2) => {
                let preset = SavedViewPreset::restored(
                    t("saved_views.custom_name").replace(
                        "{index}",
                        &self.saved_views.next_id.saturating_sub(3).to_string(),
                    ),
                    self.shell.process_status_filter,
                    self.shell.process_sort.0,
                    self.shell.process_sort.1 == SortDir::Asc,
                    self.hidden_columns.clone(),
                );
                match save_current_view(
                    &mut self.saved_views.rows,
                    &mut self.saved_views.next_id,
                    preset,
                ) {
                    Ok(id) => {
                        self.persist_saved_views();
                        if let Some(TuiSurface::SavedViews { selected, .. }) =
                            self.local_surface_mut()
                        {
                            *selected = Some(id);
                        }
                    }
                    Err(error) => self.saved_view_notice(error),
                }
            }
            KeyCode::Delete => {
                if let Some(preset) = selected
                    .and_then(|id| self.saved_views.rows.iter().find(|entry| entry.id == id))
                    .filter(|entry| entry.is_user_saved())
                {
                    let id = preset.id;
                    self.saved_views.rows.retain(|entry| entry.id != id);
                    self.persist_saved_views();
                    let selected = self.saved_views.rows.last().map(|entry| entry.id);
                    if let Some(TuiSurface::SavedViews {
                        selected: target, ..
                    }) = self.local_surface_mut()
                    {
                        *target = selected;
                    }
                }
            }
            KeyCode::F(5) => match export_saved_views_json(&self.saved_views.rows) {
                Ok(json) => {
                    return Some(json);
                }
                Err(error) => {
                    self.saved_views.feedback = Some(SavedViewTransferFeedback::ExportFailed);
                    self.saved_view_notice(error);
                }
            },
            KeyCode::F(6) => {
                if let Some(TuiSurface::SavedViews { input, .. }) = self.local_surface_mut() {
                    *input = SavedViewInput::AwaitingImport;
                }
            }
            _ => {}
        }
        None
    }
    pub(crate) fn export_saved_views_to(&mut self, output: &mut impl std::io::Write, json: &str) {
        match crate::clipboard::write_clipboard(output, json) {
            Ok(()) => self.saved_views.feedback = Some(SavedViewTransferFeedback::ExportCopied),
            Err(error) => {
                self.saved_views.feedback = Some(SavedViewTransferFeedback::ExportFailed);
                self.saved_view_notice(error);
            }
        }
    }
    pub(crate) fn paste_saved_views(&mut self, text: &str) -> bool {
        if !matches!(
            self.local_surface(),
            Some(TuiSurface::SavedViews {
                input: SavedViewInput::AwaitingImport,
                ..
            })
        ) {
            return false;
        }
        if let Some(TuiSurface::SavedViews { input, .. }) = self.local_surface_mut() {
            *input = SavedViewInput::Browsing;
        }
        self.saved_views.feedback = Some(
            match import_saved_views_json(
                &mut self.saved_views.rows,
                &mut self.saved_views.next_id,
                text,
            ) {
                Ok(summary) => {
                    self.persist_saved_views();
                    SavedViewTransferFeedback::Imported(summary)
                }
                Err(_) => SavedViewTransferFeedback::ImportInvalid,
            },
        );
        true
    }
}

#[cfg(test)]
#[path = "../tests/gui/saved_views_tests.rs"]
mod tests;
