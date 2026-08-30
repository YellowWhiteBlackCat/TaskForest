//! The SMBIOS memory and RAPL package-power lanes commit only the active
//! request terminal through the shared request sessions; a stale terminal
//! earlier in the same batch cannot overwrite the active request's answer,
//! and a batch carrying no lane events leaves the accepted sessions
//! untouched.

use super::*;

fn smbios_memory_event(
    sequence: u64,
    snapshot: taskmanager_core::SmbiosMemorySnapshot,
) -> taskmanager_application::CorrelatedSmbiosMemoryEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request id"),
            capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: 10,
        },
        taskmanager_application::SmbiosMemoryEvent::Update(snapshot),
    )
}

#[test]
fn smbios_memory_snapshots_commit_only_the_active_request() {
    let mut app = ShellApp::new();
    let stale_attempt = app.begin_smbios_memory_request();
    let attempt = app.begin_smbios_memory_request();
    assert!(!app.accept_smbios_memory_request(
        stale_attempt,
        RequestId::new(4).expect("non-zero fixture request id")
    ));
    assert!(app.accept_smbios_memory_request(
        attempt,
        RequestId::new(5).expect("non-zero fixture request id")
    ));
    let mut batch = PlatformEventBatch::default();
    batch.smbios_memory_events.push(smbios_memory_event(
        4,
        taskmanager_core::SmbiosMemorySnapshot::success(8, 8, Vec::new(), None),
    ));
    batch.smbios_memory_events.push(smbios_memory_event(
        5,
        taskmanager_core::SmbiosMemorySnapshot::failed(
            FailureKind::PermissionDenied,
            "user dismissed the prompt",
        ),
    ));

    app.apply_platform_batch(batch);

    assert!(matches!(
        app.smbios_memory_state(),
        taskmanager_application::SmbiosMemoryState::Failed(failed)
            if matches!(
                &failed.failure,
                taskmanager_application::SmbiosMemoryRequestFailure::Provider(failure)
                    if failure.kind == FailureKind::PermissionDenied
            )
    ));

    app.apply_platform_batch(PlatformEventBatch::default());
    assert!(
        matches!(
            app.smbios_memory_state(),
            taskmanager_application::SmbiosMemoryState::Failed(_)
        ),
        "an empty-events batch must leave the request lifecycle untouched"
    );
}

fn rapl_power_event(
    sequence: u64,
    snapshot: taskmanager_core::RaplPowerSnapshot,
) -> taskmanager_application::CorrelatedRaplPowerEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request id"),
            capability: CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: 10,
        },
        taskmanager_application::RaplPowerEvent::Update(snapshot),
    )
}

#[test]
fn rapl_power_reads_commit_only_the_active_request() {
    let mut app = ShellApp::new();
    let attempt = app.begin_rapl_power_request();
    assert!(app.accept_rapl_power_request(
        attempt,
        RequestId::new(7).expect("non-zero fixture request id")
    ));
    let mut batch = PlatformEventBatch::default();
    batch.smbios_memory_events.push(smbios_memory_event(
        7,
        taskmanager_core::SmbiosMemorySnapshot::success(2, 1, Vec::new(), None),
    ));
    batch.rapl_power_events.push(rapl_power_event(
        7,
        taskmanager_core::RaplPowerSnapshot::success(
            250,
            vec![taskmanager_core::RaplPackageRow {
                name: "package-1".to_owned(),
                power_w: 15.5,
                energy_delta_uj: 3_875_000,
            }],
        ),
    ));

    app.apply_platform_batch(batch);

    assert!(matches!(
        app.rapl_power_state(),
        taskmanager_application::RaplPowerState::Ready(ready)
            if ready.snapshot.packages.len() == 1
                && ready.snapshot.packages[0].power_w == 15.5
    ));
    assert!(
        matches!(
            app.smbios_memory_state(),
            taskmanager_application::SmbiosMemoryState::Closed
        ),
        "a terminal for an inactive smbios session must be dropped, not committed"
    );
}

fn msr_readout_event(
    sequence: u64,
    snapshot: taskmanager_core::MsrReadoutSnapshot,
) -> taskmanager_application::CorrelatedMsrReadoutEvent {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero fixture request id"),
            capability: CapabilityId::TELEMETRY_CPU_MSR,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: 10,
        },
        taskmanager_application::MsrReadoutEvent::Update(snapshot),
    )
}

#[test]
fn msr_readouts_commit_only_the_active_request() {
    let mut app = ShellApp::new();
    let attempt = app.begin_msr_readout_request();
    assert!(app.accept_msr_readout_request(
        attempt,
        RequestId::new(8).expect("non-zero fixture request id")
    ));
    let mut batch = PlatformEventBatch::default();
    batch.rapl_power_events.push(rapl_power_event(
        8,
        taskmanager_core::RaplPowerSnapshot::success(
            250,
            vec![taskmanager_core::RaplPackageRow {
                name: "package-1".to_owned(),
                power_w: 15.5,
                energy_delta_uj: 3_875_000,
            }],
        ),
    ));
    batch.msr_readout_events.push(msr_readout_event(
        8,
        taskmanager_core::MsrReadoutSnapshot::success(vec![taskmanager_core::MsrPackageReadout {
            cpu: 0,
            bclk_mhz: None,
            temperature_c: Some(54.5),
            multiplier: Some(42.0),
            multiplier_min: Some(8.0),
            multiplier_max: Some(58.0),
            vcore_v: Some(1.219),
        }]),
    ));

    app.apply_platform_batch(batch);

    assert!(matches!(
        app.msr_readout_state(),
        taskmanager_application::MsrReadoutState::Ready(ready)
            if ready.snapshot.packages.len() == 1
                && ready.snapshot.packages[0].temperature_c == Some(54.5)
    ));
    assert!(
        matches!(
            app.rapl_power_state(),
            taskmanager_application::RaplPowerState::Closed
        ),
        "a terminal for an inactive rapl session must be dropped, not committed"
    );
}
