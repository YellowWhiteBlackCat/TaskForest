//! GPUI `SortCol` → neutral `ProcessSortAxis` bridge.
//!
//! The mapping itself lives ONCE in the shell
//! ([`taskmanager_shell::sort_axis`], over the shared `SortCol` vocabulary)
//! and is consumed by the shell's own `visible_processes`, the iced and TUI
//! tree projections, and the GPUI canonical `category_tree_rows` path through
//! this re-export. The shell match is
//! compiler-exhaustive: adding a `SortCol` variant without an axis arm is a
//! build error, so a column can never silently miss the neutral comparator
//! in any frontend.

pub use taskmanager_shell::sort_axis;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_processes_view_sort_key_tests.rs"]
mod tests;
