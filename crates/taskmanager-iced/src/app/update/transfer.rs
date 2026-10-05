//! Clipboard, export and saved-view transfer message reducer.

use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::saved_views::{
    SavedViewPreset, SavedViewTransferFeedback, export_saved_views_json, import_saved_views_json,
    save_current_view,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;
use crate::app::LocalSurface;
use taskmanager_shell::SortDir;
use taskmanager_shell::presentation::about::metadata;
use taskmanager_shell::presentation::system_information::copy_all_text;

impl IcedApp {
    pub(super) fn reduce_transfer_message(&mut self, message: Message) -> UpdateDispatch {
        let mut task = None;
        match message {
            Message::CopyTextToClipboard { label, text } => {
                task = Some(iced::clipboard::write(text));
                self.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("{label} {}", t("common.copied")),
                );
            }
            Message::OpenStartupLocation { index } => {
                let (rows, _, _) = self.startup_projection();
                if let Some(row) = rows.get(index) {
                    task = Some(iced::clipboard::write(row.exec.clone()));
                    self.shell.report_notice(
                        FeedbackSource::Clipboard,
                        FeedbackSeverity::Success,
                        FeedbackLifecycle::SHORT,
                        format!("{} {}", row.name, t("common.copied")),
                    );
                }
            }
            Message::CopyAboutDetails => {
                let payload = metadata(
                    env!("CARGO_PKG_VERSION"),
                    product::LICENSE_SPDX,
                    product::REPOSITORY_URL,
                )
                .details_text();
                self.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("{} · {}", t("hint.copied"), t("about.copy_details")),
                );
                task = Some(iced::clipboard::write(payload));
            }
            Message::CopySystemInformation => {
                if let Some(LocalSurface::SystemInformation(facts)) = self.local_surface() {
                    task = Some(iced::clipboard::write(copy_all_text(facts)));
                }
            }
            Message::ExportSnapshot => self.request_snapshot_export(),
            Message::RequestCurrentWindowCapture => {
                let _ = self.request_current_window_capture();
            }
            Message::ApplySavedView(id) => {
                if let Some(preset) = self.saved_views.iter().find(|p| p.id == id).cloned() {
                    self.shell.set_process_status_filter(preset.filter);
                    self.shell.set_sort_column(preset.sort_col);
                    self.shell.process_sort.1 = if preset.sort_asc {
                        SortDir::Asc
                    } else {
                        SortDir::Desc
                    };
                    self.process_presentation.hidden_columns = preset.hidden_cols;
                }
            }
            Message::SaveCurrentProcessView => {
                let preset = SavedViewPreset::restored(
                    t("saved_views.custom_name").replace(
                        "{index}",
                        &self.next_saved_view_id.saturating_sub(3).to_string(),
                    ),
                    self.shell.process_status_filter,
                    self.shell.process_sort.0,
                    self.shell.process_sort.1 == SortDir::Asc,
                    self.process_presentation.hidden_columns.clone(),
                );
                match save_current_view(&mut self.saved_views, &mut self.next_saved_view_id, preset)
                {
                    Ok(_) => self.persist_saved_views(),
                    Err(error) => self.shell.report_notice(
                        FeedbackSource::Settings,
                        FeedbackSeverity::Error,
                        FeedbackLifecycle::UntilReplaced,
                        format!("View not saved: {error}"),
                    ),
                }
            }
            Message::ExportSavedViews => match export_saved_views_json(&self.saved_views) {
                Ok(json) => {
                    self.saved_view_feedback = Some(SavedViewTransferFeedback::ExportCopied);
                    task = Some(iced::clipboard::write(json));
                }
                Err(_) => {
                    self.saved_view_feedback = Some(SavedViewTransferFeedback::ExportFailed);
                }
            },
            Message::ImportSavedViews => {
                task = Some(iced::clipboard::read().map(Message::SavedViewsClipboardRead));
            }
            Message::SavedViewsClipboardRead(json) => {
                self.saved_view_feedback =
                    Some(match json.filter(|text| !text.trim().is_empty()) {
                        Some(json) => match import_saved_views_json(
                            &mut self.saved_views,
                            &mut self.next_saved_view_id,
                            &json,
                        ) {
                            Ok(summary) => {
                                self.persist_saved_views();
                                SavedViewTransferFeedback::Imported(summary)
                            }
                            Err(_) => SavedViewTransferFeedback::ImportInvalid,
                        },
                        None => SavedViewTransferFeedback::ClipboardEmpty,
                    });
            }
            Message::DeleteSavedView(id) => {
                if self
                    .saved_views
                    .iter()
                    .any(|preset| preset.id == id && preset.is_user_saved())
                {
                    self.saved_views.retain(|preset| preset.id != id);
                    self.persist_saved_views();
                }
            }
            Message::CopyProcessTsv => {
                if let Some(process) = self.shell.visible_process_at(self.shell.selected) {
                    task = Some(iced::clipboard::write(crate::export::process_to_tsv(
                        process,
                    )));
                }
            }
            Message::CopyProcessJson => {
                if let Some(process) = self.shell.visible_process_at(self.shell.selected) {
                    task = Some(iced::clipboard::write(crate::export::process_to_json(
                        process,
                    )));
                }
            }
            Message::GenerateDiagnosticsReport => self.open_diagnostic_bundle(),
            Message::ConfirmDiagnosticsExport => self.confirm_diagnostic_bundle(),
            Message::RetryDiagnostics => self.retry_diagnostic_bundle(),
            _ => return UpdateDispatch::none(),
        }
        task.map_or_else(UpdateDispatch::none, UpdateDispatch::task)
    }
}
