//! Shared selection-fixture facts for the path-mounted headless tests.
//!
//! `input.rs` and `text_selection.rs` both drive the Applications page with
//! the same two token rows; this module is the one authority for that
//! fixture so the two seams cannot drift apart.

use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};

/// A process whose provider-native start token is available, so the shared
/// gate can freeze an authoritative identity (`FrozenProcessIdentity`).
pub(crate) fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

/// The Applications page seeded with the two selection-token rows.
pub(crate) fn shell_with_selection() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| {
        *processes = Some(vec![
            token_process(100, "alpha"),
            token_process(200, "beta"),
        ])
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}
