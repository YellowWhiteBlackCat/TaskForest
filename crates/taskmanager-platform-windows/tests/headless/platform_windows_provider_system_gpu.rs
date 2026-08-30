use super::*;

fn dxgi_sample(
    luid: u64,
    name: &str,
    pci_address: taskmanager_windows_api::WindowsPciAddress,
) -> DxgiGpuSample {
    DxgiGpuSample {
        pci_address: Some(pci_address),
        metrics: GpuMetrics::new(format!("windows:gpu:dxgi:{luid:016x}"), name),
    }
}

#[test]
fn mixed_nvml_and_dxgi_keeps_complete_luid_inventory_and_enriches_exact_match() {
    let nvidia_address = taskmanager_windows_api::WindowsPciAddress {
        bus: 1,
        device: 0,
        function: 0,
    };
    let intel_address = taskmanager_windows_api::WindowsPciAddress {
        bus: 0,
        device: 2,
        function: 0,
    };
    let dxgi = vec![
        dxgi_sample(0x10, "NVIDIA RTX", nvidia_address),
        dxgi_sample(0x20, "Intel Arc B390", intel_address),
    ];
    let mut nvml_metrics = GpuMetrics::new("must-not-become-inventory-identity", "NVIDIA RTX");
    nvml_metrics.apply_scalar_observations(taskmanager_core::GpuScalarObservations {
        utilization_pct: ScalarObservation::available(37.0, 1),
        ..taskmanager_core::GpuScalarObservations::default()
    });
    nvml_metrics.driver_version = Some("566.36".into());
    nvml_metrics.provenance = vec![
        GpuMetricProvenance {
            field: GpuMetricField::Utilization,
            provider: GPU_TELEMETRY_PROVIDER,
        },
        GpuMetricProvenance {
            field: GpuMetricField::DriverVersion,
            provider: GPU_TELEMETRY_PROVIDER,
        },
    ];
    let nvml = vec![NvmlGpuSample {
        pci_address: nvidia_address,
        metrics: nvml_metrics,
    }];

    let (rows, failure) = merge_gpu_samples(dxgi, nvml);
    assert_eq!(failure, None);
    assert_eq!(rows.len(), 2, "NVML must not suppress the Intel adapter");
    assert_eq!(rows[0].device_id, "windows:gpu:dxgi:0000000000000010");
    assert_eq!(rows[0].current_utilization_pct(), Some(37.0));
    assert_eq!(
        rows[0].driver_version.as_deref(),
        Some("566.36"),
        "the NVML sys driver version enriches the exact DXGI match"
    );
    assert_eq!(
        rows[1].driver_version, None,
        "the version must never copy to a sibling adapter"
    );
    assert_eq!(rows[1].device_id, "windows:gpu:dxgi:0000000000000020");
    assert_eq!(rows[1].brand, "Intel Arc B390");
    assert!(
        rows.iter()
            .all(|row| !row.device_id.contains("must-not-become")),
        "only exact DXGI LUIDs may authorize Windows GPU inventory rows"
    );
}

#[test]
fn unmatched_nvml_sample_is_partial_and_cannot_copy_to_a_sibling() {
    let dxgi_address = taskmanager_windows_api::WindowsPciAddress {
        bus: 1,
        device: 0,
        function: 0,
    };
    let dxgi = vec![dxgi_sample(0x10, "NVIDIA sibling", dxgi_address)];
    let mut unmatched = GpuMetrics::new("", "different PCI function");
    unmatched.apply_scalar_observations(taskmanager_core::GpuScalarObservations {
        utilization_pct: ScalarObservation::available(99.0, 1),
        ..Default::default()
    });
    let nvml = vec![NvmlGpuSample {
        pci_address: taskmanager_windows_api::WindowsPciAddress {
            function: 1,
            ..dxgi_address
        },
        metrics: unmatched,
    }];

    let (rows, failure) = merge_gpu_samples(dxgi, nvml);
    assert_eq!(failure, Some(FailureKind::Unsupported));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].current_utilization_pct(), None);
}

#[test]
fn nvml_cap_plus_one_is_bounded_and_explicitly_partial() {
    assert_eq!(
        nvml_enumeration_bound(MAX_NVML_GPU_DEVICES),
        (MAX_NVML_GPU_DEVICES, None)
    );
    assert_eq!(
        nvml_enumeration_bound(MAX_NVML_GPU_DEVICES + 1),
        (MAX_NVML_GPU_DEVICES, Some(FailureKind::Unsupported))
    );
}

#[test]
fn nvml_throttle_bits_preserve_confirmed_empty_known_and_future_states() {
    use nvml_wrapper::bitmasks::device::ThrottleReasons;

    assert!(map_nvml_throttle_bits(0).is_empty());
    assert_eq!(
        map_nvml_throttle_bits(ThrottleReasons::SW_POWER_CAP.bits()),
        vec![GpuThrottleReason::SoftwarePowerLimit]
    );
    let future = (0..u64::BITS)
        .map(|shift| 1_u64 << shift)
        .find(|bit| ThrottleReasons::all().bits() & bit == 0)
        .expect("wrapper leaves one future bit");
    assert_eq!(
        map_nvml_throttle_bits(future),
        vec![GpuThrottleReason::Other]
    );
}

#[cfg(windows)]
#[test]
fn pdh_memory_sample_matches_dxgi_names_without_fabrication() {
    let samples = vec![
        taskmanager_windows_api::WindowsGpuAdapterMemorySample {
            instance_name: "Intel(R) Arc(TM) Graphics".into(),
            luid: Some(0x0000_0000_0001_3126),
            dedicated_usage_bytes: Some(1234),
            shared_usage_bytes: Some(5678),
        },
        taskmanager_windows_api::WindowsGpuAdapterMemorySample {
            instance_name: "NVIDIA GeForce RTX 5090".into(),
            luid: Some(0x0000_0000_0001_3555),
            dedicated_usage_bytes: Some(999),
            shared_usage_bytes: None,
        },
    ];

    let arc =
        find_adapter_memory_sample("Intel(R) Arc(TM) Graphics", 0x0000_0000_0001_3126, &samples)
            .expect("LUID match must resolve");
    assert_eq!(arc.shared_usage_bytes, Some(5678));
    assert_eq!(arc.dedicated_usage_bytes, Some(1234));

    assert!(
        find_adapter_memory_sample("Intel Arc Graphics", 0, &samples).is_none(),
        "a friendly-name match must not cross the exact LUID authority"
    );

    let by_luid =
        find_adapter_memory_sample("Unknown Friendly Name", 0x0000_0000_0001_3555, &samples)
            .expect("LUID match must resolve even when the friendly name differs");
    assert_eq!(by_luid.instance_name, "NVIDIA GeForce RTX 5090");

    assert!(
        find_adapter_memory_sample("AMD Radeon(TM) Graphics", 0x99, &samples).is_none(),
        "an unmatched adapter must stay honestly absent"
    );
}

#[test]
fn live_win_gpu_provider_refresh() {
    let mut provider = WinGpuTelemetryProvider::new();
    let result = provider.refresh(1000);
    assert!(result.is_ok());
    let obs = result.unwrap();
    #[cfg(not(windows))]
    {
        // Cross-target model: the native DXGI/PDH adapter path only exists
        // on Windows. Without a loadable NVML runtime the refresh completes
        // as the typed `Unavailable(Unsupported)` snapshot — absence rides
        // the observation lane, never fabricated rows. A non-Windows host
        // with the NVIDIA driver may still load NVML and deliver real rows;
        // both honest arms are pinned by their typed shape.
        match obs.current_value() {
            Some(gpus) => {
                eprintln!(
                    "LIVE WIN GPU TELEMETRY (nvml fallback host): gpus count = {}",
                    gpus.len()
                );
                for gpu in gpus {
                    assert!(!gpu.brand.is_empty());
                }
            }
            None => {
                assert_eq!(
                    obs.state().failure(),
                    Some(FailureKind::Unsupported),
                    "the non-Windows adapter must complete GPU absence with the typed Unsupported outcome"
                );
            }
        }
    }
    #[cfg(windows)]
    {
        let gpus = obs.current_value().expect("gpus should be present");
        eprintln!("LIVE WIN GPU TELEMETRY: gpus count = {}", gpus.len());
        for gpu in gpus {
            eprintln!(
                "  DEVICE: id={}, brand={}, usage_pct={:?}, driver={:?}, driver_version={:?}, dedicated_vram={}, shared_vram={}, engines={:?}",
                gpu.device_id,
                gpu.brand,
                gpu.current_utilization_pct(),
                gpu.driver,
                gpu.driver_version,
                gpu.current_dedicated_vram_total_bytes().unwrap_or_default(),
                gpu.current_shared_vram_total_bytes().unwrap_or_default(),
                gpu.engines,
            );
            eprintln!(
                "    used: dedicated={:?}, shared={:?}, total={:?}",
                gpu.current_dedicated_vram_used_bytes(),
                gpu.current_shared_vram_used_bytes(),
                gpu.current_memory_used_bytes()
            );
            assert!(!gpu.brand.is_empty());
        }
        let identities = gpus
            .iter()
            .map(|gpu| gpu.device_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            identities.len(),
            gpus.len(),
            "physical adapters must have unique stable identities"
        );
        for gpu in gpus {
            if let Some(luid) = gpu.device_id.split(":dxgi:").nth(1) {
                assert_eq!(luid.len(), 16, "DXGI identity is the full 64-bit LUID");
                assert!(u64::from_str_radix(luid, 16).is_ok());
            }
        }
    }
}
