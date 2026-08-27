use super::*;
use taskmanager_core::DeviceStatus;

#[test]
fn pdh_engine_breakdown_maps_to_typed_engine_rows() {
    let sample = taskmanager_windows_api::WindowsGpuEngineSample {
        luid: 0x10,
        utilization_pct: 42.0,
        engines: vec![
            taskmanager_windows_api::WindowsGpuEngineDetail {
                engine_name: "3D".into(),
                utilization_pct: 42.0,
            },
            taskmanager_windows_api::WindowsGpuEngineDetail {
                engine_name: "Video Decode".into(),
                utilization_pct: 7.5,
            },
            taskmanager_windows_api::WindowsGpuEngineDetail {
                engine_name: "Copy".into(),
                utilization_pct: 3.25,
            },
            taskmanager_windows_api::WindowsGpuEngineDetail {
                engine_name: "Neural".into(),
                utilization_pct: 1.0,
            },
            taskmanager_windows_api::WindowsGpuEngineDetail {
                engine_name: String::new(),
                utilization_pct: 99.0,
            },
        ],
    };
    let rows = engine_rows_from_pdh_sample(&sample);
    assert_eq!(rows.len(), 4, "an unnamed engine row is not a real row");
    assert_eq!(rows[0].name, "3D");
    assert_eq!(rows[0].kind, GpuEngineKind::Render);
    assert_eq!(rows[0].utilization_pct, 42.0);
    assert_eq!(rows[1].kind, GpuEngineKind::VideoDecode);
    assert_eq!(rows[2].kind, GpuEngineKind::Copy);
    // "Neural" has no provider-neutral class yet: Unknown, not a guess.
    assert_eq!(rows[3].kind, GpuEngineKind::Unknown);
}

#[test]
fn dxgi_identity_is_shared_by_inventory_and_engine_rows_lanes() {
    assert_eq!(
        gpu::dxgi_adapter_identity(0x10, false),
        "windows:gpu:dxgi:0000000000000010"
    );
    assert_eq!(
        gpu::dxgi_adapter_identity(0x10, true),
        "windows:npu:dxgi:0000000000000010"
    );
}

#[test]
fn engine_rows_for_an_unknown_device_stay_a_typed_failure() {
    let mut provider = WinGpuEngineRowsProvider::new();
    // A device id outside this provider's DXGI identities (or a dormant
    // boundary on the cross-target host) completes with the typed
    // Unsupported failure — never a sibling adapter's rows or an empty
    // success posing as "no engines".
    assert_eq!(
        provider.read_engine_rows(&DeviceId::new("not-a-windows-adapter")),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn memory_pressure_rate_is_temporarily_unavailable_without_a_previous_sample() {
    assert_eq!(
        used_rate_mib_per_sec(None, 100, 1_000),
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn memory_pressure_rate_is_temporarily_unavailable_when_no_time_elapsed() {
    // Same timestamp as the previous sample — would divide by zero.
    assert_eq!(
        used_rate_mib_per_sec(Some((100, 5_000)), 200, 5_000),
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn memory_pressure_rate_marks_a_backwards_clock_as_identity_change() {
    assert_eq!(
        used_rate_mib_per_sec(Some((100, 5_000)), 200, 4_999),
        ScalarObservation::unavailable(FailureKind::IdentityChanged)
    );
}

#[test]
fn memory_pressure_rate_computes_growth_in_mib_per_second() {
    const MIB: u64 = 1024 * 1024;
    // +3 MiB of used memory over 2 seconds -> 1.5 MiB/s.
    assert_eq!(
        used_rate_mib_per_sec(Some((100 * MIB, 0)), 103 * MIB, 2_000),
        ScalarObservation::available(1.5_f32, 2_000),
    );
}

#[test]
fn memory_pressure_rate_is_signed_for_freed_memory() {
    const MIB: u64 = 1024 * 1024;
    // -2 MiB over 1 second -> -2.0 MiB/s (memory released).
    assert_eq!(
        used_rate_mib_per_sec(Some((100 * MIB, 0)), 98 * MIB, 1_000),
        ScalarObservation::available(-2.0_f32, 1_000),
    );
}

#[test]
fn container_provider_refreshes_healthy_wsl_rollup() {
    let mut provider = WinContainerRollupProvider::new();
    let rollup = provider.refresh(1_000).expect("healthy wsl rollup");
    assert_eq!(rollup.state.status, DeviceStatus::Healthy);
}

#[test]
fn setupapi_accelerator_maps_to_a_discovery_first_npu_device() {
    let device = npu_device_from_setupapi(taskmanager_windows_api::WindowsComputeAccelerator {
        instance_path: "ACPI\\INTC1070\\1".into(),
        friendly_name: Some("Intel(R) AI Boost".into()),
        driver_desc: Some("Intel(R) AI Boost".into()),
    });
    assert_eq!(
        device.device_id.as_str(),
        "windows:npu:setupapi:acpi-intc1070-1"
    );
    assert_eq!(device.brand.as_deref(), Some("Intel(R) AI Boost"));
    assert_eq!(device.driver.as_deref(), Some("Intel(R) AI Boost"));
    // Discovery never fabricates a curve or a capacity (core NPU contract):
    // utilization and both memory totals stay typed-unavailable facts.
    assert_eq!(
        device.utilization_pct,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert!(device.engines.is_empty());
    assert_eq!(
        device.memory.dedicated_total_bytes,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    assert_eq!(
        device.memory.shared_total_bytes,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn setupapi_identity_fold_keeps_sibling_devices_distinct() {
    // Device instance paths are case-insensitive, so folding case is safe,
    // but sibling devices (here DEV_1170 vs DEV_1171) must stay distinct.
    assert_ne!(
        sanitize_setupapi_identity("PCI\\VEN_8086&DEV_1170\\3&11583659&0&A1"),
        sanitize_setupapi_identity("PCI\\VEN_8086&DEV_1171\\3&11583659&0&A1")
    );
    assert_eq!(
        sanitize_setupapi_identity("ACPI\\INTC1070\\1"),
        sanitize_setupapi_identity("acpi\\intc1070\\1")
    );
}

#[test]
#[cfg(not(windows))] // On Windows the enumeration is live; this proves the dormant-arm typing.
fn win_npu_inventory_stays_typed_unsupported_off_windows() {
    // Off-Windows the SetupAPI boundary is dormant, so the swapped-in
    // provider completes with the honest typed failure — never a fabricated
    // device row.
    let mut provider = WinNpuInventoryProvider::new();
    assert_eq!(
        provider.read_inventory(1_000),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn host_thread_query_is_native_and_failure_is_typed() {
    let mut provider = WinHostTelemetryProvider::new();
    let observation = provider
        .refresh(1_000)
        .expect("host observation must refresh without external commands");
    let facts = observation
        .current_value()
        .expect("host facts must be current");
    #[cfg(not(windows))]
    assert_eq!(
        facts.threads,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
    #[cfg(windows)]
    assert!(
        facts.threads.availability().is_current()
            || facts.threads.availability().failure().is_some()
    );
}

#[test]
fn live_memory_telemetry_populates_committed_and_paged_pools() {
    let mut provider = WinMemoryTelemetryProvider::new();
    let obs = provider
        .refresh(1_000)
        .expect("memory refresh should succeed");
    let metrics = obs.current_value().expect("memory metrics");
    assert!(metrics.current_total_bytes().is_some_and(|value| value > 0));
    assert!(metrics.current_used_bytes().is_some_and(|value| value > 0));
    // Committed/commit-limit/paged-pool come from the native
    // `system_performance()` boundary, which only exists on Windows. On the
    // cross-target Linux build that boundary returns its typed
    // `WindowsApiError::Unsupported`, so the provider keeps the plain
    // fields at their honest `None` absence instead of fabricating pools.
    #[cfg(windows)]
    {
        assert!(metrics.current_committed_bytes().is_some());
        assert!(metrics.current_commit_limit_bytes().is_some());
        assert!(metrics.current_reclaimable_bytes().is_some());
    }
    #[cfg(not(windows))]
    {
        assert!(metrics.current_committed_bytes().is_none());
        assert!(metrics.current_commit_limit_bytes().is_none());
        assert!(metrics.current_reclaimable_bytes().is_none());
        // The compression lane is dormant off-Windows (typed `Unsupported`
        // boundary), so the optional fact keeps its honest never-observed
        // absence instead of a fabricated zero.
        assert!(metrics.current_compressed_memory_used_bytes().is_none());
    }
    eprintln!(
        "LIVE MEMORY: total={} GB, used={} GB, committed={:?} GB, limit={:?} GB, paged_pool={:?} MB, cached={:?} MB",
        metrics.current_total_bytes().unwrap_or(0) / (1024 * 1024 * 1024),
        metrics.current_used_bytes().unwrap_or(0) / (1024 * 1024 * 1024),
        metrics
            .current_committed_bytes()
            .map(|b| b / (1024 * 1024 * 1024)),
        metrics
            .current_commit_limit_bytes()
            .map(|b| b / (1024 * 1024 * 1024)),
        metrics
            .current_reclaimable_bytes()
            .map(|b| b / (1024 * 1024)),
        metrics.current_cached_bytes().map(|b| b / (1024 * 1024)),
    );
}
