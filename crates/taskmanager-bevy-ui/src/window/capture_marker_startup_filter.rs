//! Startup status-filter capture readiness.

use crate::app::Page;
use crate::pages::startup::{StartupRowMarker, startup_rows_for};
use taskmanager_shell::StartupStatusFilter;

use super::capture_marker::CaptureAccess;

pub(super) fn ready(access: &CaptureAccess<'_, '_>) -> bool {
    let Some(filter) = access.startup_filter.as_deref() else {
        return false;
    };
    let expected = startup_rows_for(&access.track.shell, StartupStatusFilter::Enabled);
    let total_rows = access.track.shell.sorted_startup_entries().len();
    let mut mounted: Vec<_> = access
        .startup_rows
        .iter()
        .map(|row: &StartupRowMarker| (row.0, row.1.clone()))
        .collect();
    mounted.sort_by_key(|(index, _)| *index);
    access.route.page == Page::Startup
        && filter.selected == StartupStatusFilter::Enabled
        && !expected.is_empty()
        && expected.len() < total_rows
        && mounted.len() == expected.len()
        && expected
            .iter()
            .zip(mounted)
            .all(|(expected, (_, target))| expected.target == target)
}
