//! Capture-only shell seed for the deterministic demo window.
//!
//! The visual-contract capture needs a populated curve window, so the demo
//! composition records a short adjacent telemetry sequence on top of the
//! shared `demo_app()` fixture. This module is a pure data fixture: it owns
//! no UI tree and paints nothing — it only feeds the shared shell the same
//! typed observations a real collection frame would, which keeps it out of
//! the renderer/fold boundary the display gate pins (ARCH.md §8.1).
//!
//! Production collection and the shared fixture's semantics are untouched:
//! `demo_app` starts with one honest sample and the sequence below is
//! appended only inside the capture composition.

use taskmanager_application::{
    InteractionEvent, PendingConfirmation, ProcessInsightsProjection, ProcessInsightsRevision,
};
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{CpuMetrics, ScalarObservation, ScalarObservationGroup};
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::services::{
    ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_shell::ShellApp;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::{
    ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
};

/// Build the capture-only shell with a warm, deterministic graph window.
pub(crate) fn demo_shell() -> ShellApp {
    let mut shell = demo_app();
    if let Some(seed) = shell.projection().snapshot.clone() {
        for offset in 1..=24_u64 {
            let mut next = seed.clone();
            next.timestamp_ms = next
                .timestamp_ms
                .saturating_add(offset.saturating_mul(1_000));
            next.cpu = demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
            record_demo_history_frame(&mut shell, &next, None, None);
        }
    }
    shell
}

/// One adjacent CPU frame: the seed's identity and shape with a deterministic
/// utilization sequence applied to the global and per-core observations.
fn demo_cpu_frame(seed: &CpuMetrics, timestamp_ms: u64, offset: u64) -> CpuMetrics {
    let usage = if offset == 24 {
        37.4
    } else {
        24.0 + f32::from((offset.saturating_mul(7) % 26) as u8)
    };
    let mut observations = seed.scalar_observations().clone();
    observations.global_usage_pct = ScalarObservation::available(usage, timestamp_ms);
    let core_values = (0..seed.current_core_usage_len())
        .map(|index| (usage + (index as f32 * 4.0) - (offset % 3) as f32 * 2.0).clamp(0.0, 100.0))
        .collect();
    observations.core_usage_group = ScalarObservationGroup::available(core_values, timestamp_ms);
    let mut frame = CpuMetrics::from_observations(observations);
    frame.brand = seed.brand.clone();
    frame.frequency_source = seed.frequency_source;
    frame.temperature_source = seed.temperature_source;
    frame.physical_cores = seed.physical_cores;
    frame.logical_cores = seed.logical_cores;
    frame.l1d_cache_kb = seed.l1d_cache_kb;
    frame.l1i_cache_kb = seed.l1i_cache_kb;
    frame.l2_cache_kb = seed.l2_cache_kb;
    frame.l3_cache_kb = seed.l3_cache_kb;
    frame.performance_policy = seed.performance_policy.clone();
    frame.packages = seed.packages.clone();
    frame.idle_states = seed.idle_states.clone();
    frame.interrupts = seed.interrupts.clone();
    frame
}

pub(crate) fn seed_service_log_fixture(shell: &mut ShellApp) {
    let Some(service) = shell.sorted_services().first().cloned() else {
        return;
    };
    let service_id = service.id.clone();
    let _ = shell.open_service_log_for(service_id.clone());
    let lines: &[&str] = &[
        "Started Network Manager.",
        "Reached target Network.",
        "wlan0: link becomes ready",
        "Starting Network Manager Script Dispatcher Service...",
        "Started Network Manager Script Dispatcher Service.",
        "dhcp: lease renewed (3600s)",
        "wlan0: Gained IPv6LL",
        "device (wlan0): state change: activated -> deactivating",
        "device (wlan0): state change: deactivating -> disconnected",
        "wlan0: link is not ready",
        "device (wlan0): state change: disconnected -> prepare",
        "device (wlan0): supplicant interface state: scanning -> authenticating",
        "device (wlan0): supplicant interface state: authenticating -> associating",
        "device (wlan0): supplicant interface state: associating -> 4way_handshake",
        "device (wlan0): supplicant interface state: 4way_handshake -> completed",
        "wlan0: link becomes ready",
        "device (wlan0): state change: config -> activated",
        "dhcp: request granted",
        "address added: 192.168.1.42/24",
        "route added: default via 192.168.1.1",
    ];
    let base_micros = 1_700_000_000_000_000;
    let entries: Vec<ServiceLogEntry> = lines
        .iter()
        .enumerate()
        .map(|(index, message)| ServiceLogEntry {
            cursor: format!("demo:{index:04}"),
            realtime_timestamp_micros: Some(base_micros + index as u64 * 1_500_000),
            priority: Some(6),
            level: ServiceLogLevel::Unknown,
            message: (*message).to_owned(),
        })
        .collect();
    let query = ServiceLogQuery {
        service_id: service_id.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let snapshot = ServiceLogStreamSnapshot {
        query: query.clone(),
        state: ServiceLogStreamState::from_query_entries(&query, entries),
    };
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(snapshot, 1_700_000_000_000);
    }
}

pub(crate) fn seed_capture_confirmation_fixture(shell: &mut ShellApp) {
    let raw = std::env::var("TM_BEVY_CAPTURE_PAGE").unwrap_or_default();
    match raw.trim().to_ascii_lowercase().as_str() {
        "process-force-kill" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process {
                if let Some(target) = FrozenProcessIdentity::from_process(&process) {
                    let intent = ProcessBatchIntent {
                        action: ProcessBatchAction::Kill,
                        scope: ProcessGroupScope::PidAdjacency,
                        targets: vec![target],
                    };
                    let _ =
                        shell
                            .application
                            .interaction
                            .reduce(InteractionEvent::ArmConfirmation(
                                PendingConfirmation::ProcessBatch(intent),
                            ));
                }
            }
        }
        "process-tree-confirm" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process {
                if let Some(target) = FrozenProcessIdentity::from_process(&process) {
                    let intent = ProcessBatchIntent {
                        action: ProcessBatchAction::EndProcessTree,
                        scope: ProcessGroupScope::PidAdjacency,
                        targets: vec![target],
                    };
                    let _ =
                        shell
                            .application
                            .interaction
                            .reduce(InteractionEvent::ArmConfirmation(
                                PendingConfirmation::ProcessBatch(intent),
                            ));
                }
            }
        }
        "process-batch-confirm" => {
            let targets: Vec<_> = shell
                .projection()
                .processes
                .as_ref()
                .map(|p| {
                    p.iter()
                        .take(3)
                        .filter_map(FrozenProcessIdentity::from_process)
                        .collect()
                })
                .unwrap_or_default();
            if !targets.is_empty() {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::Kill,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets,
                };
                let _ = shell
                    .application
                    .interaction
                    .reduce(InteractionEvent::ArmConfirmation(
                        PendingConfirmation::ProcessBatch(intent),
                    ));
            }
        }
        "smart-self-test-confirm" => {
            let intent = SmartSelfTestIntent {
                device_id: DeviceId::new("disk:demo:nvme0"),
                device_generation: DeviceGeneration::new(1),
                device_key: StorageDeviceKey::new("nvme0n1"),
                display_name: "TiPro9000 2TB".into(),
                kind: SmartSelfTestKind::Short,
            };
            shell.arm_smart_self_test(intent);
        }
        "process-network-details"
        | "process-gpu-details"
        | "process-resource-limits"
        | "process-isolation"
        | "process-properties-performance"
        | "process-memory-pss-swap" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process {
                if let Some(target) = FrozenProcessIdentity::from_process(&process) {
                    shell.application.selected_process = Some(target.clone());
                    let revision = ProcessInsightsRevision::new(1);
                    let mut tracker = ProcessInsightsProjection::default();
                    tracker.begin(target, revision);
                    if let Some(projection) = tracker.snapshot() {
                        seed_projection_fact(
                            shell,
                            ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}
