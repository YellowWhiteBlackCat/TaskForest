//! Pure page-level telemetry projection for root render composition.

use taskforest_core::core::metrics::SystemSnapshot;
use taskmanager_shell::presentation::{observed_percentage, swap_column_visible};

pub(super) struct ProcessPageMetrics {
    pub swap_auto_hidden: bool,
    pub cpu_usage: String,
    pub memory_usage: String,
}

pub(super) fn process_page_metrics(snapshot: &SystemSnapshot) -> ProcessPageMetrics {
    ProcessPageMetrics {
        swap_auto_hidden: !swap_column_visible(Some(snapshot)),
        cpu_usage: observed_percentage(snapshot.cpu.current_global_usage_pct()),
        memory_usage: observed_percentage(snapshot.memory.used_percentage_observed()),
    }
}
