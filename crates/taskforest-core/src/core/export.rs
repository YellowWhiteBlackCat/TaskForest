//! Pure, borrowing JSON/CSV/HTML views over system and process snapshots.
//!
//! This domain module deliberately performs no path resolution or host I/O.
//! Transactional publication belongs to the application layer.

mod format;

pub use format::{
    ExportExtras, ProcessGpuEnginesEntry, processes_to_csv, processes_to_html, snapshot_to_json,
    snapshot_to_json_with_extras,
};
