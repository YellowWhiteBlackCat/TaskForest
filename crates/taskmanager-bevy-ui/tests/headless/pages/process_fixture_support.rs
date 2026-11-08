//! Shared process-page fixture facts for the path-mounted headless tests.

use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};

/// A process with exactly the observations the test names — every other
/// scalar stays honestly absent so the dash cells are exercised for free.
pub(crate) fn process(pid: u32, name: &str) -> ProcessItem {
    ProcessItem::new(pid, name).with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        ..ProcessScalarObservations::default()
    })
}

/// A shell whose process inventory is exactly `items`.
pub(crate) fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    shell
}

/// `shell_with` plus the Applications row context used by selection reducers
/// and the table's real page mount.
pub(crate) fn applications_shell(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = shell_with(items);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}
