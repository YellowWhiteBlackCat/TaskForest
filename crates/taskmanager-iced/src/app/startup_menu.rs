//! Iced Startup-row context-menu state and identity-safe dispatch.

use super::*;

impl IcedApp {
    pub(super) fn open_startup_row_menu(&mut self, visual_index: usize) {
        let Some(source_index) = self.startup_source_index(visual_index) else {
            return;
        };
        let entry = self
            .shell
            .projection()
            .startup_entries
            .as_deref()
            .and_then(|entries| entries.get(source_index))
            .cloned();
        let Some(entry) = entry else {
            return;
        };
        let _ = self.shell.select_row(visual_index);
        self.open_context_menu(ContextMenu::Startup {
            source_index,
            entry,
        });
    }

    pub(super) fn close_startup_row_menu(&mut self) {
        if self.context_menu_kind() == Some(ContextMenuKind::Startup) {
            self.dismiss_context_menu();
        }
    }

    pub(super) fn apply_startup_menu_action(&mut self, enabled: bool) -> Option<PlatformEffect> {
        let entry = self.startup_menu_entry().cloned();
        self.close_startup_row_menu();
        entry.and_then(|entry| self.shell.request_startup_control_for(entry, enabled))
    }

    pub(super) fn startup_source_index(&self, visual_index: usize) -> Option<usize> {
        self.shell
            .sorted_startup_indices()
            .get(visual_index)
            .copied()
    }
}
