//! Navigation and process-projection transitions dispatched by the main reducer.

use std::collections::HashSet;

use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::ProcessRowId;

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
        row_key: Option<taskmanager_shell::ProcessRowId>,
    ) {
        if !self.process_presentation.expanded_groups.remove(&name) {
            self.process_presentation.expanded_groups.insert(name);
        }
        if let Some(taskmanager_shell::ProcessRowId::Application(root)) = row_key {
            let _ = self
                .shell
                .select_row_id(taskmanager_shell::ProcessRowId::Application(root));
        }
        let row_count = self.shell.table_row_count().unwrap_or(0);
        if self.shell.selected >= row_count {
            self.shell.move_selection_to(row_count.saturating_sub(1));
        }
        self.sync_visual_cursor();
    }

    pub(super) fn activate_tree_node_message(
        &mut self,
        identity: Option<ProcessLiveKey>,
        flat_index: usize,
    ) {
        if let Some(identity) = identity
            && !self.process_presentation.expanded_tree.remove(&identity)
        {
            self.process_presentation.expanded_tree.insert(identity);
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
            let parent_pids: HashSet<u32> = processes
                .iter()
                .filter_map(|process| process.parent_pid)
                .collect();
            self.process_presentation.expanded_tree = processes
                .iter()
                .filter(|process| parent_pids.contains(&process.pid))
                .filter_map(ProcessLiveKey::from_process)
                .collect();
        }
        self.sync_visual_cursor();
    }

    pub(super) fn jump_to_process_message(&mut self, identity: ProcessLiveKey) {
        self.close_local_modals();
        self.close_shell_modals();
        self.select_shared_page_route();
        let _ = self
            .shell
            .apply_action(AppAction::SelectPage(AppPage::Applications));
        if self.shell.select_row_id(ProcessRowId::Process(identity)) {
            self.sync_visual_cursor();
        }
    }
}
