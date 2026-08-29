//! Bounded startup handshake for native desktop appearance facts.

use std::time::Duration;

use taskmanager_application::PlatformClient;
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_platform_contract::OperationFailure;

pub(super) struct StartupAppearanceObservation {
    pub(super) value: DesktopAppearance,
    pub(super) sources: Vec<SourceStatus>,
    pub(super) failures: Vec<OperationFailure>,
}

pub(super) fn observe_startup_appearance(
    platform: &mut PlatformClient,
) -> StartupAppearanceObservation {
    const WAIT_TIMEOUT: Duration = Duration::from_millis(650);
    const POLL_INTERVAL: Duration = Duration::from_millis(5);

    let handshake = platform.observe_desktop_appearance(WAIT_TIMEOUT, POLL_INTERVAL);
    match handshake.snapshot {
        Some(snapshot) => StartupAppearanceObservation {
            value: snapshot.value,
            sources: snapshot.sources,
            failures: handshake.failures,
        },
        None => fallback(handshake.failures),
    }
}

fn fallback(failures: Vec<OperationFailure>) -> StartupAppearanceObservation {
    StartupAppearanceObservation {
        // A timed-out Windows provider must not erase the one fact that is
        // known from the executable target itself. The scheme/HC remain
        // Unknown and are resolved conservatively, while the skin is corrected
        // immediately instead of flashing the GNOME fallback.
        value: DesktopAppearance {
            family: if cfg!(target_os = "windows") {
                DesktopFamily::Windows
            } else {
                DesktopFamily::Unknown
            },
            ..DesktopAppearance::default()
        },
        sources: Vec::new(),
        failures,
    }
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_root_startup_appearance_tests.rs"]
mod tests;
