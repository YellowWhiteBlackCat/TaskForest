//! Synchronous handshake helpers: submit one request and block until the
//! correlated outcome (snapshot, failure, or timeout) is drained.

use std::time::{Duration, Instant};

use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_platform_contract::{CompositeSourceSnapshot, OperationFailure};

use crate::PlatformClient;
use crate::platform::{DesktopAppearanceEvent, DesktopAppearanceRequest};

/// Outcome of one synchronous desktop-appearance handshake.
#[derive(Debug, Default)]
pub struct DesktopAppearanceHandshake {
    /// Matched snapshot; `None` on submit rejection, correlated failure, or
    /// timeout.
    pub snapshot: Option<CompositeSourceSnapshot<DesktopAppearance>>,
    /// Failures drained while waiting (matched failure receipts included).
    pub failures: Vec<OperationFailure>,
}

impl PlatformClient {
    /// Submit an appearance observation and poll until the matching snapshot
    /// or failure receipt arrives (or `wait` elapses).
    ///
    /// The loop is deliberately small and typed: it drains real envelopes,
    /// correlates them against the one request id this call owns, and never
    /// swallows a failure — a matched failure is returned so the caller can
    /// surface it while falling back to defaults.
    pub fn observe_desktop_appearance(
        &mut self,
        wait: Duration,
        poll: Duration,
    ) -> DesktopAppearanceHandshake {
        let Ok(request_id) =
            self.submit_desktop_appearance(DesktopAppearanceRequest::Observe, submitted_at_ms())
        else {
            return DesktopAppearanceHandshake::default();
        };
        let deadline = Instant::now() + wait;
        loop {
            match self.try_drain() {
                Ok(batch) => {
                    let failures = batch.failures;
                    if let Some(event) = batch
                        .desktop_appearance_events
                        .into_iter()
                        .find(|event| event.request_id == request_id)
                    {
                        let DesktopAppearanceEvent::Snapshot(snapshot) = event.event;
                        return DesktopAppearanceHandshake {
                            snapshot: Some(snapshot),
                            failures,
                        };
                    }
                    if failures
                        .iter()
                        .any(|failure| failure.request_id == request_id)
                    {
                        return DesktopAppearanceHandshake {
                            snapshot: None,
                            failures,
                        };
                    }
                }
                Err(_) => return DesktopAppearanceHandshake::default(),
            }
            if Instant::now() >= deadline {
                return DesktopAppearanceHandshake::default();
            }
            std::thread::sleep(poll);
        }
    }
}

fn submitted_at_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_client_handshake_tests.rs"]
mod tests;
