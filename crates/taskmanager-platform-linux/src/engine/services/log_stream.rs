//! Incremental systemd journal feed used by the Service Details UI.

#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(any(test, feature = "test-support"))]
use std::thread;
use std::time::Duration;

#[cfg(any(test, feature = "test-support"))]
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError, bounded};

use super::log_fetch::{first_message, permission_denied};
#[cfg(target_os = "linux")]
use super::run_command_with_timeout;
use super::target::resolve_active_service_target;
use super::{
    InitSystem, ServiceLogCommandOutcome, ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure,
    ServiceLogLevel, ServiceLogQuery, ServiceLogStreamState, ServiceLogTimeFilter,
};
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(unused_imports))]
use super::{ServiceLogAvailability, ServiceLogFeed, ServiceLogLevelFilter};
#[cfg(any(test, feature = "test-support"))]
use super::{ServiceLogStreamEnd, ServiceLogStreamSnapshot};

#[cfg(target_os = "linux")]
const STREAM_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
const STREAM_LIMIT: &str = "200";

fn level_for_priority(priority: Option<u8>) -> ServiceLogLevel {
    match priority {
        Some(0..=3) => ServiceLogLevel::Error,
        Some(4) => ServiceLogLevel::Warning,
        Some(5..=6) => ServiceLogLevel::Info,
        Some(7) => ServiceLogLevel::Debug,
        _ => ServiceLogLevel::Unknown,
    }
}

pub fn parse_journal_json_lines(text: &str) -> Result<Vec<ServiceLogEntry>, ServiceLogFailure> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let value = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
                ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::ProviderFailed,
                    format!(
                        "journalctl returned invalid JSON on line {}: {error}",
                        index + 1
                    ),
                )
            })?;
            let cursor = value
                .get("__CURSOR")
                .and_then(serde_json::Value::as_str)
                .filter(|cursor| !cursor.is_empty())
                .ok_or_else(|| {
                    ServiceLogFailure::with_detail(
                        ServiceLogErrorKind::ProviderFailed,
                        format!(
                            "journalctl JSON line {} is missing a non-empty cursor",
                            index + 1
                        ),
                    )
                })?
                .to_string();
            let message = value
                .get("MESSAGE")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let realtime_timestamp_micros = value
                .get("__REALTIME_TIMESTAMP")
                .and_then(serde_json::Value::as_str)
                .and_then(|timestamp| timestamp.parse().ok());
            let priority = value
                .get("PRIORITY")
                .and_then(serde_json::Value::as_str)
                .and_then(|priority| priority.parse().ok());
            Ok(ServiceLogEntry {
                cursor,
                realtime_timestamp_micros,
                priority,
                level: level_for_priority(priority),
                message,
            })
        })
        .collect()
}

fn classify_stream_outcome(
    query: &ServiceLogQuery,
    outcome: ServiceLogCommandOutcome,
    now_micros: u64,
) -> ServiceLogStreamState {
    match outcome {
        ServiceLogCommandOutcome::Failure(failure) => ServiceLogStreamState::Unavailable(failure),
        ServiceLogCommandOutcome::Exited {
            success,
            stdout,
            stderr,
        } => {
            if !success {
                let kind = if permission_denied(&stderr) {
                    ServiceLogErrorKind::PermissionDenied
                } else {
                    ServiceLogErrorKind::ProviderFailed
                };
                return ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
                    kind,
                    first_message(&stderr),
                ));
            }
            let entries: Vec<_> = match parse_journal_json_lines(&stdout) {
                Ok(entries) => entries,
                Err(failure) => return ServiceLogStreamState::Unavailable(failure),
            }
            .into_iter()
            .filter(|entry| query.level.matches(entry.priority))
            .filter(|entry| {
                query
                    .time
                    .matches(entry.realtime_timestamp_micros, now_micros)
            })
            .collect();
            ServiceLogStreamState::from_query_entries(query, entries)
        }
    }
}

pub(crate) fn fetch(
    query: &ServiceLogQuery,
    observed_at_ms: u64,
) -> Result<ServiceLogStreamState, taskmanager_platform_contract::ProviderFailure> {
    #[cfg(target_os = "linux")]
    {
        let target = resolve_active_service_target(&query.service_id)?;
        if target.init() != InitSystem::Systemd {
            return Err(taskmanager_platform_contract::ProviderFailure::Unsupported);
        }
        let unit = target.native();
        let mut command = Command::new("journalctl");
        command.args([
            "--unit",
            unit,
            "--lines",
            STREAM_LIMIT,
            "--no-pager",
            "--output=json",
            "--quiet",
        ]);
        if let Some(since) = since_argument(query.time) {
            command.args(["--since", since]);
        }
        if let Some(cursor) = &query.after_cursor {
            command.args(["--after-cursor", cursor]);
        }
        let outcome = run_command_with_timeout(command, STREAM_TIMEOUT);
        Ok(classify_stream_outcome(
            query,
            outcome,
            observed_at_ms.saturating_mul(1_000),
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (query, observed_at_ms);
        Err(taskmanager_platform_contract::ProviderFailure::Unsupported)
    }
}

#[cfg(target_os = "linux")]
const fn since_argument(filter: ServiceLogTimeFilter) -> Option<&'static str> {
    match filter {
        ServiceLogTimeFilter::All => None,
        ServiceLogTimeFilter::LastHour => Some("1 hour ago"),
        ServiceLogTimeFilter::LastDay => Some("1 day ago"),
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct ServiceLogStreamWorker {
    request_tx: Sender<ServiceLogQuery>,
    result_rx: Receiver<ServiceLogStreamSnapshot>,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceLogStreamRequestError {
    Busy,
    Disconnected,
}

#[cfg(any(test, feature = "test-support"))]
impl ServiceLogStreamRequestError {
    #[must_use]
    pub fn into_state(self) -> ServiceLogStreamState {
        match self {
            Self::Busy => ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::TemporarilyUnavailable,
                "service log stream worker is busy",
            )),
            Self::Disconnected => ServiceLogStreamState::Ended(ServiceLogStreamEnd::disconnected(
                "service log stream worker is unavailable",
            )),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ServiceLogStreamWorker {
    /// Construct the legacy bounded worker with an injected runtime clock.
    ///
    /// Product composition uses the shared service runtime lane directly; this
    /// adapter remains available for non-UI consumers without allowing a
    /// provider worker to read wall time on its own.
    pub fn new(clock_ms: impl Fn() -> u64 + Send + 'static) -> Self {
        Self::with_fetcher(clock_ms, |query, observed_at_ms| {
            fetch(query, observed_at_ms).unwrap_or_else(|failure| {
                ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
                    ServiceLogErrorKind::from_failure(failure.kind()),
                    "service log stream target is unavailable",
                ))
            })
        })
    }

    fn with_fetcher(
        clock_ms: impl Fn() -> u64 + Send + 'static,
        fetcher: impl Fn(&ServiceLogQuery, u64) -> ServiceLogStreamState + Send + 'static,
    ) -> Self {
        let (request_tx, request_rx) = bounded::<ServiceLogQuery>(1);
        let (result_tx, result_rx) = bounded::<ServiceLogStreamSnapshot>(1);
        let _ = thread::Builder::new()
            .name("service-log-stream-worker".into())
            .spawn(move || {
                while let Ok(query) = request_rx.recv() {
                    let state = fetcher(&query, clock_ms());
                    if result_tx
                        .send(ServiceLogStreamSnapshot { query, state })
                        .is_err()
                    {
                        break;
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
    fn disconnected() -> Self {
        let (request_tx, request_rx) = bounded::<ServiceLogQuery>(1);
        let (result_tx, result_rx) = bounded::<ServiceLogStreamSnapshot>(1);
        drop(request_rx);
        drop(result_tx);
        Self {
            request_tx,
            result_rx,
        }
    }

    pub fn request(&self, query: ServiceLogQuery) -> Result<(), ServiceLogStreamRequestError> {
        self.request_tx
            .try_send(query)
            .map_err(|error| match error {
                TrySendError::Full(_) => ServiceLogStreamRequestError::Busy,
                TrySendError::Disconnected(_) => ServiceLogStreamRequestError::Disconnected,
            })
    }

    pub fn try_recv(&self) -> Result<Option<ServiceLogStreamSnapshot>, ServiceLogStreamEnd> {
        match self.result_rx.try_recv() {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(ServiceLogStreamEnd::disconnected(
                "service log stream result channel disconnected",
            )),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_log_stream_tests.rs"]
mod tests;
