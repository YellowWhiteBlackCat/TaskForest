//! Iced binding for the shell-owned Startup enabled-state filter.
//!
//! The enum, catalog, localized labels, and enabled-state predicate all live
//! in `taskmanager-shell`; this module only adapts the complete catalog to the
//! segmented widget's numeric choice channel.

use taskmanager_shell::StartupStatusFilter;

use super::Message;

#[must_use]
pub(crate) fn startup_filter_index(filter: StartupStatusFilter) -> usize {
    let mut index = 0;
    while index < StartupStatusFilter::ALL.len() {
        if StartupStatusFilter::ALL[index] == filter {
            return index;
        }
        index += 1;
    }
    0
}

pub(crate) fn startup_filter_message(index: usize) -> Message {
    Message::SelectStartupStatusFilter(
        StartupStatusFilter::ALL
            .get(index)
            .copied()
            .unwrap_or_default(),
    )
}
