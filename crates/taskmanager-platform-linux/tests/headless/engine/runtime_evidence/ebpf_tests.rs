use super::*;

fn probe() -> RawCapabilityProbe {
    RawCapabilityProbe {
        supported_platform: true,
        hardware_build_profile: HardwareBuildProfile::StandardAll,
        init_evidence: InitRuntimeEvidence::OpenrcRuntime,
        ata_candidate_devices: 1,
        nvme_namespace_devices: 1,
        nvidia_device_markers: 1,
        smartctl_available: true,
        nvme_cli_available: true,
        systemctl_available: true,
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
fn modern_or_compat_is_eligible_without_cgroup_v2() {
    let mut compat = probe();
    compat.ebpf_backend_compiled = true;
    compat.kernel_btf_available = false;
    compat.cgroup_v2_available = false;
    compat.ebpf_compat_environment_available = true;
    compat.effective_bpf_privilege = true;
    assert_eq!(
        build_receipt(ReceiptSource::Fixture, 1, compat)
            .target_environment
            .ebpf_process_rates,
        ProviderProbeEligibility::Eligible
    );

    let mut modern = probe();
    modern.ebpf_backend_compiled = true;
    modern.cgroup_v2_available = false;
    modern.ebpf_compat_environment_available = false;
    modern.effective_bpf_privilege = true;
    assert_eq!(
        build_receipt(ReceiptSource::Fixture, 2, modern)
            .target_environment
            .ebpf_process_rates,
        ProviderProbeEligibility::Eligible
    );
}

#[test]
fn missing_paths_permission_and_unknown_policy_remain_distinct() {
    let mut missing = probe();
    missing.ebpf_backend_compiled = true;
    missing.kernel_btf_available = false;
    missing.ebpf_compat_environment_available = false;
    assert_eq!(
        build_receipt(ReceiptSource::Fixture, 1, missing)
            .target_environment
            .ebpf_process_rates,
        ProviderProbeEligibility::BackendInactive
    );

    let mut denied = probe();
    denied.ebpf_backend_compiled = true;
    assert_eq!(
        build_receipt(ReceiptSource::Fixture, 2, denied)
            .target_environment
            .ebpf_process_rates,
        ProviderProbeEligibility::PrivilegeRequired
    );

    let mut unknown = probe();
    unknown.ebpf_backend_compiled = true;
    unknown.unprivileged_bpf_disabled = None;
    assert_eq!(
        build_receipt(ReceiptSource::Fixture, 3, unknown)
            .target_environment
            .ebpf_process_rates,
        ProviderProbeEligibility::BackendUnconfirmed
    );
}

#[test]
fn effective_capabilities_and_absent_hardware_are_typed() {
    assert!(ebpf::has_capability_set(Some(1_u64 << 21)));
    assert!(ebpf::has_capability_set(Some(
        (1_u64 << 38) | (1_u64 << 39)
    )));
    assert!(!ebpf::has_capability_set(Some(1_u64 << 39)));
    assert!(!ebpf::has_capability_set(None));
    assert!(ebpf::has_perfmon_capability(Some(1_u64 << 21)));
    assert!(ebpf::has_perfmon_capability(Some(1_u64 << 38)));
    assert!(!ebpf::has_perfmon_capability(Some(1_u64 << 39)));
    assert!(!ebpf::has_perfmon_capability(None));

    let mut absent = probe();
    absent.init_evidence = InitRuntimeEvidence::UnknownPid1;
    absent.ata_candidate_devices = 0;
    absent.nvidia_device_markers = 0;
    absent.openrc_tools_available = false;
    absent.ebpf_backend_compiled = false;
    let receipt = build_receipt(ReceiptSource::Fixture, 4, absent);
    assert_eq!(
        receipt.ata_smart_probe,
        ProviderProbeEligibility::HardwareNotDetected
    );
    assert_eq!(
        receipt.nvidia_nvml_probe,
        ProviderProbeEligibility::HardwareNotDetected
    );
    assert_eq!(
        receipt.openrc_probe,
        ProviderProbeEligibility::BackendUnconfirmed
    );
}
