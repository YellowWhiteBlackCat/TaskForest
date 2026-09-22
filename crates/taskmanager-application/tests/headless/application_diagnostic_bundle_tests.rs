//! Unit tests for the neutral diagnostic bundle export engine and application port.

use std::path::PathBuf;

use taskmanager_core::DiagnosticBundleErrorKind;
use taskmanager_core::config::Config;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::{
    ProcessItem, ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner,
    ProcessOwnerIdentity, ProcessScalarObservations,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::{ProviderId, ScalarObservation};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
};

use super::*;
use crate::test_support::repo_temp_dir;

struct TestCapabilities {
    descriptors: Vec<CapabilityDescriptor>,
}

impl CapabilityCatalog for TestCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::from_descriptors(self.descriptors.clone())
    }
}

fn sample_process(pid: u32, name: &str, user: &str, cmdline: &str) -> ProcessItem {
    let mut item = ProcessItem::new(pid, name);
    item.cmdline = cmdline.to_owned();
    item.status = "running".to_owned();
    item.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Opaque(user.into()),
                label: None,
            },
            100,
        ),
        executable_path: ProcessMetadataObservation::available(PathBuf::from(cmdline), 100),
    });
    item.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(18.5, 100),
        memory_bytes: ScalarObservation::available(128 * 1024 * 1024, 100),
        ..ProcessScalarObservations::default()
    });
    item
}

fn sample_hardware() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("Linux".to_owned()),
        os_version: Some("6.8.0-generic".to_owned()),
        kernel_version: Some("6.8.0".to_owned()),
        hostname: Some("secret-workstation".to_owned()),
        architecture: Some("x86_64".to_owned()),
        cpu_brand: Some("Test Processor 8-Core".to_owned()),
        cpu_cores: Some(8),
        sockets: Some(1),
        base_freq_mhz: Some(3600),
        total_memory_mb: Some(16384),
        ..HardwareInfo::default()
    }
}

fn sample_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms: 1_700_000_000_000,
        uptime_secs: 3600,
        telemetry_sources: vec![SourceStatus {
            provider: ProviderId::borrowed("procfs"),
            outcome: SourceOutcome::Available,
            item_count: 1,
        }],
        ..SystemSnapshot::default()
    }
}

fn sample_capabilities() -> TestCapabilities {
    TestCapabilities {
        descriptors: vec![
            CapabilityDescriptor {
                id: CapabilityId::TELEMETRY_CPU,
                status: CapabilityStatus::Available,
                providers: vec![ProviderId::borrowed("procfs")],
                observed_at_ms: 1_700_000_000_000,
                last_success_at_ms: Some(1_700_000_000_000),
            },
            CapabilityDescriptor {
                id: CapabilityId::TELEMETRY_GPU_ENGINES,
                status: CapabilityStatus::Unsupported,
                providers: Vec::new(),
                observed_at_ms: 1_700_000_000_000,
                last_success_at_ms: None,
            },
        ],
    }
}

#[test]
fn diagnostic_bundle_builds_complete_structured_facts() {
    let hw = sample_hardware();
    let snap = sample_snapshot();
    let caps = sample_capabilities();
    let config = Config::default();
    let proc1 = sample_process(
        1001,
        "taskmanager",
        "developer_alice",
        "/usr/bin/taskmanager",
    );
    let proc2 = sample_process(1002, "worker", "service_bob", "/usr/bin/worker");
    let processes = [proc1, proc2];

    let bundle = DiagnosticBundle::build(
        Some(&hw),
        Some(&snap),
        None,
        Some(&caps),
        Some(&config),
        &processes,
    );

    assert_eq!(bundle.version, 1);
    assert_eq!(bundle.timestamp_ms, 1_700_000_000_000);

    // 1. System overview facts
    assert_eq!(bundle.system_overview.os_name.as_deref(), Some("Linux"));
    assert_eq!(
        bundle.system_overview.cpu_brand.as_deref(),
        Some("Test Processor 8-Core")
    );
    assert_eq!(bundle.system_overview.cpu_cores, Some(8));
    assert_eq!(bundle.system_overview.uptime_secs, 3600);

    // 2. Platform capabilities facts
    assert_eq!(bundle.platform_capabilities.capabilities.len(), 2);
    let cpu_cap = bundle
        .platform_capabilities
        .capabilities
        .iter()
        .find(|c| c.id == "telemetry.cpu")
        .expect("telemetry.cpu capability present");
    assert_eq!(cpu_cap.status, "available");
    assert_eq!(cpu_cap.providers, vec!["procfs"]);

    // 3. Process summary facts
    assert_eq!(bundle.process_summary.total_processes, 2);
    assert_eq!(bundle.process_summary.distinct_users, 2);
    assert_eq!(bundle.process_summary.top_cpu.len(), 2);

    // 4. Telemetry health facts
    assert_eq!(bundle.telemetry_health.sources.len(), 1);
    assert_eq!(bundle.telemetry_health.sources[0].provider, "procfs");
    assert_eq!(bundle.telemetry_health.sources[0].outcome, "available");

    // 5. Configuration facts
    assert!(bundle.configuration.is_object());
}

#[test]
fn diagnostic_bundle_plan_redacts_usernames_paths_and_ips() {
    let mut hw = sample_hardware();
    hw.hostname = Some("alice-box".to_owned());
    let snap = sample_snapshot();
    let caps = sample_capabilities();

    let config = Config {
        skin: "/opt/secret_developer/custom_theme".to_owned(),
        mono_font: "192.168.10.25".to_owned(),
        ..Config::default()
    };

    let proc = sample_process(
        4242,
        "private_app",
        "secret_developer",
        "/opt/secret_developer/bin/app --connect 10.0.0.1",
    );
    let processes = [proc];

    let bundle = DiagnosticBundle::build(
        Some(&hw),
        Some(&snap),
        None,
        Some(&caps),
        Some(&config),
        &processes,
    );

    let plan = bundle
        .to_plan(["extra_secret_user".to_string()])
        .expect("prepare diagnostic plan");

    // Verify redactions
    let preview = plan.preview();
    assert!(
        preview.redactions.usernames > 0,
        "usernames must be redacted"
    );
    assert!(preview.redactions.paths > 0, "paths must be redacted");
    assert!(
        preview.redactions.ipv4_addresses > 0,
        "IPv4 addresses must be redacted"
    );

    let encoded = String::from_utf8(plan.encoded().expect("encode plan")).expect("utf8");

    // Verify sensitive material is eliminated
    assert!(
        !encoded.contains("secret_developer"),
        "secret_developer username must not appear in exported plan"
    );
    assert!(
        !encoded.contains("/opt/secret_developer"),
        "path must not appear in exported plan"
    );
    assert!(
        !encoded.contains("192.168.10.25"),
        "ip address must not appear in exported plan"
    );
    assert!(
        !encoded.contains("10.0.0.1"),
        "ip address must not appear in exported plan"
    );

    // Verify redaction placeholders are present
    assert!(encoded.contains("<redacted-user>"));
    assert!(encoded.contains("<redacted-path>"));
    assert!(encoded.contains("<redacted-ipv4>"));
}

#[test]
fn diagnostic_bundle_exports_transactionally_to_file() {
    let hw = sample_hardware();
    let snap = sample_snapshot();
    let caps = sample_capabilities();
    let config = Config::default();
    let proc = sample_process(1, "systemd", "root", "/sbin/init");
    let processes = [proc];

    let bundle = DiagnosticBundle::build(
        Some(&hw),
        Some(&snap),
        None,
        Some(&caps),
        Some(&config),
        &processes,
    );

    let scratch = repo_temp_dir();
    let destination = scratch.join("nested").join("exported_bundle.json");

    let result = export_diagnostic_bundle(&bundle, &destination).expect("export to file");
    assert_eq!(result, destination);
    assert!(destination.exists(), "exported file must exist on disk");

    let contents = std::fs::read_to_string(&destination).expect("read exported bundle");
    assert!(contents.contains("\"version\": 1"));
    assert!(contents.contains("system-overview.json"));
    assert!(contents.contains("platform-capabilities.json"));
    assert!(contents.contains("configuration.json"));
    assert!(contents.contains("process-summary.json"));
    assert!(contents.contains("telemetry-health.json"));
    assert!(contents.contains("manifest.json"));
}

#[test]
fn diagnostic_bundle_rejects_empty_target_path() {
    let bundle = DiagnosticBundle::build(None, None, None, None, None, &[]);
    let empty_path = PathBuf::from("");
    let err = bundle
        .export_diagnostic_bundle(&empty_path)
        .expect_err("empty path must be rejected");
    assert_eq!(err.kind(), DiagnosticBundleErrorKind::InvalidTarget);
}

#[test]
fn generate_diagnostic_report_produces_readable_markdown() {
    let hw = sample_hardware();
    let snap = sample_snapshot();
    let caps = sample_capabilities();
    let config = Config::default();
    let proc = sample_process(1234, "taskforest", "tester", "/usr/bin/taskforest");
    let processes = [proc];

    let bundle = DiagnosticBundle::build(
        Some(&hw),
        Some(&snap),
        None,
        Some(&caps),
        Some(&config),
        &processes,
    );

    let report = generate_diagnostic_report(&bundle);
    assert!(report.contains("# TaskForest Diagnostic Report"));
    assert!(report.contains("## System Overview"));
    assert!(report.contains("Linux"));
    assert!(report.contains("## Platform Capabilities"));
    assert!(report.contains("telemetry.cpu"));
    assert!(report.contains("## Process Summary"));
    assert!(report.contains("taskforest"));
    assert!(report.contains("## Telemetry Health"));

    // Redacted report test
    let redacted_report = bundle
        .generate_redacted_report(["tester".to_string()])
        .expect("redacted report");
    assert!(!redacted_report.contains("user: tester"));
    assert!(redacted_report.contains("<redacted-user>"));
}

#[test]
fn diagnostic_bundle_engine_coordinates_build_and_export() {
    let engine = DiagnosticBundleEngine::new();
    let bundle = engine.build(None, None, None, None, None, &[]);
    assert_eq!(bundle.version, 1);
    assert_eq!(bundle.process_summary.total_processes, 0);

    let report = engine.generate_report(&bundle);
    assert!(report.contains("# TaskForest Diagnostic Report"));

    let scratch = repo_temp_dir();
    let target = scratch.join("engine_bundle.json");
    let path = engine
        .export_bundle(&bundle, &target)
        .expect("export via engine");
    assert!(path.exists());
}
