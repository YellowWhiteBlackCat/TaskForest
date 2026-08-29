//! Crossterm-runtime unit tests, split by topic.
//!
//! Each sibling submodule covers one slice of `handle_key` / the runtime seam
//! behavior so no single file exceeds the source line budget. The tests reach
//! the runtime items through `use super::super::*;` (the same items the inline
//! module used to pull in via `use super::*;`).

#[path = "tests/binding_matrix.rs"]
mod binding_matrix;
#[path = "tests/clipboard.rs"]
mod clipboard;
#[path = "tests/column_menu.rs"]
mod column_menu;
#[path = "tests/command_palette.rs"]
mod command_palette;
#[path = "tests/control_feedback.rs"]
mod control_feedback;
#[path = "tests/directory_usage.rs"]
mod directory_usage;
#[path = "tests/draw_predicate.rs"]
mod draw_predicate;
#[path = "tests/focus_ring.rs"]
mod focus_ring;
#[path = "tests/group_view.rs"]
mod group_view;
#[path = "tests/keys_navigation.rs"]
mod keys_navigation;
#[path = "tests/overlays.rs"]
mod overlays;
#[path = "tests/page_navigation.rs"]
mod page_navigation;
#[path = "tests/process_batch.rs"]
mod process_batch;
#[path = "tests/process_properties.rs"]
mod process_properties;
#[path = "tests/process_rows_snapshot.rs"]
mod process_rows_snapshot;
#[path = "tests/search_match.rs"]
mod search_match;
#[path = "tests/semantic_snapshot.rs"]
mod semantic_snapshot;
#[path = "tests/service_control.rs"]
mod service_control;
#[path = "tests/session_control.rs"]
mod session_control;
#[path = "tests/settings_export.rs"]
mod settings_export;
#[path = "tests/smart_self_test.rs"]
mod smart_self_test;
#[path = "tests/source_recovery.rs"]
mod source_recovery;
#[path = "tests/startup_control.rs"]
mod startup_control;
#[path = "tests/surface_protocol.rs"]
mod surface_protocol;
