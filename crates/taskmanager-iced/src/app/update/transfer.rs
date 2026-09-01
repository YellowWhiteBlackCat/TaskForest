//! Clipboard, export and saved-view transfer message reducer.

use taskmanager_application::i18n::t;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

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
                let payload = crate::ui::about_copy_payload(
                    self.shell.projection().hardware.as_ref(),
                    self.shell.projection().snapshot.as_ref(),
                );
                self.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("{} · {}", t("hint.copied"), t("about.copy_details")),
                );
                task = Some(iced::clipboard::write(payload));
            }
            Message::ExportSnapshot => self.request_snapshot_export(),
            Message::ApplySavedView(id) => {
                if let Some(preset) = self.saved_views.iter().find(|p| p.id == id).cloned() {
                    self.shell.set_process_status_filter(preset.filter);
                    self.shell.set_sort_column(preset.sort_col);
                    self.shell.process_sort.1 = if preset.sort_asc {
                        taskmanager_shell::SortDir::Asc
                    } else {
                        taskmanager_shell::SortDir::Desc
                    };
                    self.process_presentation.hidden_columns = preset.hidden_cols;
                }
            }
            Message::SaveCurrentProcessView => {
                let id = self.next_saved_view_id;
                self.next_saved_view_id = self.next_saved_view_id.wrapping_add(1);
                let count = self
                    .saved_views
                    .iter()
                    .filter(|preset| preset.is_user_saved())
                    .count()
                    + 1;
                let mut custom = crate::saved_views::SavedViewPreset::restored(
                    format!("Custom View ({count})"),
                    self.shell.process_status_filter,
                    self.shell.process_sort.0,
                    self.shell.process_sort.1 == taskmanager_shell::SortDir::Asc,
                    self.process_presentation.hidden_columns.clone(),
                );
                custom.id = id;
                self.saved_views.push(custom);
            }
            Message::ExportSavedViews => {
                match crate::saved_views::export_saved_views_json(&self.saved_views) {
                    Ok(json) => {
                        self.saved_view_feedback =
                            Some(crate::saved_views::SavedViewTransferFeedback::ExportCopied);
                        task = Some(iced::clipboard::write(json));
                    }
                    Err(_) => {
                        self.saved_view_feedback =
                            Some(crate::saved_views::SavedViewTransferFeedback::ExportFailed);
                    }
                }
            }
            Message::ImportSavedViews => {
                self.saved_view_feedback =
                    Some(crate::saved_views::SavedViewTransferFeedback::ClipboardEmpty);
            }
            Message::DeleteSavedView(id) => {
                self.saved_views
                    .retain(|preset| preset.id != id || preset.built_in);
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
            Message::GenerateDiagnosticsReport => {
                // The account labels observed on this host join the paths and
                // addresses core already redacts, matching the GPUI bundle's
                // source preparation.
                let usernames: Vec<String> = self
                    .shell
                    .projection()
                    .processes
                    .as_deref()
                    .into_iter()
                    .flatten()
                    .filter_map(taskmanager_core::core::process::ProcessItem::current_user)
                    .collect();
                // Fail closed: a report whose redaction could not be verified
                // is never written to the clipboard, so no unredacted text can
                // leak by way of a failure path.
                if let Ok(report) = crate::export::system_diagnostics_markdown(
                    self.shell.projection().hardware.as_ref(),
                    self.shell.projection().snapshot.as_ref(),
                    usernames,
                ) {
                    task = Some(iced::clipboard::write(report));
                }
            }
            _ => return UpdateDispatch::none(),
        }
        task.map_or_else(UpdateDispatch::none, UpdateDispatch::task)
    }
}
