use super::*;
use taskmanager_core::FailureKind;

fn complete_readout(pci_bus_id: &str) -> NvmlDeviceReadout {
    NvmlDeviceReadout {
        brand: Ok("NVIDIA Test GPU".to_string()),
        pci_bus_id: Ok(pci_bus_id.to_string()),
        uuid: Ok(format!("GPU-{pci_bus_id}")),
        utilization: Ok((41, 12)),
        memory: Ok((2_048, 8_192)),
        temperature_c: Ok(63.0),
        power_w: Ok(145.0),
        current_clock_mhz: Ok(1_900),
        max_clock_mhz: Ok(2_500),
        encoder_pct: Ok(7.0),
        decoder_pct: Ok(11.0),
        fan_speed_pct: Ok(48.0),
        throttle_reasons: Ok(vec![GpuThrottleReason::SoftwarePowerLimit]),
    }
}

#[test]
fn partial_api_failures_keep_successful_fields_and_only_real_provenance() {
    let mut readout = complete_readout("00000000:03:00.0");
    readout.brand = Err(NvmlFailureKind::PermissionDenied);
    readout.memory = Err(NvmlFailureKind::NotSupported);
    readout.throttle_reasons = Err(NvmlFailureKind::Transient);

    let assembly = assemble_nvml_device(readout);
    let sample = assembly.sample.expect("PCI identity remains available");

    assert!(sample.fields.contains(&GpuMetricField::Identity));
    assert!(sample.fields.contains(&GpuMetricField::Utilization));
    assert!(sample.fields.contains(&GpuMetricField::Fan));
    assert!(!sample.fields.contains(&GpuMetricField::Brand));
    assert!(!sample.fields.contains(&GpuMetricField::Memory));
    assert!(!sample.fields.contains(&GpuMetricField::Throttle));
    assert_eq!(sample.metrics.current_fan_speed_pct(), Some(48.0));
    assert_eq!(sample.metrics.current_utilization_pct(), Some(41.0));
    assert_eq!(sample.metrics.current_memory_total_bytes(), None);
    assert!(sample.field_failures.contains(&GpuProviderFieldFailure {
        field: GpuMetricField::Brand,
        failure: FailureKind::PermissionDenied,
    }));
    assert!(sample.field_failures.contains(&GpuProviderFieldFailure {
        field: GpuMetricField::Memory,
        failure: FailureKind::Unsupported,
    }));
    assert!(sample.field_failures.contains(&GpuProviderFieldFailure {
        field: GpuMetricField::Throttle,
        failure: FailureKind::TemporarilyUnavailable,
    }));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Brand,
        kind: NvmlFailureKind::PermissionDenied,
    }));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Memory,
        kind: NvmlFailureKind::NotSupported,
    }));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Throttle,
        kind: NvmlFailureKind::Transient,
    }));
}

#[test]
fn not_supported_fan_does_not_erase_other_device_fields() {
    let mut readout = complete_readout("00000000:04:00.0");
    readout.fan_speed_pct = Err(NvmlFailureKind::NotSupported);

    let assembly = assemble_nvml_device(readout);
    let sample = assembly.sample.expect("stable PCI identity");

    assert!(!sample.fields.contains(&GpuMetricField::Fan));
    assert!(sample.fields.contains(&GpuMetricField::Temperature));
    assert!(sample.fields.contains(&GpuMetricField::Power));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Fan,
        kind: NvmlFailureKind::NotSupported,
    }));
}

#[test]
fn unsupported_throttle_api_is_exact_and_does_not_erase_sibling_metrics() {
    let mut readout = complete_readout("00000000:05:00.0");
    readout.throttle_reasons = Err(NvmlFailureKind::NotSupported);

    let assembly = assemble_nvml_device(readout);
    let sample = assembly.sample.expect("stable PCI identity");

    assert!(!sample.fields.contains(&GpuMetricField::Throttle));
    for field in [
        GpuMetricField::Utilization,
        GpuMetricField::Memory,
        GpuMetricField::Temperature,
        GpuMetricField::Power,
        GpuMetricField::Fan,
        GpuMetricField::Frequency,
    ] {
        assert!(
            sample.fields.contains(&field),
            "throttle failure erased sibling {field:?}"
        );
    }
    assert!(sample.field_failures.contains(&GpuProviderFieldFailure {
        field: GpuMetricField::Throttle,
        failure: FailureKind::Unsupported,
    }));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Throttle,
        kind: NvmlFailureKind::NotSupported,
    }));
}

#[test]
fn uuid_is_a_stable_fallback_when_pci_identity_is_temporarily_unavailable() {
    let mut readout = complete_readout("00000000:04:00.0");
    readout.pci_bus_id = Err(NvmlFailureKind::Transient);
    readout.uuid = Ok(" GPU-stable-uuid ".to_string());

    let assembly = assemble_nvml_device(readout);
    let sample = assembly.sample.expect("UUID remains a stable identity");

    assert_eq!(sample.metrics.device_id, "gpu:uuid:GPU-stable-uuid");
    assert!(sample.fields.contains(&GpuMetricField::Identity));
    assert!(assembly.failures.contains(&NvmlFieldFailure {
        field: GpuMetricField::Identity,
        kind: NvmlFailureKind::Transient,
    }));
}

#[test]
fn one_failed_fan_keeps_other_real_fan_readings() {
    assert_eq!(
        select_maximum_fan_speed(&[Ok(40.0), Err(NvmlFailureKind::Transient), Ok(60.0),]),
        Ok(60.0)
    );
    assert_eq!(
        select_maximum_fan_speed(&[Err(NvmlFailureKind::NotSupported)]),
        Err(NvmlFailureKind::NotSupported)
    );
}

#[test]
fn multi_card_pci_identity_merges_stably_independent_of_nvml_order() {
    fn baseline(slot: &str) -> GpuMetrics {
        GpuMetrics::new(stable_gpu_id("card", Some(slot)), "NVIDIA")
    }
    fn assembled(slot: &str) -> GpuProviderSample {
        assemble_nvml_device(complete_readout(slot))
            .sample
            .expect("fixture has PCI identity")
    }

    let mut left = vec![baseline("0000:03:00.0"), baseline("0000:04:00.0")];
    let mut right = left.clone();
    merge_provider_samples(
        &mut left,
        taskmanager_core::ProviderId::borrowed("linux.gpu.nvml"),
        vec![assembled("00000000:03:00.0"), assembled("00000000:04:00.0")],
    );
    merge_provider_samples(
        &mut right,
        taskmanager_core::ProviderId::borrowed("linux.gpu.nvml"),
        vec![assembled("00000000:04:00.0"), assembled("00000000:03:00.0")],
    );

    assert_eq!(
        left.iter()
            .map(|gpu| (&gpu.device_id, gpu.current_memory_total_bytes()))
            .collect::<Vec<_>>(),
        right
            .iter()
            .map(|gpu| (&gpu.device_id, gpu.current_memory_total_bytes()))
            .collect::<Vec<_>>()
    );
    assert_eq!(left.len(), 2);
}
