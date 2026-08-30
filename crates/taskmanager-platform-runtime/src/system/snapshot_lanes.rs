//! SMBIOS memory, RAPL package-power, and CPU MSR-readout snapshot lanes: the
//! twin of the per-engine GPU lane for the three system-scoped privileged
//! helper reads (ADR-023 Boundary 2). Split into a sibling module so
//! `system.rs` stays inside the workspace file-line budget.

use std::sync::Arc;

use crossbeam_channel::Receiver;
use taskmanager_application::{MsrReadoutEvent, PlatformEvent, RaplPowerEvent, SmbiosMemoryEvent};
use taskmanager_core::{MsrReadoutSnapshot, RaplPowerSnapshot, SmbiosMemorySnapshot};
use taskmanager_platform_contract::{CapabilityId, ProviderFailure};

use super::{MsrReadoutRequest, RaplPowerRequest, SmbiosMemoryRequest};
use crate::delivery::{recv_or_shutdown_with_idle, spawn_or_register_lane};
use crate::health::CapabilityHealth;
use crate::{Queued, RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError};

type SmbiosMemoryExecutor =
    dyn FnMut() -> Result<SmbiosMemorySnapshot, ProviderFailure> + Send + 'static;
type RaplPowerExecutor = dyn FnMut() -> Result<RaplPowerSnapshot, ProviderFailure> + Send + 'static;
type MsrReadoutExecutor =
    dyn FnMut() -> Result<MsrReadoutSnapshot, ProviderFailure> + Send + 'static;

/// Spawn the SMBIOS memory-inventory lane: one bounded executor call per
/// queued request, answered with exactly one correlated publication — real
/// slot/module rows on success, a typed failure snapshot otherwise (never a
/// fabricated inventory). Health is recorded per answer so the catalog
/// reflects the latest helper outcome, mirroring the engine-rows lane.
pub(super) fn spawn_smbios_memory_lane(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<SmbiosMemoryRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: Box<SmbiosMemoryExecutor>,
) -> Result<(), WorkerSpawnError> {
    let lane = CapabilityId::TELEMETRY_MEMORY_SMBIOS.to_string();
    spawn_or_register_lane(
        workers,
        Some(CapabilityId::TELEMETRY_MEMORY_SMBIOS),
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = crate::delivery::LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let (snapshot, health) = match crate::delivery::execute_isolated(
                    &panic_notes,
                    crate::delivery::ProviderPanicContext {
                        lane: lane.clone(),
                        capability: queued.capability.clone(),
                        request_id: queued.request_id,
                    },
                    || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute()
                    },
                ) {
                    Ok(snapshot) => (snapshot, CapabilityHealth::Available),
                    Err(failure) => (
                        SmbiosMemorySnapshot::failed(
                            failure.kind(),
                            format!("provider failure: {failure:?}"),
                        ),
                        CapabilityHealth::Unavailable(failure),
                    ),
                };
                let event = PlatformEvent::SmbiosMemory(SmbiosMemoryEvent::Update(snapshot));
                if crate::delivery::shutdown_requested(&shutdown)
                    || publisher
                        .publish_health(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            event,
                            health,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}

/// Spawn the CPU package-power lane: one bounded executor call per queued
/// request, answered with exactly one correlated publication — real
/// per-package watt figures on success, a typed failure snapshot otherwise
/// (never a fabricated zero-watt reading). Health is recorded per answer so
/// the catalog reflects the latest helper outcome, mirroring the engine-rows
/// lane.
pub(super) fn spawn_rapl_power_lane(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<RaplPowerRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: Box<RaplPowerExecutor>,
) -> Result<(), WorkerSpawnError> {
    let lane = CapabilityId::TELEMETRY_CPU_PACKAGE_POWER.to_string();
    spawn_or_register_lane(
        workers,
        Some(CapabilityId::TELEMETRY_CPU_PACKAGE_POWER),
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = crate::delivery::LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let (snapshot, health) = match crate::delivery::execute_isolated(
                    &panic_notes,
                    crate::delivery::ProviderPanicContext {
                        lane: lane.clone(),
                        capability: queued.capability.clone(),
                        request_id: queued.request_id,
                    },
                    || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute()
                    },
                ) {
                    Ok(snapshot) => (snapshot, CapabilityHealth::Available),
                    Err(failure) => (
                        RaplPowerSnapshot::failed(
                            failure.kind(),
                            format!("provider failure: {failure:?}"),
                        ),
                        CapabilityHealth::Unavailable(failure),
                    ),
                };
                let event = PlatformEvent::RaplPower(RaplPowerEvent::Update(snapshot));
                if crate::delivery::shutdown_requested(&shutdown)
                    || publisher
                        .publish_health(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            event,
                            health,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}

/// Spawn the CPU MSR-readout lane: one bounded executor call per queued
/// request, answered with exactly one correlated publication — real per-node
/// register rows on success, a typed failure snapshot otherwise (never a
/// fabricated zero for a register the CPU does not implement). Health is
/// recorded per answer so the catalog reflects the latest helper outcome,
/// mirroring the engine-rows lane.
pub(super) fn spawn_msr_readout_lane(
    workers: &WorkerRuntime,
    receiver: Receiver<Queued<MsrReadoutRequest>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: Box<MsrReadoutExecutor>,
) -> Result<(), WorkerSpawnError> {
    let lane = CapabilityId::TELEMETRY_CPU_MSR.to_string();
    spawn_or_register_lane(
        workers,
        Some(CapabilityId::TELEMETRY_CPU_MSR),
        receiver,
        publisher,
        execute,
        move |receiver, execute, publisher, shutdown, idle_timeout| {
            let _lane_exit = crate::delivery::LaneExitGuard::new(publisher.lane_exit_counter());
            let panic_notes = publisher.panic_ledger();
            while let Some(queued) = recv_or_shutdown_with_idle(&receiver, &shutdown, idle_timeout)
            {
                let (snapshot, health) = match crate::delivery::execute_isolated(
                    &panic_notes,
                    crate::delivery::ProviderPanicContext {
                        lane: lane.clone(),
                        capability: queued.capability.clone(),
                        request_id: queued.request_id,
                    },
                    || {
                        let mut execute = execute
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        execute()
                    },
                ) {
                    Ok(snapshot) => (snapshot, CapabilityHealth::Available),
                    Err(failure) => (
                        MsrReadoutSnapshot::failed(
                            failure.kind(),
                            format!("provider failure: {failure:?}"),
                        ),
                        CapabilityHealth::Unavailable(failure),
                    ),
                };
                let event = PlatformEvent::MsrReadout(MsrReadoutEvent::Update(snapshot));
                if crate::delivery::shutdown_requested(&shutdown)
                    || publisher
                        .publish_health(
                            queued.request_id,
                            queued.capability,
                            queued.provider,
                            event,
                            health,
                        )
                        .is_stop()
                {
                    break;
                }
            }
        },
    )
}
