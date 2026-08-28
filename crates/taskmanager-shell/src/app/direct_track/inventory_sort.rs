//! Direct-track inventory sort authority and row-order projections.

use super::*;

impl InventorySorts {
    /// The active sort of one table (`None` = provider order).
    #[must_use]
    pub const fn active(&self, table: InfoTable) -> Option<(InfoSortCol, SortDir)> {
        match table {
            InfoTable::Services => self.services,
            InfoTable::Startup => self.startup,
            InfoTable::Users => self.sessions,
        }
    }

    /// Route a relative header click through the shared interactive rule (the
    /// same post-conditions as `ShellApp::set_info_sort`): clicking the
    /// already-active column flips the direction, any other column switches
    /// directly to ascending.
    pub fn click(&mut self, table: InfoTable, column: InfoSortCol) {
        let slot = self.slot_mut(table);
        match slot {
            Some((active, direction)) if *active == column => {
                *direction = direction.toggle();
            }
            _ => *slot = Some((column, SortDir::Asc)),
        }
    }

    /// Apply an ABSOLUTE `(column, direction)` sort (or `None` for provider
    /// order). Direct-track frontends whose table widget cycles a three-state
    /// header indicator (unsorted → descending → ascending) report the
    /// post-cycle state here, so the widget indicator and this authority can
    /// never disagree.
    pub fn set(&mut self, table: InfoTable, sort: Option<(InfoSortCol, SortDir)>) {
        *self.slot_mut(table) = sort;
    }

    fn slot_mut(&mut self, table: InfoTable) -> &mut Option<(InfoSortCol, SortDir)> {
        match table {
            InfoTable::Services => &mut self.services,
            InfoTable::Startup => &mut self.startup,
            InfoTable::Users => &mut self.sessions,
        }
    }
}
