use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskMetrics, DiskScalarObservations, GpuEngineKind,
    GpuMetrics, GpuScalarObservations, GpuThrottleReason, MemoryCompositionObservations,
    MemoryCompressionObservations, MemoryMetrics, MemoryModuleObservations,
    MemoryOptionalObservations, MemoryScalarObservations, NetworkAdapterType, NetworkMetrics,
    NetworkScalarObservations, NetworkWirelessObservations, OptionalObservation, ScalarObservation,
    ScalarObservationGroup, SmartAvailability, StorageConnection, StorageDeviceKind,
    StorageIdentityStability, StorageInterconnect, StorageProtocol, SystemSnapshot,
    VirtualMemoryCommitObservations,
};

const GIB: u64 = 1024 * 1024 * 1024;

/// A representative telemetry snapshot with every optional branch populated so the
/// device detail views exercise their conditional render arms: swap graph + zram +
/// zswap + committed + usage-rate rows (memory), SMART temp/endurance/power-on +
/// removable (disk), SSID + signal + link-speed + utilization (network), dedicated
/// + shared VRAM + per-engine + power + clock + throttling (GPU).
pub(super) fn rich_snapshot() -> SystemSnapshot {
    let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(17.0, 1_000),
        core_usage_group: ScalarObservationGroup::available(vec![10.0, 20.0, 30.0, 40.0], 1_000),
        frequency_mhz: ScalarObservation::available(3_200, 1_000),
        max_frequency_mhz: ScalarObservation::available(4_400, 1_000),
        per_core_frequency_group: ScalarObservationGroup::available(
            vec![3_200, 3_100, 3_000, 2_900],
            1_000,
        ),
        temperature_c: ScalarObservation::available(55.0, 1_000),
        power_w: ScalarObservation::available(12.5, 1_000),
        ..Default::default()
    });
    cpu.brand = Some("Test CPU".into());
    cpu.physical_cores = Some(2);
    cpu.logical_cores = Some(4);
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(16 * GIB, 1_000),
            used_bytes: ScalarObservation::available(8 * GIB, 1_000),
            available_bytes: ScalarObservation::available(7 * GIB, 1_000),
            swap_total_bytes: ScalarObservation::available(4 * GIB, 1_000),
            swap_used_bytes: ScalarObservation::available(GIB, 1_000),
            used_rate_mib_per_sec: ScalarObservation::available(1.25, 1_000),
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::present(3 * GIB, 1_000),
                buffers_bytes: OptionalObservation::present(GIB / 2, 1_000),
                active_bytes: OptionalObservation::present(5 * GIB, 1_000),
                inactive_bytes: OptionalObservation::present(2 * GIB, 1_000),
                free_bytes: OptionalObservation::present(GIB, 1_000),
                reclaimable_bytes: OptionalObservation::present(GIB / 2, 1_000),
                ..Default::default()
            },
            hardware_reserved_bytes: OptionalObservation::present(GIB / 4, 1_000),
            modules: MemoryModuleObservations {
                speed_mhz: OptionalObservation::present(4_800, 1_000),
                slots_used: OptionalObservation::present(2, 1_000),
                slots_total: OptionalObservation::present(4, 1_000),
                module_type: OptionalObservation::present("DDR5".into(), 1_000),
                manufacturer: OptionalObservation::present("Samsung".into(), 1_000),
                form_factor: OptionalObservation::present("DIMM".into(), 1_000),
            },
            virtual_memory_commit: VirtualMemoryCommitObservations {
                committed_bytes: OptionalObservation::present(18 * GIB, 1_000),
                limit_bytes: OptionalObservation::present(20 * GIB, 1_000),
            },
            compression: MemoryCompressionObservations {
                compressed_swap_used_bytes: OptionalObservation::present(GIB / 2, 1_000),
                compressed_swap_capacity_bytes: OptionalObservation::present(2 * GIB, 1_000),
                compressed_swap_cache_enabled: OptionalObservation::present(true, 1_000),
                ..Default::default()
            },
        },
    );
    SystemSnapshot {
        timestamp_ms: 1_000,
        uptime_secs: 86_400 + 3_642,
        processes: 120,
        threads: Some(900),
        cpu,
        memory,
        disks: vec![rich_disk()],
        networks: vec![rich_network()],
        gpu: vec![rich_gpu()],
        telemetry_sources: Vec::new(),
        provider_states: Vec::new(),
        device_lifecycles: Default::default(),
    }
}

fn rich_disk() -> DiskMetrics {
    let mut disk = DiskMetrics::new("/dev/nvme0n1");
    disk.device_id = "disk:wwid:test-disk".into();
    disk.device_state = DeviceState::healthy(1);
    disk.disk_type = "NVMe SSD".into();
    disk.identity_stability = StorageIdentityStability::Persistent;
    disk.model = "TestDisk Model 1TB".into();
    disk.mount_point = "/".into();
    disk.fs_type = "ext4".into();
    disk.smart_availability = SmartAvailability::Available;
    disk.smart_state = DeviceState::healthy(1);
    disk.smart_temperature_c = Some(42.0);
    disk.smart_critical_warning = Some(false);
    disk.smart_temp_critical_c = Some(85.0);
    disk.smart_percent_used = Some(7.0);
    disk.smart_power_on_hours = Some(8_760);
    disk.apply_connection(StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Pcie,
        StorageDeviceKind::Physical,
    ));
    disk.apply_attachment_capabilities(Some(false), Some(false));
    disk.apply_scalar_observations(DiskScalarObservations {
        capacity_bytes: ScalarObservation::available(500 * GIB, 1_000),
        available_bytes: ScalarObservation::available(250 * GIB, 1_000),
        read_bytes_per_sec: ScalarObservation::available(50 * 1024 * 1024, 1_000),
        write_bytes_per_sec: ScalarObservation::available(12 * 1024 * 1024, 1_000),
        iops: ScalarObservation::available(3_400, 1_000),
        active_time_pct: ScalarObservation::available(33.0, 1_000),
        response_time_ms: ScalarObservation::available(1.2, 1_000),
    });
    disk
}

fn rich_network() -> NetworkMetrics {
    let mut network = NetworkMetrics::new("wlan0");
    network.device_id = "net:mac:aa:bb:cc:dd:ee:ff".into();
    network.device_state = DeviceState::healthy(1);
    network.ipv4_addr = Some("192.168.1.42".into());
    network.ipv6_addr = Some("fe80::1".into());
    network.mac_addr = Some("aa:bb:cc:dd:ee:ff".into());
    network.apply_observations(
        NetworkAdapterType::WiFi,
        NetworkScalarObservations {
            rx_bytes_per_sec: ScalarObservation::available(1_250_000, 1_000),
            tx_bytes_per_sec: ScalarObservation::available(480_000, 1_000),
            total_rx_bytes: ScalarObservation::available(12 * GIB, 1_000),
            total_tx_bytes: ScalarObservation::available(3 * GIB, 1_000),
            link_speed_mbps: ScalarObservation::available(866, 1_000),
            utilization_pct: ScalarObservation::available(12.5, 1_000),
            ..Default::default()
        },
        NetworkWirelessObservations {
            association: OptionalObservation::present(true, 1_000),
            signal_dbm: OptionalObservation::present(-55, 1_000),
            ssid: OptionalObservation::present("HomeNet".into(), 1_000),
            ..Default::default()
        },
    );
    network
}

fn rich_gpu() -> GpuMetrics {
    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(41.0, 1_000),
        idle_residency_pct: ScalarObservation::available(52.0, 1_000),
        memory_used_bytes: ScalarObservation::available(2 * GIB, 1_000),
        memory_total_bytes: ScalarObservation::available(8 * GIB, 1_000),
        dedicated_vram_used_bytes: ScalarObservation::available(2 * GIB, 1_000),
        dedicated_vram_total_bytes: ScalarObservation::available(8 * GIB, 1_000),
        shared_vram_used_bytes: ScalarObservation::available(GIB / 2, 1_000),
        shared_vram_total_bytes: ScalarObservation::available(4 * GIB, 1_000),
        temperature_c: ScalarObservation::available(63.0, 1_000),
        power_w: ScalarObservation::available(95.0, 1_000),
        frequency_mhz: ScalarObservation::available(2_100, 1_000),
        ..Default::default()
    });
    gpu.device_id = "gpu:pci:0000:00:02.0".into();
    gpu.device_state = DeviceState::healthy(1);
    gpu.brand = "Test GPU".into();
    gpu.engines = vec![
        taskmanager_core::core::metrics::GpuEngine {
            name: "3D".into(),
            kind: GpuEngineKind::Unknown,
            usage_pct: 60.0,
        },
        taskmanager_core::core::metrics::GpuEngine {
            name: "Compute".into(),
            kind: GpuEngineKind::Compute,
            usage_pct: 10.0,
        },
    ];
    gpu.apply_throttle_observation(ScalarObservation::available(
        vec![GpuThrottleReason::SoftwareThermalLimit],
        1_000,
    ));
    gpu
}
