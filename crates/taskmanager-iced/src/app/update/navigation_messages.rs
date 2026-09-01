//! Page, selector, hierarchy, table and search message reducer.

use super::super::{ContextMenu, ContextMenuKind, IcedApp, Message};
use super::dispatch::UpdateDispatch;
use taskmanager_shell::ProcessRowId;

impl IcedApp {
    pub(super) fn reduce_navigation_message(&mut self, message: Message) -> UpdateDispatch {
        match message {
            Message::SelectPage(page) => self.select_page_message(page),
            Message::SelectPerfDevice(device) => self.performance.selected_device = device,
            Message::SelectProcessStatusFilter(filter) => {
                self.shell.set_process_status_filter(filter);
                self.process_presentation.visual_cursor = 0;
            }
            Message::ToggleGroupExpansion { name, row_key } => {
                self.toggle_group_expansion_message(name, row_key)
            }
            Message::ActivateTreeNode {
                identity,
                flat_index,
            } => {
                self.activate_tree_node_message(identity, flat_index);
            }
            Message::ExpandAllProcessTree => self.expand_all_process_tree_message(),
            Message::CollapseAllProcessTree => self.collapse_all_process_tree_message(),
            Message::JumpToProcess { identity } => self.jump_to_process_message(identity),
            Message::EnvironmentFilterChanged(filter) => {
                self.process_presentation.env_filter = filter
            }
            Message::SelectRow(index) => {
                let modifiers = self.input.modifiers;
                if modifiers.shift() {
                    let _ = self.shell.extend_row_selection(index);
                } else if modifiers.control() || modifiers.logo() {
                    let _ = self.shell.toggle_row_selection(index);
                } else {
                    let _ = self.shell.select_row(index);
                    self.close_context_menus();
                }
                self.sync_visual_cursor();
                if let Some(effect) = self.shell.request_process_insights() {
                    self.queue(effect);
                }
            }
            Message::SortBy(column) => {
                if column == self.shell.process_sort.0 {
                    self.shell.toggle_sort_direction();
                } else {
                    self.shell.set_sort_column(column);
                }
                self.process_presentation.visual_cursor = 0;
            }
            Message::SortInfoTable { table, column } => self.shell.set_info_sort(table, column),
            Message::SearchChanged(query) => {
                self.shell.query = query;
                self.shell.move_selection_to(0);
                self.shell.sync_application_selection();
                self.process_presentation.visual_cursor = 0;
            }
            Message::FocusSearch => self.shell.open_search(),
            Message::CloseSearch => self.shell.close_search(),
            Message::ServicesSearchChanged(query) => {
                self.process_presentation.services_query = query
            }
            Message::SelectDetailsSection(section) => {
                self.process_presentation.details_section = section
            }
            Message::OpenProcessRowMenu { identity } => {
                if self.shell.select_row_id(ProcessRowId::Process(identity)) {
                    self.open_context_menu(ContextMenu::Process { identity });
                }
            }
            Message::CloseProcessRowMenu => {
                self.dismiss_context_menu_kind(ContextMenuKind::Process);
            }
            Message::ProcessMenuAction(action) => {
                let mut clipboard = None;
                let effect = self.apply_process_menu_action(action, &mut clipboard);
                let mut dispatch = UpdateDispatch::effect(effect);
                if let Some(task) = clipboard {
                    dispatch = dispatch.with_task(task);
                }
                return dispatch;
            }
            Message::OpenProcessColumnsMenu => self.open_process_columns_menu(),
            Message::CloseProcessColumnsMenu => self.close_process_columns_menu(),
            Message::ToggleProcessColumn(column) => self.toggle_process_column(column),
            Message::ResetProcessColumns => self.reset_process_columns(),
            _ => return UpdateDispatch::none(),
        }
        UpdateDispatch::none()
    }
}
