//! Iced Applications column-visibility menu state.

use super::*;

impl IcedApp {
    pub(super) fn open_process_columns_menu(&mut self) {
        self.open_context_menu(ContextMenu::ProcessColumns);
    }

    pub(super) fn close_process_columns_menu(&mut self) {
        if self.context_menu_kind() == Some(ContextMenuKind::ProcessColumns) {
            self.dismiss_context_menu();
        }
    }

    pub(super) fn toggle_process_column(&mut self, column: SortCol) {
        // Hideability is contract truth (PROCESS_COLUMNS): the identity
        // column is not hideable, so its toggle is a no-op that only closes
        // the menu.
        if crate::ui::applications::column_hideable(column)
            && !self.process_presentation.hidden_columns.remove(&column)
        {
            self.process_presentation.hidden_columns.insert(column);
        }
        self.close_process_columns_menu();
    }

    pub(super) fn reset_process_columns(&mut self) {
        self.process_presentation.hidden_columns.clear();
        // "Reset" restores the whole default column layout: visibility AND
        // widths. This is also the documented recovery path for a persisted
        // width the user wants gone — clearing empties the config token.
        self.process_column_sizing.overrides.clear();
        self.close_process_columns_menu();
        self.persist_process_column_widths();
        // The direct persist subsumes any deferred stepper commit.
        self.process_column_sizing.note_direct_persist();
    }
}
