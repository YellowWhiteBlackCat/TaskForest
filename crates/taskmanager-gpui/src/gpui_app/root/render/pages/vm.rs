//! Pure page-level telemetry projection for root render composition.

use crate::core::metrics::SystemSnapshot;
use crate::gpui_app::formatting;

pub(super) struct ProcessPageMetrics {
    pub swap_total_bytes: Option<u64>,
    pub swap_auto_hidden: bool,
    pub cpu_usage: String,
    pub memory_usage: String,
}

pub(super) fn process_page_metrics(snapshot: &SystemSnapshot) -> ProcessPageMetrics {
    let swap_total_bytes = snapshot.memory.current_swap_total_bytes();
    ProcessPageMetrics {
        swap_total_bytes,
        swap_auto_hidden: swap_total_bytes == Some(0),
        cpu_usage: snapshot
            .cpu
            .current_global_usage_pct()
            .map_or_else(formatting::missing_value, |value| format!("{value:.0}%")),
        memory_usage: snapshot
            .memory
            .used_percentage_observed()
            .map_or_else(formatting::missing_value, |value| format!("{value:.0}%")),
    }
}
