//! Navigation and process-projection transitions dispatched by the main reducer.

use taskmanager_application::{AppAction, AppPage};

use super::super::IcedApp;

impl IcedApp {
    pub(super) fn select_page_message(&mut self, page: AppPage) {
        self.close_context_menus();
        self.shell.close_service_log();
        self.select_shared_page_route();
        let _ = self.shell.apply_action(AppAction::SelectPage(page));
        self.process_presentation.visual_cursor = 0;
        self.persist_last_page(page);
    }

    pub(super) fn toggle_group_expansion_message(
        &mut self,
        name: String,
        flat_index: usize,
        row_key: Option<taskmanager_shell::ProcessRowId>,
    ) {
        if !self.process_presentation.expanded_groups.remove(&name) {
            self.process_presentation.expanded_groups.insert(name);
        }
        if let Some(taskmanager_shell::ProcessRowId::Application(root)) = row_key {
            let _ = self.shell.select_application_row(root.pid(), flat_index);
        }
        let row_count = self.shell.table_row_count().unwrap_or(0);
        if self.shell.selected >= row_count {
            self.shell.selected = row_count.saturating_sub(1);
        }
        self.sync_visual_cursor();
    }

    pub(super) fn activate_tree_node_message(&mut self, pid: u32, flat_index: usize) {
        if !self.process_presentation.expanded_tree.remove(&pid) {
            self.process_presentation.expanded_tree.insert(pid);
        }
        let _ = self.shell.select_row(flat_index);
        self.sync_visual_cursor();
    }

    pub(super) fn expand_all_process_tree_message(&mut self) {
        self.process_presentation.expanded_tree.clear();
        self.sync_visual_cursor();
    }

    pub(super) fn collapse_all_process_tree_message(&mut self) {
        if let Some(processes) = self.shell.projection().processes.as_deref() {
            self.process_presentation.expanded_tree = processes
                .iter()
                .filter_map(|process| process.parent_pid)
                .collect();
        }
        self.sync_visual_cursor();
    }

    pub(super) fn jump_to_process_message(&mut self, pid: u32) {
        self.close_local_modals();
        self.close_shell_modals();
        self.select_shared_page_route();
        let _ = self
            .shell
            .apply_action(AppAction::SelectPage(AppPage::Applications));
        if let Some(position) = self
            .shell
            .visible_process_indices()
            .iter()
            .position(|&index| {
                self.shell
                    .projection()
                    .processes
                    .as_deref()
                    .and_then(|processes| processes.get(index))
                    .is_some_and(|process| process.pid == pid)
            })
        {
            let _ = self.shell.select_row(position);
            self.sync_visual_cursor();
        }
    }
}
