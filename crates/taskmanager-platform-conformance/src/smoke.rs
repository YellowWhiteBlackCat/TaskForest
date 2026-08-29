//! Live-runtime drain contract used by every platform's conformance test.

use std::time::{Duration, Instant};

use taskmanager_application::{PlatformClient, PlatformEventBatch, ProcessEvent};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::process::ProcessItem;
use taskmanager_platform_contract::{EventPortError, OperationFailure, RetryDisposition};

/// Batches and failures collected while waiting for the expected event count.
#[derive(Default)]
pub struct LiveDrain {
    pub failures: Vec<OperationFailure>,
    pub batches: Vec<PlatformEventBatch>,
    pub arrived_total: usize,
    pub deadline_reached: bool,
}

/// Drain the event port until at least `expected_total_events` envelopes
/// arrived and the port reports empty, or the deadline passes.
pub fn drain_until(
    client: &mut PlatformClient,
    expected_total_events: usize,
    deadline: Duration,
    poll: Duration,
) -> Result<LiveDrain, EventPortError> {
    let started = Instant::now();
    let mut drain = LiveDrain::default();
    loop {
        let batch = client.try_drain()?;
        let count = batch_event_count(&batch);
        drain.arrived_total += count;
        drain.failures.extend(batch.failures.iter().cloned());
        drain.batches.push(batch);
        if drain.arrived_total >= expected_total_events && count == 0 {
            break;
        }
        if started.elapsed() >= deadline {
            drain.deadline_reached = true;
            break;
        }
        std::thread::sleep(poll);
    }
    Ok(drain)
}

/// Drain until a process snapshot arrives (the canonical live data source for
/// every adapter) or the deadline passes. Failures and other events are still
/// collected so the caller can assert attribution and honesty.
pub fn drain_until_process_rows(
    client: &mut PlatformClient,
    deadline: Duration,
    poll: Duration,
) -> Result<LiveDrain, EventPortError> {
    let started = Instant::now();
    let mut drain = LiveDrain::default();
    loop {
        let batch = client.try_drain()?;
        drain.arrived_total += batch_event_count(&batch);
        drain.failures.extend(batch.failures.iter().cloned());
        drain.batches.push(batch);
        if !collect_process_rows(&drain).is_empty() {
            break;
        }
        if started.elapsed() >= deadline {
            drain.deadline_reached = true;
            break;
        }
        std::thread::sleep(poll);
    }
    Ok(drain)
}

/// Total raw provider envelopes observed in one batch, failures included.
/// Application-owned projections are excluded: they never represent provider
/// payloads and would make the drain counter drift from provider activity.
pub fn batch_event_count(batch: &PlatformEventBatch) -> usize {
    batch.system_telemetry_outcomes.len()
        + batch.hardware_inventory_events.len()
        + batch.containers_events.len()
        + batch.process_events.len()
        + batch.process_affinity_events.len()
        + batch.service_events.len()
        + batch.startup_events.len()
        + batch.session_events.len()
        + batch.shell_events.len()
        + batch.setup_script_events.len()
        + batch.desktop_appearance_events.len()
        + batch.storage_health_events.len()
        + batch.directory_usage_events.len()
        + batch.sensor_events.len()
        + batch.power_supply_events.len()
        + batch.smart_events.len()
        + batch.gpu_engine_rows_events.len()
        + batch.npu_inventory_events.len()
        + batch.failures.len()
}

/// A live runtime must publish events or typed failures within the deadline,
/// attribute every failure to its owning platform prefix, and never mark an
/// unsupported capability retryable.
pub fn assert_live_smoke_ok(drain: &LiveDrain, provider_prefix: &str) -> Result<(), String> {
    if drain.arrived_total == 0 && drain.failures.is_empty() {
        return Err("live runtime published neither events nor typed failures".to_owned());
    }
    if drain.deadline_reached {
        return Err(
            "live runtime did not publish the expected events within the deadline".to_owned(),
        );
    }
    for failure in &drain.failures {
        let Some(provider) = failure.provider.as_ref() else {
            return Err(format!("{} failure must be attributed", failure.capability));
        };
        if !provider.as_str().starts_with(provider_prefix) {
            return Err(format!("{} attributed to {provider}", failure.capability));
        }
        if failure.kind == FailureKind::Unsupported && failure.retry != RetryDisposition::Never {
            return Err(format!(
                "unsupported {} must not be retried",
                failure.capability
            ));
        }
    }
    Ok(())
}

/// Collect every process snapshot row delivered by the drained batches.
pub fn collect_process_rows(drain: &LiveDrain) -> Vec<ProcessItem> {
    let mut rows = Vec::new();
    for batch in &drain.batches {
        for event in &batch.process_events {
            if let ProcessEvent::Snapshot(snapshot) = &event.event {
                rows.extend(snapshot.iter().cloned());
            }
        }
    }
    rows
}

#[cfg(test)]
#[path = "../tests/headless/smoke_contract.rs"]
mod tests;
