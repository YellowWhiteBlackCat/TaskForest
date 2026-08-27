//! Bounded service-log classification and worker protocol.

#[cfg(any(test, feature = "test-support"))]
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};
#[cfg(any(test, feature = "test-support"))]
use std::thread;

use super::{SERVICE_LOG_LINE_LIMIT, ServiceLogErrorKind, ServiceLogFailure, ServiceLogState};
#[cfg(any(test, feature = "test-support"))]
use super::{ServiceLogSnapshot, ServiceManager};
#[cfg(any(test, feature = "test-support"))]
use taskmanager_core::ServiceId;

/// Platform/process outcome kept separate from presentation-state classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceLogCommandOutcome {
    Exited {
        success: bool,
        stdout: String,
        stderr: String,
    },
    Failure(ServiceLogFailure),
}

/// Map a bounded provider call into explicit UI states and defensively retain
/// only its most recent 50 non-empty lines.
pub fn classify_service_log_outcome(outcome: ServiceLogCommandOutcome) -> ServiceLogState {
    match outcome {
        ServiceLogCommandOutcome::Failure(failure) => ServiceLogState::Unavailable(failure),
        ServiceLogCommandOutcome::Exited {
            success,
            stdout,
            stderr,
        } => {
            if permission_denied(&stderr) {
                return ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::PermissionDenied,
                    first_message(&stderr),
                ));
            }
            if !success {
                return ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::ProviderFailed,
                    first_message(&stderr),
                ));
            }
            let mut lines: Vec<String> = stdout
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.trim().is_empty())
                .rev()
                .take(SERVICE_LOG_LINE_LIMIT)
                .map(str::to_string)
                .collect();
            lines.reverse();
            ServiceLogState::from_lines(lines)
        }
    }
}

pub(super) fn permission_denied(stderr: &str) -> bool {
    let message = stderr.to_ascii_lowercase();
    [
        "permission denied",
        "not permitted",
        "access denied",
        "not authorized",
        "insufficient permissions",
        "not seeing messages from other users",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub(super) fn first_message(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("service log command failed")
        .to_string()
}

#[cfg(any(test, feature = "test-support"))]
/// Capacity of the worker→UI result queue. Small on purpose: the UI consumer
/// keeps only the newest snapshot, so on overflow the worker discards the
/// OLDEST queued result and an idle consumer can never accumulate unbounded
/// memory.
const SERVICE_LOG_RESULT_CAPACITY: usize = 4;

/// A single background reader used by the details dialog. `request` only sends
/// a small message and therefore never runs journalctl on the GPUI thread.
#[cfg(any(test, feature = "test-support"))]
pub struct ServiceLogWorker {
    request_tx: Sender<ServiceId>,
    result_rx: Receiver<ServiceLogSnapshot>,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for ServiceLogWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ServiceLogWorker {
    pub fn new() -> Self {
        Self::with_loader(|service_id| {
            ServiceManager::fetch_logs(&service_id).unwrap_or_else(|failure| {
                ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::from_failure(failure.kind()),
                    "service log target is unavailable",
                ))
            })
        })
    }

    pub(super) fn with_loader(
        loader: impl Fn(ServiceId) -> ServiceLogState + Send + 'static,
    ) -> Self {
        // One queued refresh is enough while journalctl is in flight. A bounded
        // queue prevents repeated clicks from creating an unbounded command tail.
        let (request_tx, request_rx) = bounded::<ServiceId>(1);
        // The result queue is bounded too: if the consumer stops draining while
        // requests keep flowing, the worker discards the oldest queued snapshot
        // instead of accumulating without bound.
        let (result_tx, result_rx) = bounded::<ServiceLogSnapshot>(SERVICE_LOG_RESULT_CAPACITY);
        // Worker-side receive handle used only to drop the oldest result when
        // the queue is full; the consumer still sees the newest snapshot.
        let overflow_rx = result_rx.clone();
        let _ = thread::Builder::new()
            .name("service-log-worker".into())
            .spawn(move || {
                while let Ok(service_id) = request_rx.recv() {
                    let state = loader(service_id.clone());
                    let mut snapshot = ServiceLogSnapshot { service_id, state };
                    loop {
                        match result_tx.try_send(snapshot) {
                            Ok(()) => break,
                            Err(TrySendError::Full(returned)) => {
                                // Make room by discarding the oldest queued
                                // snapshot; if the consumer just drained the
                                // queue instead, the retry send cannot fail.
                                let _ = overflow_rx.try_recv();
                                snapshot = returned;
                            }
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
            });
        Self {
            request_tx,
            result_rx,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", allow(dead_code))]
    pub(super) fn disconnected() -> Self {
        let (request_tx, request_rx) = bounded::<ServiceId>(1);
        let (result_tx, result_rx) = bounded::<ServiceLogSnapshot>(SERVICE_LOG_RESULT_CAPACITY);
        drop(request_rx);
        drop(result_tx);
        Self {
            request_tx,
            result_rx,
        }
    }

    /// Queue an initial load or refresh and return the immediate transition.
    pub fn request(&self, service_id: ServiceId) -> ServiceLogSnapshot {
        let state = match self.request_tx.try_send(service_id.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) => ServiceLogState::Loading,
            Err(TrySendError::Disconnected(_)) => {
                ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::TemporarilyUnavailable,
                    "service log worker is unavailable",
                ))
            }
        };
        ServiceLogSnapshot { service_id, state }
    }

    /// Drain queued responses and return the newest one, if any.
    pub fn try_recv_latest(&self) -> Result<Option<ServiceLogSnapshot>, ServiceLogFailure> {
        let mut latest = None;
        loop {
            match self.result_rx.try_recv() {
                Ok(snapshot) => latest = Some(snapshot),
                Err(TryRecvError::Empty) => return Ok(latest),
                Err(TryRecvError::Disconnected) if latest.is_some() => return Ok(latest),
                Err(TryRecvError::Disconnected) => {
                    return Err(ServiceLogFailure::with_detail(
                        ServiceLogErrorKind::TemporarilyUnavailable,
                        "service log worker result channel disconnected",
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_log_fetch_tests.rs"]
mod tests;
