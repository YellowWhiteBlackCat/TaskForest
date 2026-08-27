use super::*;

fn fixture_probe() -> RawCapabilityProbe {
    RawCapabilityProbe {
        supported_platform: true,
        hardware_build_profile: HardwareBuildProfile::StandardAll,
        init_evidence: InitRuntimeEvidence::OpenrcRuntime,
        ata_candidate_devices: 1,
        nvme_namespace_devices: 2,
        nvidia_device_markers: 1,
        smartctl_available: true,
        nvme_cli_available: false,
        systemctl_available: false,
        openrc_tools_available: true,
        nvidia_backend_compiled: true,
        kernel_btf_available: true,
        cgroup_v2_available: true,
        unprivileged_bpf_disabled: Some(2),
        effective_bpf_privilege: false,
        ebpf_compat_environment_available: true,
        ebpf_compat_probe_permission_required: false,
        ebpf_backend_compiled: false,
        amd_device_markers: 1,
        sas_candidate_devices: 1,
        usb_candidate_devices: 1,
        at_spi_session_detected: true,
        at_spi_backend_compiled: false,
        hotplug_inventory_available: true,
        intel_gpu_engine_pmu_devices: 1,
        effective_perfmon_privilege: false,
        perf_event_paranoid: Some(2),
    }
}

#[test]
fn receipt_clock_projects_the_supplied_wall_time() {
    let fixed = std::time::UNIX_EPOCH + std::time::Duration::from_millis(12_345);
    assert_eq!(unix_time_millis(fixed), 12_345);
}

#[test]
fn fixture_receipt_stays_explicit_and_only_claims_probe_eligibility() {
    let receipt = build_receipt(ReceiptSource::Fixture, 123, fixture_probe());

    assert_eq!(receipt.source, ReceiptSource::Fixture);
    assert!(receipt.capability_only);
    assert_eq!(
        receipt.hardware_build_profile,
        HardwareBuildProfile::StandardAll
    );
    assert_eq!(receipt.ata_smart_probe, ProviderProbeEligibility::Eligible);
    assert_eq!(
        receipt.nvidia_nvml_probe,
        ProviderProbeEligibility::Eligible
    );
    assert_eq!(receipt.openrc_probe, ProviderProbeEligibility::Eligible);
    assert_eq!(
        receipt.systemd_probe,
        ProviderProbeEligibility::BackendInactive
    );
    assert!(!receipt.nvme_cli_available);
    assert_eq!(
        receipt.target_environment.ebpf_process_rates,
        ProviderProbeEligibility::BackendNotCompiled
    );
    assert_eq!(
        receipt.target_environment.amd_gpu,
        ProviderProbeEligibility::Eligible
    );
    assert_eq!(
        receipt.target_environment.sas_smart,
        ProviderProbeEligibility::Eligible
    );
    assert_eq!(
        receipt.target_environment.usb_smart,
        ProviderProbeEligibility::Eligible
    );
    assert_eq!(
        receipt.target_environment.at_spi,
        ProviderProbeEligibility::BackendNotCompiled
    );
    assert_eq!(
        receipt.target_environment.hotplug,
        ProviderProbeEligibility::BackendUnconfirmed
    );
    assert_eq!(
        receipt.target_environment.intel_gpu_engine_pmu,
        ProviderProbeEligibility::PrivilegeRequired
    );
    assert_eq!(receipt.target_environment.intel_gpu_engine_pmu_devices, 1);
    assert_eq!(receipt.target_environment.perf_event_paranoid, Some(2));
}

#[test]
fn standard_profile_requires_every_current_optional_backend() {
    assert_eq!(
        classify_hardware_build_profile(true, true),
        HardwareBuildProfile::StandardAll
    );
    assert_eq!(
        classify_hardware_build_profile(true, false),
        HardwareBuildProfile::DeveloperReduced
    );
    assert_eq!(
        classify_hardware_build_profile(false, true),
        HardwareBuildProfile::DeveloperReduced
    );
}

#[test]
fn backend_absence_is_not_hidden_by_missing_target_hardware() {
    let mut probe = fixture_probe();
    probe.nvidia_device_markers = 0;
    probe.nvidia_backend_compiled = false;
    probe.hardware_build_profile = HardwareBuildProfile::DeveloperReduced;

    let receipt = build_receipt(ReceiptSource::Fixture, 124, probe);

    assert_eq!(
        receipt.nvidia_nvml_probe,
        ProviderProbeEligibility::BackendNotCompiled
    );
    assert_eq!(
        receipt.target_environment.nvidia_gpu,
        ProviderProbeEligibility::BackendNotCompiled
    );
}

#[test]
fn confirmed_systemd_receipt_keeps_known_backend_behavior() {
    let mut probe = fixture_probe();
    probe.init_evidence = InitRuntimeEvidence::SystemdPid1;
    probe.systemctl_available = true;
    let receipt = build_receipt(ReceiptSource::Fixture, 234, probe);

    assert_eq!(receipt.systemd_probe, ProviderProbeEligibility::Eligible);
    assert_eq!(
        receipt.openrc_probe,
        ProviderProbeEligibility::BackendInactive
    );
}

#[test]
fn previous_assumed_systemd_value_deserializes_as_unknown_pid_one() {
    assert_eq!(
        serde_json::from_str::<InitRuntimeEvidence>("\"assumed_systemd\"")
            .expect("the previous receipt spelling must remain readable"),
        InitRuntimeEvidence::UnknownPid1
    );
}

#[cfg(target_os = "linux")]
#[test]
fn init_evidence_preserves_known_backends_and_types_unknown_pid_one() {
    assert_eq!(
        classify_init_evidence(Some("systemd\n"), false),
        InitRuntimeEvidence::SystemdPid1
    );
    assert_eq!(
        classify_init_evidence(Some("systemd"), true),
        InitRuntimeEvidence::SystemdPid1
    );
    assert_eq!(
        classify_init_evidence(Some("openrc-init\n"), false),
        InitRuntimeEvidence::OpenrcRuntime
    );
    assert_eq!(
        classify_init_evidence(Some("runit\n"), false),
        InitRuntimeEvidence::UnknownPid1
    );
    assert_eq!(
        classify_init_evidence(None, false),
        InitRuntimeEvidence::UnknownPid1
    );
}

#[test]
fn deterministic_json_is_redaction_safe_and_preserves_fixture_source() {
    let receipt = build_receipt(ReceiptSource::Fixture, 789, fixture_probe());
    let first = linux_provider_capability_receipt_json(&receipt)
        .expect("fixed capability receipt must serialize");
    let second = linux_provider_capability_receipt_json(&receipt)
        .expect("repeated capability receipt must serialize");

    assert_eq!(first, second);
    assert!(first.contains("\"source\": \"fixture\""));
    assert!(first.contains("\"capability_only\": true"));
    assert!(first.contains("\"ata_smart_probe\": \"eligible\""));
    assert!(first.contains("\"target_environment\""));
    assert!(first.contains("\"ebpf_process_rates\": \"backend_not_compiled\""));
    assert!(first.contains("\"hotplug\": \"backend_unconfirmed\""));
    assert!(!first.contains("serial"));
    assert!(!first.contains("hostname"));
    assert!(first.ends_with('\n'));
}

#[cfg(target_os = "linux")]
#[test]
fn device_family_classification_rejects_partitions_and_controller_nodes() {
    assert!(is_ata_candidate("sda"));
    assert!(is_ata_candidate("hdaa"));
    assert!(!is_ata_candidate("sda1"));
    assert!(!is_ata_candidate("vda"));
    assert!(is_nvme_namespace("nvme0n1"));
    assert!(is_nvme_namespace("nvme12n3"));
    assert!(!is_nvme_namespace("nvme0"));
    assert!(!is_nvme_namespace("nvme0n1p2"));
}

#[cfg(target_os = "linux")]
#[test]
fn live_receipt_cannot_be_labelled_as_fixture() {
    let receipt = collect_linux_provider_capability_receipt();
    assert_eq!(receipt.source, ReceiptSource::LiveHost);
    assert_eq!(receipt.schema_version, CAPABILITY_RECEIPT_SCHEMA_VERSION);
    assert_ne!(
        receipt.init_evidence,
        InitRuntimeEvidence::UnsupportedPlatform
    );
}

#[test]
fn intel_engine_pmu_eligibility_keeps_permission_and_absence_distinct() {
    let mut absent = fixture_probe();
    absent.intel_gpu_engine_pmu_devices = 0;
    let receipt = build_receipt(ReceiptSource::Fixture, 900, absent);
    assert_eq!(
        receipt.target_environment.intel_gpu_engine_pmu,
        ProviderProbeEligibility::HardwareNotDetected
    );

    let mut unconfirmed = fixture_probe();
    unconfirmed.perf_event_paranoid = None;
    let receipt = build_receipt(ReceiptSource::Fixture, 901, unconfirmed);
    assert_eq!(
        receipt.target_environment.intel_gpu_engine_pmu,
        ProviderProbeEligibility::BackendUnconfirmed
    );

    let mut privileged = fixture_probe();
    privileged.effective_perfmon_privilege = true;
    let receipt = build_receipt(ReceiptSource::Fixture, 902, privileged);
    assert_eq!(
        receipt.target_environment.intel_gpu_engine_pmu,
        ProviderProbeEligibility::Eligible
    );
}
