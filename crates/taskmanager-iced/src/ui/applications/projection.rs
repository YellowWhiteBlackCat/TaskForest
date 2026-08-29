//! Pure Applications-page facts projected before widget construction.

use taskmanager_core::core::metrics::SystemSnapshot;

/// A measured zero swap total confirms that no swap device exists. Unknown
/// telemetry keeps the column visible so absence is never fabricated.
#[must_use]
pub(super) fn swap_column_visible(snapshot: Option<&SystemSnapshot>) -> bool {
    snapshot.is_none_or(|snapshot| snapshot.memory.current_swap_total_bytes() != Some(0))
}
