//! Whole-tree presentation commands over the shared structural row authority.
use crate::TuiApp;
impl TuiApp {
    pub(crate) fn set_tree_expanded(&mut self, expanded: bool) {
        let anchor = self.selected_application_row_anchor();
        self.collapsed_tree.clear();
        self.expanded_groups.clear();
        if expanded {
            loop {
                let keys: Vec<_> = self
                    .build_canonical_rows()
                    .iter()
                    .filter_map(|row| row.expansion_key().map(str::to_owned))
                    .collect();
                let before = self.expanded_groups.len();
                self.expanded_groups.extend(keys);
                if self.expanded_groups.len() == before {
                    break;
                }
            }
        }
        self.reconcile_application_row_anchor(anchor);
    }
}
