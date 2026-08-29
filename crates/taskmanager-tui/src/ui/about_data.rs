//! Pure data-layer folds for the About overlay.

use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_shell::presentation::{bytes, missing_value};

pub(super) fn memory_value(snapshot: Option<&SystemSnapshot>) -> String {
    snapshot
        .and_then(|snapshot| snapshot.memory.current_total_bytes())
        .map_or_else(missing_value, bytes)
}
