//! `ShellApp::selected_row_summary` — the single Ctrl+C clipboard-copy seam.

use taskmanager_application::AppPage;

use super::ShellApp;

impl ShellApp {
    /// Plain-text summary of the currently selected row for Ctrl+C clipboard
    /// copy: `pid<TAB>name` for Applications (the TUI OSC-52 payload shape),
    /// `name<TAB>status` for Services/Startup/Users, and `None` on pages
    /// without a row selection. Single source so every frontend copies the
    /// same bytes.
    #[must_use]
    pub fn selected_row_summary(&self) -> Option<String> {
        match self.page() {
            AppPage::Applications => {
                let index = *self.visible_process_indices().get(self.selected)?;
                let process = self.data.processes.as_ref()?.get(index)?;
                Some(format!("{}\t{}", process.pid, process.name))
            }
            AppPage::Services => {
                let service = self.data.services.as_ref()?.get(self.selected)?;
                Some(format!("{}\t{:?}", service.name, service.status))
            }
            AppPage::Startup => {
                let entry = self.data.startup_entries.as_ref()?.get(self.selected)?;
                let state = if entry.enabled {
                    taskmanager_application::i18n::t("common.enabled")
                } else {
                    taskmanager_application::i18n::t("common.disabled")
                };
                Some(format!("{}\t{state}", entry.name))
            }
            AppPage::Users => {
                let session = self.data.sessions.as_ref()?.get(self.selected)?;
                let detail = session
                    .seat
                    .as_deref()
                    .filter(|seat| !seat.is_empty())
                    .unwrap_or(session.user.as_str());
                Some(format!("{}\t{detail}", session.user))
            }
            AppPage::Performance | AppPage::System | AppPage::AppHistory => None,
        }
    }
}
