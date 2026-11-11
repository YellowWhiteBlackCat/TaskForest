//! Iced-local Startup search state and filtered row projection.

use taskmanager_shell::StartupStatusFilter;

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

impl IcedApp {
    pub(crate) fn startup_visible_indices(&self) -> std::rc::Rc<Vec<usize>> {
        self.startup_projection().1
    }

    pub(crate) fn set_startup_query(&mut self, query: String) {
        self.reproject_startup_selection(|app| app.startup_presentation.replace_query(query));
    }

    pub(crate) fn set_startup_status_filter(&mut self, filter: StartupStatusFilter) {
        self.reproject_startup_selection(|app| {
            if app.startup_status_filter == filter {
                false
            } else {
                app.startup_status_filter = filter;
                true
            }
        });
    }

    fn reproject_startup_selection(&mut self, change: impl FnOnce(&mut Self) -> bool) {
        let selected_source = self.startup_source_index(self.shell.selected);
        if !change(self) {
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

#[cfg(test)]
#[path = "../../tests/gui/app/startup_status_tests.rs"]
mod status_tests;
