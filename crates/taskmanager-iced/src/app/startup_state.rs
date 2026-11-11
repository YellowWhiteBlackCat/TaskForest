//! Iced-local Startup search state and filtered row projection.

use taskmanager_shell::matches_startup_text;

use crate::ui::startup_table::StartupRow;

use super::IcedApp;

#[derive(Default)]
pub(crate) struct StartupPresentationState {
    query: String,
}

impl StartupPresentationState {
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    fn replace_query(&mut self, query: String) -> bool {
        if self.query == query {
            return false;
        }
        self.query = query;
        true
    }
}

pub(crate) fn startup_query_matches_row(row: &StartupRow, query: &str) -> bool {
    matches_startup_text(&row.name, &row.exec, query)
}

impl IcedApp {
    pub(crate) fn startup_visible_indices(&self) -> std::rc::Rc<Vec<usize>> {
        self.startup_projection().1
    }

    pub(crate) fn set_startup_query(&mut self, query: String) {
        let selected_source = self.startup_source_index(self.shell.selected);
        if !self.startup_presentation.replace_query(query) {
            return;
        }

        self.close_startup_row_menu();
        let visible = self.startup_visible_indices();
        let sorted = self.shell.sorted_startup_indices();
        let selected = selected_source
            .and_then(|source| {
                visible.iter().position(|canonical| {
                    sorted.get(*canonical).is_some_and(|index| *index == source)
                })
            })
            .unwrap_or(0);
        self.shell.move_selection_to(selected);
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/startup_search_tests.rs"]
mod tests;
