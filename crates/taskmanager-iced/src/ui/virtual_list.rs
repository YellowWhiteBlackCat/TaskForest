//! Product column specifications, viewport tuning, and renderer cache keys.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use zhuge_ui_core::virtual_list::VirtualWindow;

mod columns;
pub(crate) use columns::{ColumnWidth, TableColumn};

mod window;
pub(crate) use window::resolve_table_window;

/// Inventory header remains product-owned.
pub(crate) const VIRTUAL_TABLE_HEADER_HEIGHT: f32 = 32.0;

/// Add the materialized row range to a renderer-specific lazy-body key.
/// Keeping this in one place prevents a new virtual table from accidentally
/// caching the first visible range forever.
#[must_use]
pub(crate) fn virtual_table_key(base: u64, window: VirtualWindow) -> u64 {
    let mut hasher = DefaultHasher::new();
    base.hash(&mut hasher);
    window.key().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/virtual_list_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/table_columns_tests.rs"]
mod table_columns_tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/virtual_list_overscan_tests.rs"]
mod virtual_list_overscan_tests;
