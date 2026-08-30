//! Test-side selection reference helpers on [`TuiApp`]: the cache-validity
//! probe and the detail-row resolver the runtime tests compare against.

use taskmanager_core::core::process::ProcessItem;

use super::process_view;
use crate::TuiApp;

impl TuiApp {
    /// Test-side validity probe: delegates to the private cache-validity
    /// check so runtime tests can assert invalidation without reaching into
    /// the selection's private cache fields.
    pub(crate) fn canonical_row_cache_is_valid_for_current_inputs(&self) -> bool {
        self.canonical_row_cache_is_valid()
    }

    /// Test-side detail-row resolver: the process the current selection
    /// addresses within `rows`, if any.
    #[must_use]
    pub(crate) fn selected_detail_process_rows(
        &self,
        rows: &[process_view::ProcessRow<'_>],
    ) -> Option<ProcessItem> {
        process_view::process_at(rows, self.selected).cloned()
    }
}
