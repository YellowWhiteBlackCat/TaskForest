//! Windows Event Log snapshot and stream provider runtime.

use taskmanager_core::core::services::ServiceLogQuery;
use taskmanager_core::{ServiceId, ServiceLogState, ServiceLogStreamState};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::{ServiceLogSnapshotProvider, ServiceLogStreamProvider};
#[cfg(windows)]
use taskmanager_windows_api::{WindowsApiError, WindowsEventLogQuery, query_event_log};

#[cfg(windows)]
use super::{
    SERVICE_LOG_CHANNEL, SERVICE_LOG_SNAPSHOT_LIMIT, SERVICE_LOG_STREAM_LIMIT, event_log_entries,
    event_log_lines, parse_stream_cursor, valid_service_id,
};
use super::{WinServiceLogSnapshotProvider, WinServiceLogStreamProvider};

impl ServiceLogSnapshotProvider for WinServiceLogSnapshotProvider {
    fn snapshot(&mut self, service_id: &ServiceId) -> Result<ServiceLogState, ProviderFailure> {
        #[cfg(windows)]
        {
            windows_service_log_snapshot(service_id)
        }
        #[cfg(not(windows))]
        {
            let _ = service_id;
            // No Windows Event Log on this host: the native source is a
            // missing dependency, not an empty success.
            Err(ProviderFailure::MissingDependency)
        }
    }
}

impl ServiceLogStreamProvider for WinServiceLogStreamProvider {
    fn stream(
        &mut self,
        query: &ServiceLogQuery,
        observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, ProviderFailure> {
        #[cfg(windows)]
        {
            windows_service_log_stream(query, observed_at_ms)
        }
        #[cfg(not(windows))]
        {
            let _ = (query, observed_at_ms);
            Err(ProviderFailure::MissingDependency)
        }
    }
}

#[cfg(windows)]
fn windows_service_log_snapshot(
    service_id: &ServiceId,
) -> Result<ServiceLogState, ProviderFailure> {
    let name = valid_service_id(service_id)?;
    let query = event_log_query_for(name, None);
    let entries =
        query_event_log(&query, SERVICE_LOG_SNAPSHOT_LIMIT).map_err(map_event_log_failure)?;
    Ok(ServiceLogState::from_lines(event_log_lines(&entries)))
}

// Dead until the registration swap in `provider.rs` (integrator-owned)
// constructs `WinServiceLogStreamProvider` in production.
#[cfg(windows)]
#[allow(dead_code)]
fn windows_service_log_stream(
    query: &ServiceLogQuery,
    observed_at_ms: u64,
) -> Result<ServiceLogStreamState, ProviderFailure> {
    let name = valid_service_id(&query.service_id)?;
    let after_record_id = parse_stream_cursor(query.after_cursor.as_deref())?;
    let windows_query = event_log_query_for(name, after_record_id);
    let entries =
        query_event_log(&windows_query, SERVICE_LOG_STREAM_LIMIT).map_err(map_event_log_failure)?;
    let now_micros = observed_at_ms.saturating_mul(1_000);
    let mapped = event_log_entries(entries)
        .into_iter()
        .filter(|entry| query.level.matches(entry.priority))
        .filter(|entry| {
            query
                .time
                .matches(entry.realtime_timestamp_micros, now_micros)
        })
        .collect();
    Ok(ServiceLogStreamState::from_query_entries(query, mapped))
}

#[cfg(windows)]
fn event_log_query_for(service_name: &str, after_record_id: Option<u64>) -> WindowsEventLogQuery {
    WindowsEventLogQuery {
        channel: SERVICE_LOG_CHANNEL.to_string(),
        provider: Some(service_name.to_string()),
        event_id: None,
        after_record_id,
    }
}

#[cfg(windows)]
fn map_event_log_failure(error: WindowsApiError) -> ProviderFailure {
    match error {
        WindowsApiError::Unsupported => ProviderFailure::MissingDependency,
        WindowsApiError::PermissionDenied => ProviderFailure::PermissionDenied,
        WindowsApiError::InvalidInput | WindowsApiError::IdentityChanged => {
            ProviderFailure::IdentityChanged
        }
        WindowsApiError::InvalidText | WindowsApiError::ResourceLimit => {
            ProviderFailure::ProviderFault
        }
        WindowsApiError::QueryFailed => ProviderFailure::TemporarilyUnavailable,
    }
}
