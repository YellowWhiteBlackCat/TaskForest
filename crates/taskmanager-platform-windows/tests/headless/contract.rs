//! Second operating-system adapter contract proof.
//!
//! The Windows adapter composes the complete standard product surface from the
//! shared runtime (ADR-018): safe-crate implementations cover many domains
//! (CPU/memory/process via sysinfo, NVIDIA GPU via nvml-wrapper, battery,
//! startup via windows-registry, directory usage via the shared safe
//! scanner, ...), and the audited native boundary covers the rest (per-process
//! network/isolation/threads, affinity control, PDH GPU engine rows and
//! per-engine detail, winevt service-log snapshot+stream, boot evidence,
//! session lock, power overlay, toast). Capabilities with no safe Windows
//! source (per-process network byte accounting, resource-control escalation,
//! first-run setup, plus the registered-pending open-files facet, G-05)
//! publish typed `Unsupported` outcomes attributed to a `windows.*` identity.
//! No fabricated observation may reach the event port.
//!
//! These tests run on the Windows CI runner (native target) AND on the Linux
//! workspace suite — the adapter compiles on every target (the Windows-only
//! safe wrappers are `[target.'cfg(windows)'.dependencies]` with honest
//! `MissingDependency` fallbacks elsewhere, and sysinfo/battery/nvml-wrapper
//! are cross-platform), which makes the contract proof repeatable on every
//! gate. On Linux the sysinfo-backed providers return real data, the
//! NVML/registry/shell-out providers degrade to `MissingDependency`, and the
//! lanes implemented through the audited Windows API boundary degrade to that
//! boundary's typed `Unsupported` fallback — exactly the honesty the contract
//! asserts.

use std::time::{Duration, Instant};

use taskmanager_application::{
    CapabilityId, CapabilityStatus, ContainerRollupEvent, DesktopNotificationRequest,
    DirectoryUsageRequest, FailureKind, FrozenProcessIdentity, LatestControlRequest,
    OperationFailure, PlatformClient, PlatformEventBatch, PlatformFacets,
    ProcessAffinityControlRequest, ProcessControlRequest, ProcessResourceControlRequest,
    RefreshRequest, ResourceGroupLimitRequest, ResourceRevealRequest, RetryDisposition,
    ServiceAction, ServiceControlOutcome, ServiceControlRequest, ServiceEvent, ServiceId,
    ServiceUpdate, SessionControlAction, SessionControlOutcome, SessionControlRequest,
    SessionEvent, SessionId, SetupScriptAction, SetupScriptRequest, SmartObservationBatch,
    alerts::AlertSeverity,
};
use taskmanager_core::{DeviceStatus, DirectoryScanSpec};
use taskmanager_platform_windows::WindowsPlatformRuntime;

const DRAIN_DEADLINE: Duration = Duration::from_secs(5);
const DRAIN_POLL: Duration = Duration::from_millis(5);

/// Every standard capability with the provider identity the Windows adapter
/// must attribute to it. The 45 entries are the complete product surface:
/// 38 always-required lanes plus the optional facets (open-files /
/// environment / desktop-notify / first-run-setup / directory-usage) and
/// the two auxiliary observation facets (gpu-engines / accelerator-npu).
/// Directory usage is wired to the shared safe scanner (2026-08-18);
/// engine rows are served from unprivileged PDH, open-files from the
/// handle-table boundary lane, NPU inventory from SetupAPI, and the
/// environment facet from the PEB-reading boundary lane — all 2026-08-24
/// (environment 08-25).
const STANDARD_SURFACE: &[(&str, &str)] = &[
    ("telemetry.host", "windows.system.host"),
    ("telemetry.cpu", "windows.system.cpu"),
    ("telemetry.memory", "windows.system.memory"),
    ("telemetry.storage", "windows.system.storage"),
    ("telemetry.network", "windows.system.network"),
    ("telemetry.gpu", "windows.system.gpu"),
    ("hardware.inventory", "windows.hardware.inventory"),
    ("containers.rollup", "windows.containers.wsl"),
    ("process.list", "windows.process.list"),
    ("process.control", "windows.process.control"),
    (
        "process.insights.network",
        "windows.process.insights.network",
    ),
    ("process.insights.gpu", "windows.process.insights.gpu"),
    (
        "process.insights.resources",
        "windows.process.insights.resources",
    ),
    (
        "process.insights.isolation",
        "windows.process.insights.isolation",
    ),
    (
        "process.insights.threads",
        "windows.process.insights.threads",
    ),
    (
        "process.insights.open_files",
        "windows.process.insights.open_files",
    ),
    (
        "process.insights.environment",
        "windows.process.insights.environment",
    ),
    ("process.affinity", "windows.process.affinity"),
    (
        "process.affinity.control",
        "windows.process.affinity.control",
    ),
    (
        "process.resource.control",
        "windows.process.resource.control",
    ),
    (
        "process.network.escalation",
        "windows.process.network.escalation",
    ),
    ("services", "windows.service.inventory"),
    ("services.dependencies", "windows.service.dependencies"),
    ("services.control", "windows.service.control"),
    ("services.logs", "windows.service.logs.snapshot"),
    ("services.logs.stream", "windows.service.logs.stream"),
    ("startup", "windows.startup.inventory"),
    ("startup.evidence", "windows.startup.evidence"),
    ("startup.control", "windows.startup.control"),
    ("sessions", "windows.session.inventory"),
    ("sessions.control", "windows.session.control"),
    ("shell.command.launch", "windows.shell.command"),
    ("shell.resource.reveal", "windows.shell.resource-reveal"),
    ("shell.url.open", "windows.shell.url-open"),
    ("desktop.appearance", "windows.desktop.appearance"),
    ("alerts.notify", "windows.alerts.desktop-notification"),
    ("first-run.setup", "windows.first-run.setup-script"),
    ("storage.health", "windows.storage.filesystem.registry"),
    ("storage.smart", "windows.storage.smart.observation"),
    ("storage.smart.control", "windows.storage.smart.control"),
    (
        "filesystem.directory.usage",
        "windows.storage.directory-usage",
    ),
    ("telemetry.gpu.engines", "windows.system.gpu-engines"),
    ("accelerator.npu", "windows.accelerator.npu"),
    ("sensors", "windows.sensor.registry"),
    ("hardware.power-supplies", "windows.power-supply.registry"),
];

/// Capabilities with NO safe Windows source: they must complete with a typed
/// unsupported outcome everywhere (ADR-018). `process.network.escalation`
/// has no Windows fd chain (permanent), and `first-run.setup` awaits
/// packaged assets plus an elevated runner. These registered-pending
/// facets' failures ride the failure lane (G-05); service-log streaming
/// left this set when the winevt boundary lane landed and resource control
/// when the job-object lane landed (both 2026-08-24).
const PENDING_CAPABILITIES: &[&str] = &["process.network.escalation", "first-run.setup"];

/// Lanes implemented natively through the audited Windows API boundary
/// (IP Helper connection tables, token/SID isolation, ToolHelp32
/// threads/modules, SetProcessAffinityMask, WinRT toast). Per the
/// cross-target model the boundary returns its typed
/// `WindowsApiError::Unsupported` on every other host, so there these lanes
/// complete with the same typed `Unsupported` outcome as the pending set.
/// On the Windows host they stay implemented: submissions must complete
/// with real data or an honest non-Unsupported failure
/// (`assert_honest_real_failure`), so the Windows guarantee is untouched.
/// Split by surface so each contract test can pin its exact expected set.
#[cfg(not(windows))]
const WINDOWS_API_ONLY_INSIGHT_LANES: &[&str] = &[
    "process.insights.network",
    "process.insights.gpu",
    "process.insights.resources",
    "process.insights.isolation",
    "process.insights.threads",
    "process.insights.open_files",
    "process.insights.environment",
];

#[cfg(not(windows))]
const WINDOWS_API_ONLY_CONTROL_LANES: &[&str] = &[
    "process.affinity.control",
    "process.resource.control",
    "shell.resource.reveal",
    "alerts.notify",
];

/// Whether a capability is expected to complete with the typed Unsupported
/// outcome on this host: the always-pending set everywhere, plus the
/// Windows-API-only lanes whose non-Windows fallback is that typed failure.
fn expects_typed_unsupported(capability: &str) -> bool {
    if PENDING_CAPABILITIES.contains(&capability) {
        return true;
    }
    #[cfg(not(windows))]
    {
        WINDOWS_API_ONLY_INSIGHT_LANES.contains(&capability)
            || WINDOWS_API_ONLY_CONTROL_LANES.contains(&capability)
    }
    #[cfg(windows)]
    {
        false
    }
}

/// Safe-crate capabilities: submissions must be accepted and complete either
/// with real data or an honest non-unsupported failure (MissingDependency /
/// TemporarilyUnavailable when the host lacks the tool or driver).
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
        provider.as_str().starts_with("windows."),
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
        + batch.directory_usage_events.len()
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
            taskmanager_application::SmartEvent::Batch(batch) => batch,
        })
        .collect()
}

fn assert_honest_failure(failure: &OperationFailure) {
    assert_eq!(
        failure.kind,
        FailureKind::Unsupported,
        "Windows adapter must publish typed unsupported outcomes, not data or other failures"
    );
    assert_eq!(
        failure.retry,
        RetryDisposition::Never,
        "an unsupported Windows capability must not be retried"
    );
    let provider = failure
        .provider
        .as_ref()
        .unwrap_or_else(|| panic!("{} failure must be attributed", failure.capability));
    assert!(
        provider.as_str().starts_with("windows."),
        "{} attributed to {provider}",
        failure.capability
    );
}

#[test]
fn complete_standard_surface_composes_with_descriptors_and_facets() {
    let handle = WindowsPlatformRuntime::spawn().expect("complete Windows composition");

    let snapshot = handle.capabilities().snapshot();
    assert_eq!(
        snapshot.iter().count(),
        STANDARD_SURFACE.len(),
        "the second OS adapter must expose exactly the standard product surface"
    );
    for (capability, provider) in STANDARD_SURFACE {
        let descriptor = snapshot
            .get(&CapabilityId::borrowed(capability))
            .unwrap_or_else(|| panic!("missing capability descriptor {capability}"));
        assert_eq!(
            descriptor.status,
            CapabilityStatus::TemporarilyUnavailable,
            "a fresh descriptor must not claim availability before any observation"
        );
        assert_eq!(
            descriptor.providers,
            [taskmanager_application::ProviderId::borrowed(provider)],
            "{capability} must be owned by its windows.* provider"
        );
        assert!(descriptor.last_success_at_ms.is_none());
    }

    let facets = handle.facets();
    assert_complete_facet_surface(facets);
}

/// The full facet census of the composed runtime: every request port the
/// standard surface wires must be present. Split out of the composition
/// test so the flat assert list stays under the complexity ratchet.
fn assert_complete_facet_surface(facets: &PlatformFacets) {
    assert!(facets.system().host().is_some());
    assert!(facets.system().cpu().is_some());
    assert!(facets.system().memory().is_some());
    assert!(facets.system().storage().is_some());
    assert!(facets.system().network().is_some());
    assert!(facets.system().gpu().is_some());
    assert!(facets.system().hardware_inventory().is_some());
    assert!(
        facets.system().gpu_engine_rows().is_some(),
        "the engine-rows facet must expose its request port"
    );
    assert!(
        facets.system().containers().is_some(),
        "containers port must be present"
    );
    assert!(facets.process().list().is_some());
    assert!(facets.process().control().is_some());
    assert!(facets.process().network().is_some());
    assert!(facets.process().gpu().is_some());
    assert!(facets.process().resources().is_some());
    assert!(facets.process().isolation().is_some());
    assert!(facets.process().threads().is_some());
    assert!(facets.process().affinity().is_some());
    assert!(facets.process().affinity_control().is_some());
    assert!(facets.process().resource_control().is_some());
    assert!(
        facets.process().open_files().is_some(),
        "the open-files facet must expose its request port"
    );
    assert!(
        facets.process().environment().is_some(),
        "the environment facet must expose its request port"
    );
    assert!(facets.service().inventory().is_some());
    assert!(facets.service().dependencies().is_some());
    assert!(facets.service().control().is_some());
    assert!(facets.service().log_snapshot().is_some());
    assert!(facets.service().log_stream().is_some());
    assert!(facets.environment().startup_inventory().is_some());
    assert!(facets.environment().startup_evidence().is_some());
    assert!(facets.environment().startup_control().is_some());
    assert!(facets.environment().session_inventory().is_some());
    assert!(facets.environment().session_control().is_some());
    assert!(facets.integration().command_launch().is_some());
    assert!(facets.integration().resource_reveal().is_some());
    assert!(facets.integration().url_open().is_some());
    assert!(facets.integration().desktop_appearance().is_some());
    assert!(
        facets.integration().desktop_notification().is_some(),
        "the notification facet must expose its request port"
    );
    assert!(
        facets.integration().setup_script().is_some(),
        "the registered-pending setup facet must expose its request port"
    );
    assert!(facets.storage().health().is_some());
    assert!(facets.storage().smart_observation().is_some());
    assert!(facets.storage().smart_control().is_some());
    assert!(
        facets.storage().directory_usage().is_some(),
        "the wired directory-usage facet must expose its request port"
    );
    assert!(facets.sensor().observation().is_some());
    assert!(facets.power().supplies().is_some());
}

#[test]
fn observation_surface_accepts_submissions_and_publishes_only_typed_outcomes() {
    let mut client =
        PlatformClient::new(WindowsPlatformRuntime::spawn().expect("complete Windows composition"));

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
    // + power supplies + SMART observation = 23 accepted observations. Pending
    // capabilities publish typed unsupported failures; implemented
    // capabilities either publish data or degrade to an honest
    // non-unsupported failure (e.g. MissingDependency without NVML on Linux
    // CI).
    let drains = drain_until(&mut client, 23);
    for failure in &drains.failures {
        if expects_typed_unsupported(failure.capability.as_str()) {
            assert_honest_failure(failure);
        } else {
            assert_honest_real_failure(failure);
        }
    }
    let pending_failures: Vec<&str> = drains
        .failures
        .iter()
        .map(|failure| failure.capability.as_str())
        .filter(|capability| expects_typed_unsupported(capability))
        .collect();
    #[cfg(windows)]
    assert_eq!(
        pending_failures.len(),
        0,
        "observation lanes for implemented process insights and telemetry complete with snapshots; \
         received pending failures: {pending_failures:?}"
    );
    #[cfg(not(windows))]
    {
        // The Windows-API-only insight lanes degrade to their typed
        // Unsupported fallback exactly — every other implemented lane still
        // completes with a snapshot or an honest non-Unsupported failure.
        let mut observed = pending_failures.clone();
        observed.sort_unstable();
        let mut expected = WINDOWS_API_ONLY_INSIGHT_LANES.to_vec();
        expected.sort_unstable();
        assert_eq!(
            observed, expected,
            "on a non-Windows host exactly the Windows-API-only insight lanes complete typed Unsupported; \
             received: {pending_failures:?}"
        );
    }

    // containers.rollup rides the SNAPSHOT lane (not the failure lane): Windows
    // has no cgroup-v2, so the provider returns a typed-unavailable rollup
    // whose DeviceState is Unsupported. The snapshot must reach the batch so
    // the page shows the honest "containers.unsupported" reason — never the
    // doubly-dishonest empty-healthy "no containers detected" default.
    let containers_rollup = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.containers_events)
        .map(|event| match &event.event {
            ContainerRollupEvent::Snapshot(rollup) => rollup.as_ref(),
        })
        .next()
        .expect("containers.rollup must publish a snapshot");
    assert!(
        matches!(
            containers_rollup.state.status,
            DeviceStatus::Healthy | DeviceStatus::Unsupported
        ),
        "Windows containers rollup must carry a typed DeviceState"
    );

    for batch in smart_batches(&drains) {
        assert!(
            batch.observations.is_empty() && batch.issues.is_empty() && batch.ended.is_empty(),
            "the Windows adapter must not fabricate SMART jobs, issues, or history"
        );
    }
}

#[test]
fn control_surface_accepts_submissions_and_publishes_only_typed_outcomes() {
    let mut client =
        PlatformClient::new(WindowsPlatformRuntime::spawn().expect("complete Windows composition"));
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
        client.submit_resource_reveal(
            ResourceRevealRequest {
                target: target.clone(),
                cached_executable: None,
            },
            1,
        ),
        // Registered-pending optional facets (G-05): the request ports exist,
        // submissions are accepted, and the lanes complete with the honest
        // typed Unsupported outcome (asserted below) — no toast center touch,
        // no setup asset access.
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
        // The PDH engine-rows facet: accepted, then completed by its lane as
        // one typed failure snapshot for a fixture device no DXGI adapter
        // owns (asserted below).
        client.submit_gpu_engine_rows(
            taskmanager_application::GpuEngineRowsRequest {
                device_id: taskmanager_application::DeviceId::new("contract-fixture-gpu"),
            },
            1,
        ),
        // The NPU inventory facet: accepted, then completed by its lane as
        // one snapshot — the honest SetupAPI inventory on Windows, the typed
        // dormant-boundary failure elsewhere (asserted below).
        client.submit_npu_inventory(taskmanager_application::NpuInventoryRequest {}, 1),
        // NOTE: `shell.command.launch` and `shell.url.open` are deliberately
        // NOT submitted here — the real providers spawn a child process /
        // open the platform browser, and a headless/CI run must not trigger
        // external side effects. Their lanes are asserted wired above and
        // their mapping logic is covered by in-crate unit tests.
    ];
    assert!(
        submissions.into_iter().all(|result| result.is_ok()),
        "every control lane must accept submissions on the second OS"
    );

    // The directory-usage facet is wired to the shared pure-safe scanner: a
    // real fixture tree must complete with real aggregates (asserted below),
    // never a typed Unsupported snapshot.
    let fixture_root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-contract-dir-usage-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(fixture_root.join("logs")).expect("fixture parent");
    std::fs::write(fixture_root.join("a.txt"), vec![0_u8; 100]).expect("fixture file");
    std::fs::write(fixture_root.join("logs/b.log"), vec![0_u8; 50]).expect("fixture file");
    client
        .submit_directory_usage(
            DirectoryUsageRequest::StartScan(DirectoryScanSpec {
                root: fixture_root.to_string_lossy().into_owned(),
                bounds: Default::default(),
            }),
            1,
        )
        .expect("directory-usage lane accepts the scan");

    // Pending control lanes publish typed unsupported failures; implemented
    // lanes either complete or degrade to an honest non-unsupported failure.
    // Service and session control publish correlated outcome events whose
    // embedded result carries the same failure. The registered-pending
    // alerts.notify / first-run.setup facets complete with typed Unsupported;
    // the wired directory-usage facet rides its own scan lane, publishing one
    // progress snapshot then one terminal snapshot (asserted below), so the
    // failure lane sees 2 + 2 pending failures, the scan lane 2 snapshots and
    // the engine-rows lane 1 snapshot and the NPU lane 1 snapshot =
    // 11 drained envelopes (command-launch is not submitted).
    let drains = drain_until(&mut client, 11);
    for failure in &drains.failures {
        if expects_typed_unsupported(failure.capability.as_str()) {
            assert_honest_failure(failure);
        } else {
            assert_honest_real_failure(failure);
        }
    }
    let pending_failures: Vec<&str> = drains
        .failures
        .iter()
        .map(|failure| failure.capability.as_str())
        .filter(|capability| expects_typed_unsupported(capability))
        .collect();
    #[cfg(windows)]
    {
        assert_eq!(
            pending_failures.len(),
            1,
            "control lanes must publish typed unsupported failures for pending capabilities;          \
             sessions.control embeds its outcome in the correlated event;          \
             the registered-pending first-run.setup facet completes with typed          \
             Unsupported; resource control now rejects its fixture target with an          \
             honest identity failure; received: {pending_failures:?}"
        );
        assert!(
            pending_failures.contains(&"first-run.setup"),
            "the registered-pending control facets must publish their typed Unsupported failures: {pending_failures:?}"
        );
    }
    #[cfg(not(windows))]
    {
        // The always-pending control lane keeps its typed Unsupported
        // outcome, joined on a non-Windows host by the Windows-API-only
        // affinity/resource control and toast lanes degrading to the same
        // typed fallback — and nothing else may claim Unsupported.
        let mut observed = pending_failures.clone();
        observed.sort_unstable();
        let mut expected: Vec<&str> = WINDOWS_API_ONLY_CONTROL_LANES.to_vec();
        expected.extend_from_slice(&["first-run.setup"]);
        expected.sort_unstable();
        assert_eq!(
            observed, expected,
            "on a non-Windows host exactly the pending plus Windows-API-only control lanes complete typed Unsupported; \
             received: {pending_failures:?}"
        );
    }

    // The PDH engine-rows facet: the lane receives the request, and because
    // the fixture device id matches no DXGI adapter identity the provider
    // answers with the typed `Unsupported` failure folded into a failure
    // snapshot — never a sibling adapter's rows and never a fabricated row.
    let engine_rows_snapshots: Vec<&taskmanager_application::GpuEngineRowsSnapshot> = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.gpu_engine_rows_events)
        .map(|event| match &event.event {
            taskmanager_application::GpuEngineRowsEvent::Update(snapshot) => snapshot,
        })
        .collect();
    assert_eq!(
        engine_rows_snapshots.len(),
        1,
        "the engine-rows request must publish exactly one snapshot"
    );
    assert_eq!(
        engine_rows_snapshots[0]
            .failure
            .as_ref()
            .map(|failure| failure.kind),
        Some(FailureKind::Unsupported),
        "an engine-rows read for an unknown device must complete with the typed Unsupported failure"
    );
    assert!(
        engine_rows_snapshots[0].engines.is_empty(),
        "an unknown-device engine-rows read must never carry a fabricated row"
    );

    // The NPU inventory facet: the lane receives the request and answers with
    // exactly one snapshot — real SetupAPI devices or the honest empty
    // inventory on the Windows host, the typed dormant-boundary failure
    // everywhere else. Never a fabricated device row.
    let npu_snapshots: Vec<&taskmanager_core::NpuInventorySnapshot> = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.npu_inventory_events)
        .map(|event| match &event.event {
            taskmanager_application::NpuInventoryEvent::Update(snapshot) => snapshot,
        })
        .collect();
    assert_eq!(
        npu_snapshots.len(),
        1,
        "the NPU inventory request must publish exactly one snapshot"
    );
    #[cfg(not(windows))]
    {
        assert_eq!(
            npu_snapshots[0]
                .failure
                .as_ref()
                .map(|failure| failure.kind),
            Some(FailureKind::Unsupported),
            "off-Windows the NPU inventory read must complete with the typed Unsupported failure"
        );
    }
    #[cfg(windows)]
    {
        // On the Windows host an absent NPU is the honest empty success, not
        // a failure pose; either way no device row may be fabricated.
        let failure_kind = npu_snapshots[0]
            .failure
            .as_ref()
            .map(|failure| failure.kind);
        assert!(
            failure_kind.is_none() || failure_kind != Some(FailureKind::Unsupported),
            "the NPU inventory lane is implemented; it must not degrade to Unsupported"
        );
    }
    assert!(
        npu_snapshots[0].devices.iter().all(|device| matches!(
            device.utilization_pct.availability(),
            taskmanager_core::ScalarAvailability::Unavailable(_)
        )),
        "a discovered NPU must never carry a fabricated utilization curve"
    );

    // The wired directory-usage facet: the scan lane receives the StartScan,
    // drives the shared pure-safe scanner to its terminal state, and publishes
    // real progress + terminal snapshots attributed to the windows provider —
    // real aggregates, never a fabricated entry, total, or Unsupported.
    let directory_snapshots: Vec<&taskmanager_core::DirectoryUsageSnapshot> = drains
        .batches
        .iter()
        .flat_map(|batch| &batch.directory_usage_events)
        .map(|event| match &event.event {
            taskmanager_application::DirectoryUsageEvent::Update(snapshot) => snapshot,
        })
        .collect();
    assert!(
        !directory_snapshots.is_empty(),
        "the wired directory-usage scan must publish snapshots"
    );
    let scan_id = directory_snapshots[0].scan_id;
    for snapshot in &directory_snapshots {
        assert_eq!(snapshot.scan_id, scan_id, "one scan keeps one identity");
        assert_eq!(
            snapshot.root,
            fixture_root.to_string_lossy().into_owned(),
            "every snapshot reports the requested root"
        );
    }
    let terminal = directory_snapshots
        .last()
        .expect("at least one terminal snapshot");
    assert_eq!(
        terminal.status,
        taskmanager_core::DirectoryScanStatus::Completed,
        "the wired directory-usage scan must complete with real data"
    );
    assert_eq!(terminal.totals.files_counted, 2);
    assert_eq!(terminal.totals.bytes_counted.current_value(), Some(&150));
    assert!(
        terminal.entries.iter().any(|entry| entry.path == "logs"),
        "the report must carry the real subtree aggregate"
    );
    let _ = std::fs::remove_dir_all(&fixture_root);

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
        "service control outcome must embed a typed failure"
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
    // Session control is implemented through the WTS native boundary; Lock has
    // no per-session API. On Linux CI it degrades to a typed non-Unsupported failure embedded
    // in the correlated outcome — host-independent: it must be an error and must
    // NOT be Unsupported (the capability is no longer pending).
    assert!(
        session_outcomes[0].result.is_err(),
        "session control outcome must embed a typed failure"
    );
    assert_ne!(
        session_outcomes[0].result,
        Err(FailureKind::Unsupported),
        "session control is implemented through WTS; it must not degrade to Unsupported"
    );
    assert_eq!(
        session_outcomes[0].session_id,
        SessionId::new("contract.fixture.session")
    );
}

#[test]
fn catalog_health_derives_from_published_outcomes() {
    let mut client =
        PlatformClient::new(WindowsPlatformRuntime::spawn().expect("complete Windows composition"));
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
        } else if capability == "telemetry.gpu" {
            // GPU telemetry is implemented through NVML with native DXGI fallback.
            // When physical/integrated GPU hardware is present, health reports
            // Available or Degraded; when absent, Unsupported.
            assert!(
                matches!(
                    descriptor.status,
                    CapabilityStatus::Available
                        | CapabilityStatus::Unsupported
                        | CapabilityStatus::Degraded(_)
                ),
                "GPU health must remain a valid honest status"
            );
        } else {
            assert_ne!(
                descriptor.status,
                CapabilityStatus::Unsupported,
                "{capability} is implemented; its health must not read unsupported"
            );
            // A capability need not record a success when it honestly did not
            // fully succeed this cycle: it published a failure, its catalog
            // status is Degraded (partial), or it is host-limited.
            let degraded = matches!(descriptor.status, CapabilityStatus::Degraded(_))
                || drains
                    .failures
                    .iter()
                    .any(|failure| failure.capability.as_str() == capability);
            if !degraded {
                assert!(
                    descriptor.last_success_at_ms.is_some(),
                    "{capability} must record its real success"
                );
            }
        }
    }
}

#[test]
fn event_port_stays_idle_and_live_before_any_submission() {
    let handle = WindowsPlatformRuntime::spawn().expect("complete Windows composition");
    assert!(
        matches!(handle.events().try_recv(), Ok(None)),
        "an idle second-OS adapter must not emit fabricated events"
    );
}
