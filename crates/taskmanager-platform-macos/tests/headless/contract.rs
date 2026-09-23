//! Second operating-system adapter contract proof.
//!
//! The macOS adapter composes the complete standard product surface from the
//! shared runtime (ADR-019): safe-crate implementations cover most domains,
//! and capabilities without a safe source (GPU, per-process network/GPU/
//! isolation, affinity, service dependencies, log streaming, session
//! control, plus the registered-pending open-files / desktop-notify /
//! first-run-setup optional facets, G-05) publish typed `Unsupported`
//! outcomes attributed to a `macos.*` identity. No fabricated observation may
//! reach the event port.
//!
//! These tests run on the macOS CI runner (native target) and on the Linux
//! workspace suite (sysinfo/battery providers work there too; launchctl/
//! smartctl shell-outs degrade to honest MissingDependency failures), which
//! makes the contract proof repeatable on every gate.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use taskmanager_application::SmartEvent;
use taskmanager_application::{
    CommandLaunchRequest, ContainerRollupEvent, DesktopNotificationRequest, EnvironmentFacets,
    IntegrationFacets, LatestControlRequest, PlatformClient, PlatformEventBatch, PlatformFacets,
    PowerFacets, ProcessAffinityControlRequest, ProcessControlRequest, ProcessFacets,
    ProcessResourceControlRequest, RefreshRequest, ResourceRevealRequest, SensorFacets,
    ServiceControlOutcome, ServiceControlRequest, ServiceEvent, ServiceFacets, ServiceUpdate,
    SessionControlOutcome, SessionControlRequest, SessionEvent, SetupScriptRequest,
    SmartObservationBatch, StorageFacets, SystemFacets,
};
use taskmanager_core::DeviceStatus;
use taskmanager_core::core::alerts::AlertSeverity;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::process_telemetry::ResourceGroupLimitRequest;
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_core::core::target::{ServiceId, SessionId};
use taskmanager_platform_conformance::assert_capability_surface_matches_catalog;
use taskmanager_platform_contract::{
    CapabilityId, CapabilitySnapshot, CapabilityStatus, OperationFailure, PlatformAxis,
    PlatformSource, RetryDisposition,
};
use taskmanager_platform_macos::{MacOsPlatformRuntime, capability_surface};

const DRAIN_DEADLINE: Duration = Duration::from_secs(5);
const DRAIN_POLL: Duration = Duration::from_millis(5);

/// Every standard capability with the provider identity the macOS adapter
/// must attribute to it. The 43 entries are the complete product surface:
/// 38 always-required lanes plus the five optional facets (directory usage
/// real; open-files / desktop-notify / first-run-setup registered-pending
/// with typed `Unsupported` outcomes, G-05).
const STANDARD_SURFACE: &[(&str, &str)] = &[
    ("telemetry.host", "macos.system.host"),
    ("telemetry.cpu", "macos.system.cpu"),
    ("telemetry.memory", "macos.system.memory"),
    ("telemetry.storage", "macos.system.storage"),
    ("telemetry.network", "macos.system.network"),
    ("telemetry.gpu", "macos.system.gpu"),
    ("hardware.inventory", "macos.hardware.inventory"),
    ("containers.rollup", "macos.containers.unavailable"),
    ("process.list", "macos.process.list"),
    ("process.control", "macos.process.control"),
    ("process.insights.network", "macos.process.insights.network"),
    ("process.insights.gpu", "macos.process.insights.gpu"),
    (
        "process.insights.resources",
        "macos.process.insights.resources",
    ),
    (
        "process.insights.isolation",
        "macos.process.insights.isolation",
    ),
    ("process.insights.threads", "macos.process.insights.threads"),
    (
        "process.insights.open_files",
        "macos.process.insights.open_files",
    ),
    ("process.affinity", "macos.process.affinity"),
    ("process.affinity.control", "macos.process.affinity.control"),
    ("process.resource.control", "macos.process.resource.control"),
    (
        "process.network.escalation",
        "macos.process.network.escalation",
    ),
    ("services", "macos.service.inventory"),
    ("services.dependencies", "macos.service.dependencies"),
    ("services.control", "macos.service.control"),
    ("services.logs", "macos.service.logs.snapshot"),
    ("services.logs.stream", "macos.service.logs.stream"),
    ("startup", "macos.startup.inventory"),
    ("startup.evidence", "macos.startup.evidence"),
    ("startup.control", "macos.startup.control"),
    ("sessions", "macos.session.inventory"),
    ("sessions.control", "macos.session.control"),
    ("shell.command.launch", "macos.shell.command"),
    ("shell.resource.reveal", "macos.shell.resource-reveal"),
    ("shell.url.open", "macos.shell.url-open"),
    ("desktop.appearance", "macos.desktop.appearance"),
    ("alerts.notify", "macos.alerts.desktop-notification"),
    ("first-run.setup", "macos.first-run.setup-script"),
    ("storage.health", "macos.storage.filesystem.registry"),
    ("storage.smart", "macos.storage.smart.observation"),
    ("storage.smart.control", "macos.storage.smart.control"),
    ("sensors", "macos.sensor.registry"),
    ("hardware.power-supplies", "macos.power-supply.registry"),
    (
        "filesystem.directory.usage",
        "macos.storage.directory-usage",
    ),
    ("telemetry.gpu.engines", "macos.system.gpu-engines"),
    ("accelerator.npu", "macos.accelerator.npu"),
    ("telemetry.memory.smbios", "macos.telemetry.memory.smbios"),
    (
        "telemetry.cpu.package_power",
        "macos.telemetry.cpu.package-power",
    ),
    ("telemetry.cpu.msr", "macos.telemetry.cpu.msr"),
    (
        "telemetry.cpu.throttle",
        "macos.telemetry.cpu.throttle-pending",
    ),
];

/// Capabilities with NO safe macOS source: they must complete with a typed
/// unsupported outcome everywhere (ADR-019). The leading five are the
/// registered-pending optional facets (G-05): present in the catalog, honest
/// `Unsupported` on submission.
const PENDING_CAPABILITIES: &[&str] = &[
    "telemetry.gpu.engines",
    "accelerator.npu",
    "telemetry.memory.smbios",
    "telemetry.cpu.package_power",
    "telemetry.cpu.msr",
    // The cumulative Linux `thermal_throttle` counters have no macOS source
    // with the same semantics (IOKit speed-limit/thermal-pressure
    // notifications are instantaneous); the lane stays registered-pending.
    "telemetry.cpu.throttle",
    "containers.rollup",
    // macOS does not currently expose an authoritative process start token
    // through the safe provider boundary. Target mutation/read/reveal must
    // therefore fail closed instead of acting on a PID-only guess.
    "process.control",
    "process.insights.resources",
    "shell.resource.reveal",
    "process.insights.network",
    "process.insights.gpu",
    "process.insights.isolation",
    "process.insights.threads",
    "process.insights.open_files",
    "process.affinity",
    "process.affinity.control",
    "process.resource.control",
    // The per-process byte-accounting escalation chain (AF_PACKET /
    // SCM_RIGHTS) is Linux-only; the registered provider answers the typed
    // `Unsupported` outcome like every other pending lane.
    "process.network.escalation",
    "services.dependencies",
    "services.logs.stream",
    "sessions.control",
    "alerts.notify",
    "first-run.setup",
];

/// Safe-crate capabilities: submissions must be accepted and complete either
/// with real data or an honest non-unsupported failure (MissingDependency /
/// TemporarilyUnavailable when the host lacks the tool).
fn assert_honest_real_failure(failure: &OperationFailure) {
    assert_ne!(
        failure.kind,
        FailureKind::Unsupported,
        "{} must not degrade an implemented capability to Unsupported",
        failure.capability
    );
    let provider = failure
        .provider
        .as_ref()
        .unwrap_or_else(|| panic!("{} failure must be attributed", failure.capability));
    assert!(
        provider.as_str().starts_with("macos."),
        "{} attributed to {provider}",
        failure.capability
    );
}

fn frozen_process() -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "contract-fixture", 7, 700)
        .expect("fixture identity")
}

/// Every batch drained while waiting for the expected event count.
struct Drains {
    failures: Vec<OperationFailure>,
    batches: Vec<PlatformEventBatch>,
}

/// Drain until at least `expected_total_events` envelopes arrived and the event
/// port reported empty, or the deadline passes.
fn drain_until(client: &mut PlatformClient, expected_total_events: usize) -> Drains {
    let deadline = Instant::now() + DRAIN_DEADLINE;
    let mut drains = Drains {
        failures: Vec::new(),
        batches: Vec::new(),
    };
    let mut arrived_total = 0;
    loop {
        let batch = client.try_drain().expect("event port must stay live");
        let empty = batch_event_count(&batch) == 0;
        arrived_total += batch_event_count(&batch);
        drains.failures.extend(batch.failures.iter().cloned());
        drains.batches.push(batch);
        if arrived_total >= expected_total_events && empty {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(DRAIN_POLL);
    }
    drains
}

/// Total envelopes observed in one batch, failures included.
fn batch_event_count(batch: &PlatformEventBatch) -> usize {
    batch.system_telemetry_outcomes.len()
        + batch.hardware_inventory_events.len()
        + batch.containers_events.len()
        + batch.process_events.len()
        + batch.process_affinity_events.len()
        + batch.service_events.len()
        + batch.startup_events.len()
        + batch.session_events.len()
        + batch.shell_events.len()
        + batch.desktop_appearance_events.len()
        + batch.storage_health_events.len()
        + batch.sensor_events.len()
        + batch.power_supply_events.len()
        + batch.smart_events.len()
        + batch.failures.len()
}

/// Capability ids of raw provider payloads delivered as successful events.
/// Projections and correlated outcomes are application-owned state and
/// excluded; they never represent fabricated provider data.
fn raw_success_capabilities(drains: &Drains) -> Vec<String> {
    let mut capabilities = Vec::new();
    for batch in &drains.batches {
        for event in &batch.hardware_inventory_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.containers_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.process_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.process_affinity_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.service_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.startup_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.session_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.shell_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.desktop_appearance_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.storage_health_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.sensor_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.power_supply_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
        for event in &batch.smart_events {
            capabilities.push(event.capability.as_str().to_owned());
        }
    }
    capabilities
}

/// SMART tracking batches that crossed the batch boundary. The shared tracking
/// state machine publishes an empty batch when no self-test job is tracked;
/// that is a typed tracking-state event, not a fabricated SMART reading.
fn smart_batches(drains: &Drains) -> Vec<&SmartObservationBatch> {
    drains
        .batches
        .iter()
        .flat_map(|batch| &batch.smart_events)
        .map(|event| match &event.event {
            SmartEvent::Batch(batch) => batch,
        })
        .collect()
}

fn assert_honest_failure(failure: &OperationFailure) {
    assert_eq!(
        failure.kind,
        FailureKind::Unsupported,
        "macOS adapter must publish typed unsupported outcomes, not data or other failures"
    );
    assert_eq!(
        failure.retry,
        RetryDisposition::Never,
        "an unsupported macOS capability must not be retried"
    );
    let provider = failure
        .provider
        .as_ref()
        .unwrap_or_else(|| panic!("{} failure must be attributed", failure.capability));
    assert!(
        provider.as_str().starts_with("macos."),
        "{} attributed to {provider}",
        failure.capability
    );
}

fn assert_standard_descriptors(snapshot: &CapabilitySnapshot) {
    assert_eq!(
        snapshot.registered().count(),
        STANDARD_SURFACE.len(),
        "the second OS adapter must register exactly the standard product surface"
    );
    for (capability, provider) in STANDARD_SURFACE {
        let descriptor = snapshot
            .get(&CapabilityId::borrowed(capability))
            .unwrap_or_else(|| panic!("missing capability descriptor {capability}"));
        assert!(
            CapabilityId::borrowed(capability).is_expected(),
            "{capability} must be part of the product-expected surface"
        );
        assert_eq!(
            descriptor.status,
            CapabilityStatus::TemporarilyUnavailable,
            "a fresh descriptor must not claim availability before any observation"
        );
        assert_eq!(
            descriptor.providers,
            [ProviderId::borrowed(provider)],
            "{capability} must be owned by its macos.* provider"
        );
        assert!(descriptor.last_success_at_ms.is_none());
    }

    // Every product capability this adapter does not register answers with its
    // typed absence: no silent omission, no fabricated provider, no fabricated
    // success.
    for descriptor in snapshot.typed_absences() {
        assert!(
            descriptor.id.is_expected(),
            "an unregistered capability must still be product-expected: {}",
            descriptor.id
        );
        assert_eq!(descriptor.status, CapabilityStatus::Unsupported);
        assert!(descriptor.providers.is_empty());
        assert_eq!(descriptor.observed_at_ms, 0);
        assert!(descriptor.last_success_at_ms.is_none());
    }
    for expected in CapabilityId::EXPECTED_SURFACE {
        assert!(
            snapshot.get(&expected).is_some(),
            "the product surface must stay addressable: {expected}"
        );
    }
}

/// Grouped by facet owner so every flat assert battery stays under the
/// complexity ratchet while the census itself stays complete.
fn assert_complete_facets(facets: &PlatformFacets) {
    assert_system_facets(facets.system());
    assert_process_facets(facets.process());
    assert_service_facets(facets.service());
    assert_environment_facets(facets.environment());
    assert_integration_facets(facets.integration());
    assert_storage_facets(facets.storage());
    assert_sensor_and_power_facets(facets.sensor(), facets.power());
}

fn assert_system_facets(facets: &SystemFacets) {
    assert!(facets.host().is_some());
    assert!(facets.cpu().is_some());
    assert!(facets.memory().is_some());
    assert!(facets.storage().is_some());
    assert!(facets.network().is_some());
    assert!(facets.gpu().is_some());
    assert!(facets.hardware_inventory().is_some());
    assert!(facets.smbios_memory().is_some());
    assert!(facets.rapl_power().is_some());
    assert!(facets.msr_readout().is_some());
    assert!(facets.cpu_throttle().is_some());
    assert!(facets.containers().is_some());
}

fn assert_process_facets(facets: &ProcessFacets) {
    assert!(facets.list().is_some());
    assert!(facets.control().is_some());
    assert!(facets.network().is_some());
    assert!(facets.gpu().is_some());
    assert!(facets.resources().is_some());
    assert!(facets.isolation().is_some());
    assert!(facets.threads().is_some());
    assert!(facets.affinity().is_some());
    assert!(facets.affinity_control().is_some());
    assert!(facets.resource_control().is_some());
    assert!(facets.open_files().is_some());
}

fn assert_service_facets(facets: &ServiceFacets) {
    assert!(facets.inventory().is_some());
    assert!(facets.dependencies().is_some());
    assert!(facets.control().is_some());
    assert!(facets.log_snapshot().is_some());
    assert!(facets.log_stream().is_some());
}

fn assert_environment_facets(facets: &EnvironmentFacets) {
    assert!(facets.startup_inventory().is_some());
    assert!(facets.startup_evidence().is_some());
    assert!(facets.startup_control().is_some());
    assert!(facets.session_inventory().is_some());
    assert!(facets.session_control().is_some());
}

fn assert_integration_facets(facets: &IntegrationFacets) {
    assert!(facets.command_launch().is_some());
    assert!(facets.resource_reveal().is_some());
    assert!(facets.url_open().is_some());
    assert!(facets.desktop_appearance().is_some());
    assert!(facets.desktop_notification().is_some());
    assert!(facets.setup_script().is_some());
}

fn assert_storage_facets(facets: &StorageFacets) {
    assert!(facets.health().is_some());
    assert!(facets.smart_observation().is_some());
    assert!(facets.smart_control().is_some());
}

fn assert_sensor_and_power_facets(sensor: &SensorFacets, power: &PowerFacets) {
    assert!(sensor.observation().is_some());
    assert!(power.supplies().is_some());
}

#[test]
fn complete_standard_surface_composes_with_descriptors_and_facets() {
    let handle = MacOsPlatformRuntime::spawn().expect("complete macOS composition");

    let snapshot = handle.capabilities().snapshot();
    assert_standard_descriptors(&snapshot);
    assert_complete_facets(handle.facets());
}

#[test]
fn observation_surface_accepts_submissions_and_publishes_only_typed_outcomes() {
    let mut client =
        PlatformClient::new(MacOsPlatformRuntime::spawn().expect("complete macOS composition"));

    let submissions = client.request_refresh(RefreshRequest::All, 1);
    assert_eq!(
        submissions.len(),
        17,
        "six telemetry domains plus eleven lists (incl. containers rollup)"
    );
    assert!(
        submissions.into_iter().all(|result| result.is_ok()),
        "every observation lane must accept submissions on the second OS"
    );
    let insights = client
        .submit_process_insights(frozen_process(), 1)
        .expect("process insights revision");
    assert!(insights.network.is_ok());
    assert!(insights.gpu.is_ok());
    assert!(insights.resources.is_ok());
    assert!(insights.isolation.is_ok());
    assert!(insights.threads.is_ok());
    assert!(
        insights.open_files.is_ok(),
        "the registered-pending open-files facet must accept submissions"
    );

    // 6 telemetry + hardware + containers rollup + process list + 6 insights
    // (incl. the registered-pending open-files facet) + service inventory
    // + startup inventory/evidence + sessions + storage health + sensors
    // + power supplies + SMART observation = 23 accepted observations. The
    // SMART tracking state machine publishes an empty batch (no tracked
    // jobs); pending capabilities publish typed unsupported failures,
    // implemented capabilities either publish data or degrade to an honest
    // non-unsupported failure (e.g. MissingDependency without
    // launchctl/smartctl).
    let drains = drain_until(&mut client, 23);
    for failure in &drains.failures {
        if PENDING_CAPABILITIES.contains(&failure.capability.as_str()) {
            assert_honest_failure(failure);
        } else {
            assert_honest_real_failure(failure);
        }
    }
    let pending_failures: Vec<&str> = drains
        .failures
        .iter()
        .map(|failure| failure.capability.as_str())
        .filter(|capability| PENDING_CAPABILITIES.contains(capability))
        .collect();
    assert_eq!(
        pending_failures.len(),
        6,
        "six observation lanes must publish typed unsupported failures; startup.evidence and telemetry.gpu now have safe sources; containers.rollup publishes a typed unavailable observation instead of a lane failure; target-scoped resources fail closed without an authoritative process token; received: {pending_failures:?}"
    );
    assert!(
        pending_failures.contains(&"process.insights.open_files"),
        "the registered-pending open-files facet must publish its typed Unsupported failure: {pending_failures:?}"
    );

    // containers.rollup rides the SNAPSHOT lane (not the failure lane): macOS
    // has no cgroup-v2, so the provider returns a typed-unavailable rollup
    // whose DeviceState is Unsupported. The snapshot must reach the batch so
    // the page shows the honest "containers.unsupported" reason — never the
    // doubly-dishonest empty-healthy "no containers detected" default that an
    // Err(Unsupported) would leave behind.
    let containers_rollup = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.containers_events)
        .map(|event| match &event.event {
            ContainerRollupEvent::Snapshot(rollup) => rollup.as_ref(),
        })
        .next()
        .expect("containers.rollup must publish a typed-unavailable snapshot");
    assert_eq!(
        containers_rollup.state.status,
        DeviceStatus::Unsupported,
        "macOS containers rollup must carry a typed Unsupported DeviceState (cgroup-v2 is Linux-only)"
    );
    assert!(
        containers_rollup.containers.is_empty(),
        "an unsupported containers rollup must never carry fabricated rows"
    );

    let successes = raw_success_capabilities(&drains);
    assert!(
        successes
            .iter()
            .any(|capability| capability == "storage.smart"),
        "the SMART tracking state machine must publish its (empty) batch"
    );
    // telemetry.gpu is verified by `catalog_health_derives_from_published_outcomes`
    // (its unavailable observation drives the Unsupported catalog state); it
    // intentionally does not appear here because system telemetry lanes
    // publish projections, not per-capability event batches.
    for batch in smart_batches(&drains) {
        assert!(
            batch.observations.is_empty() && batch.issues.is_empty() && batch.ended.is_empty(),
            "the macOS adapter must not fabricate SMART jobs, issues, or history"
        );
    }
}

#[test]
fn control_surface_accepts_submissions_and_publishes_only_typed_outcomes() {
    let mut client =
        PlatformClient::new(MacOsPlatformRuntime::spawn().expect("complete macOS composition"));
    let target = frozen_process();
    let mut request_ids = LatestControlRequest::default();

    let submissions = [
        client.submit_process_affinity_control(
            ProcessAffinityControlRequest {
                target: target.clone(),
                cpus: vec![0],
            },
            1,
        ),
        client.submit_process_control(ProcessControlRequest::EndTask(target.clone()), 1),
        client.submit_process_resource_control(
            ProcessResourceControlRequest {
                target: target.clone(),
                limits: ResourceGroupLimitRequest {
                    memory: None,
                    cpu: None,
                    processes: None,
                },
            },
            1,
        ),
        client.submit_service_control(
            ServiceControlRequest {
                request_id: request_ids.begin(),
                service_id: ServiceId::new("contract.fixture.service"),
                action: ServiceAction::Restart,
            },
            1,
        ),
        client.submit_session_control(
            SessionControlRequest {
                request_id: request_ids.begin(),
                session_id: SessionId::new("contract.fixture.session"),
                action: SessionControlAction::Lock,
            },
            1,
        ),
        client.submit_command_launch(
            CommandLaunchRequest {
                command: "true".into(),
            },
            1,
        ),
        client.submit_resource_reveal(
            ResourceRevealRequest {
                target: target.clone(),
                cached_executable: None,
            },
            1,
        ),
        // Registered-pending optional facets (G-05): the request ports exist,
        // submissions are accepted, and the lanes complete with the honest
        // typed Unsupported outcome (asserted below) — no notification
        // center touch, no setup asset access.
        client.submit_desktop_notification(
            DesktopNotificationRequest {
                instance_id: "contract-fixture".into(),
                title: "fixture".into(),
                body: "fixture".into(),
                severity: AlertSeverity::Warning,
                target: "fixture".into(),
            },
            1,
        ),
        client.submit_setup_script(
            SetupScriptRequest {
                action: SetupScriptAction::Observe,
            },
            1,
        ),
        // NOTE: `shell.url.open` is deliberately NOT submitted here — the
        // provider opens the real platform browser and a headless/CI run must
        // not trigger external side effects. The lane's acceptance is covered
        // by `complete_standard_surface_composes_with_descriptors_and_facets`
        // (registry + facet presence) and on macOS by manual verification.
    ];
    assert!(
        submissions.into_iter().all(|result| result.is_ok()),
        "every control lane must accept submissions on the second OS"
    );

    // Pending control lanes publish typed unsupported failures; implemented
    // lanes either complete or degrade to an honest non-unsupported failure.
    // Service and session control publish correlated outcome events whose
    // embedded result carries the same failure.
    let drains = drain_until(&mut client, 9);
    for failure in &drains.failures {
        if PENDING_CAPABILITIES.contains(&failure.capability.as_str()) {
            assert_honest_failure(failure);
        } else {
            assert_honest_real_failure(failure);
        }
    }
    let pending_failures: Vec<&str> = drains
        .failures
        .iter()
        .map(|failure| failure.capability.as_str())
        .filter(|capability| PENDING_CAPABILITIES.contains(capability))
        .collect();
    assert_eq!(
        pending_failures.len(),
        6,
        "six control lanes must publish typed unsupported failures; process control and resource reveal fail closed without an authoritative process token; sessions.control embeds its unsupported outcome in the correlated event; received: {pending_failures:?}"
    );
    assert!(
        pending_failures.contains(&"alerts.notify")
            && pending_failures.contains(&"first-run.setup"),
        "the registered-pending control facets must publish their typed Unsupported failures: {pending_failures:?}"
    );

    let successes = raw_success_capabilities(&drains);
    assert!(
        successes
            .iter()
            .any(|capability| capability == "services.control"),
        "service control must deliver its typed outcome payload"
    );
    assert!(
        successes
            .iter()
            .any(|capability| capability == "sessions.control"),
        "session control must deliver its typed outcome payload"
    );
    let service_outcomes: Vec<&ServiceControlOutcome> = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.service_events)
        .filter_map(|event| match &event.event {
            ServiceEvent::Update(ServiceUpdate::Action(outcome)) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(service_outcomes.len(), 1);
    assert!(
        service_outcomes[0].result.is_err(),
        "service control outcome must embed a typed failure on non-macOS hosts"
    );
    assert_eq!(
        service_outcomes[0].service_id,
        ServiceId::new("contract.fixture.service")
    );
    let session_outcomes: Vec<&SessionControlOutcome> = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.session_events)
        .filter_map(|event| match &event.event {
            SessionEvent::Control(outcome) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(session_outcomes.len(), 1);
    assert_eq!(
        session_outcomes[0].result,
        Err(FailureKind::Unsupported),
        "session control outcome must embed the typed failure"
    );
    assert_eq!(
        session_outcomes[0].session_id,
        SessionId::new("contract.fixture.session")
    );
}

#[test]
fn catalog_health_derives_from_published_outcomes() {
    let mut client =
        PlatformClient::new(MacOsPlatformRuntime::spawn().expect("complete macOS composition"));
    let submission = client
        .submit_system_telemetry(1)
        .expect("system telemetry revision");
    assert!(
        submission
            .into_request_results()
            .into_iter()
            .all(|result| result.is_ok()),
        "all six telemetry lanes accept submissions"
    );

    let drains = drain_until(&mut client, 6);
    let snapshot = client.capabilities().snapshot();
    for (capability, _) in STANDARD_SURFACE.iter().take(6) {
        let capability = *capability;
        let descriptor = snapshot
            .get(&CapabilityId::borrowed(capability))
            .unwrap_or_else(|| panic!("missing descriptor {capability}"));
        if PENDING_CAPABILITIES.contains(&capability) {
            assert_eq!(
                descriptor.status,
                CapabilityStatus::Unsupported,
                "{capability} health must derive from the published unsupported outcome"
            );
            assert!(
                descriptor.last_success_at_ms.is_none(),
                "{capability} must never record a fabricated success"
            );
        } else {
            assert_ne!(
                descriptor.status,
                CapabilityStatus::Unsupported,
                "{capability} is implemented; its health must not read unsupported"
            );
            let degraded = drains
                .failures
                .iter()
                .any(|failure| failure.capability.as_str() == capability);
            if !degraded && descriptor.last_success_at_ms.is_none() {
                assert!(
                    descriptor.observed_at_ms > 0,
                    "{capability} must record that its typed-unavailable observation was evaluated"
                );
                assert!(
                    matches!(
                        descriptor.status,
                        CapabilityStatus::PermissionRequired
                            | CapabilityStatus::MissingDependency
                            | CapabilityStatus::TemporarilyUnavailable
                            | CapabilityStatus::Stale
                    ),
                    "{capability} without a successful observation must retain a specific unavailable status, got {:?}",
                    descriptor.status
                );
            }
        }
    }
}

#[test]
fn event_port_stays_idle_and_live_before_any_submission() {
    let handle = MacOsPlatformRuntime::spawn().expect("complete macOS composition");
    assert!(
        matches!(handle.events().try_recv(), Ok(None)),
        "an idle second-OS adapter must not emit fabricated events"
    );
}

/// The macOS layer-B declaration is the live catalog's registration face, and
/// the lanes it declares with an absence are exactly this contract's
/// registered-pending census: the declaration is co-located with the real
/// provider registration and cross-checked against both independent facts.
#[test]
fn capability_surface_matches_the_live_catalog_and_the_pending_census() {
    let handle = MacOsPlatformRuntime::spawn().expect("complete macOS composition");
    let snapshot = handle.capabilities().snapshot();
    let surface = capability_surface();

    assert_eq!(
        assert_capability_surface_matches_catalog(
            &snapshot,
            &surface,
            PlatformAxis::Macos,
            "macos.",
        ),
        Ok(())
    );
    assert_eq!(snapshot.registered().count(), STANDARD_SURFACE.len());
    assert_eq!(
        surface.present_count(PlatformAxis::Macos),
        STANDARD_SURFACE.len() - PENDING_CAPABILITIES.len(),
        "the declared-present set is the registered face minus the pending census"
    );

    let mut declared_pending: BTreeSet<&str> = BTreeSet::new();
    for descriptor in snapshot.registered() {
        let declared = surface.source(PlatformAxis::Macos, &descriptor.id);
        assert_ne!(
            declared,
            PlatformSource::Undeclared,
            "{} is registered but silently undeclared",
            descriptor.id
        );
        if declared != PlatformSource::Present {
            declared_pending.insert(descriptor.id.as_str());
        }
    }
    let expected_pending: BTreeSet<&str> = PENDING_CAPABILITIES.iter().copied().collect();
    assert_eq!(
        declared_pending, expected_pending,
        "the layer-B absence declarations and the registered-pending census must agree"
    );
}
