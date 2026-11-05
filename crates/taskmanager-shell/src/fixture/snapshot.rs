//! Deterministic system and hardware snapshot fixtures.

use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::identity::DeviceGeneration;
use taskforest_core::core::metrics::{
    CpuIdleState, CpuInterruptSnapshot, CpuMetrics, CpuPackageMetrics, CpuPerformancePolicy,
    CpuScalarObservations, DiskMetrics, DiskScalarObservations, GpuGraphicsApi, GpuMetrics,
    GpuScalarObservations, MemoryCompositionObservations, MemoryCompressionObservations,
    MemoryMetrics, MemoryModuleObservations, MemoryOptionalObservations, MemoryScalarObservations,
    NetworkAdapterType, NetworkMetrics, NetworkScalarObservations, NetworkWirelessObservations,
    OptionalObservation, ScalarObservation, ScalarObservationGroup, SystemLoadAverage,
    SystemSnapshot, VirtualMemoryCommitObservations,
};

use super::cpu_topology::{
    core_usage_seed, cpu_types_seed, demo_cpu_topology, per_core_frequency_seed,
    per_core_temperature_seed,
};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

pub(super) fn snapshot() -> SystemSnapshot {
    let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(37.4, 1_785_292_800_000),
        core_usage_group: ScalarObservationGroup::available(core_usage_seed(), 1_785_292_800_000),
        per_core_frequency_group: ScalarObservationGroup::available(
            per_core_frequency_seed(),
            1_785_292_800_000,
        ),
        per_core_temperature_group: ScalarObservationGroup::available(
            per_core_temperature_seed(),
            1_785_292_800_000,
        ),
        frequency_mhz: ScalarObservation::available(3_284, 1_785_292_800_000),
        temperature_c: ScalarObservation::available(54.0, 1_785_292_800_000),
        ..Default::default()
    });
    cpu.brand = Some("Intel(R) Core(TM) Ultra 7 358H".into());
    cpu.physical_cores = Some(demo_cpu_topology().physical_cores());
    cpu.logical_cores = Some(demo_cpu_topology().logical_cores());
    cpu.performance_policy = CpuPerformancePolicy {
        frequency_implementation: Some("intel_pstate".into()),
        active_policy: Some("powersave".into()),
        energy_preference: Some("balance_performance".into()),
        boost_enabled: Some(true),
        boost_max_frequency_mhz: Some(4_800),
        power_limit_1_w: Some(89.0),
        power_limit_2_w: Some(95.0),
        power_time_window_ms: Some(55_000),
    };
    cpu.packages = vec![CpuPackageMetrics {
        package_id: 0,
        numa_node_id: Some(0),
        numa_node_ids: vec![0],
        logical_core_ids: (0..cpu.logical_cores.unwrap_or_default()).collect(),
        chiplet_ids: vec![0],
        smt_threads_per_core: Some(2),
        smt_sibling_groups: vec![(0..2).collect()],
        physical_core_count: cpu.physical_cores,
        local_memory_bytes: Some(32 * GIB),
        numa_hit_ratio_pct: Some(99.8),
        is_throttled: Some(false),
        package_throttle_count: Some(0),
        core_throttle_count: Some(0),
        thermal_margin_c: Some(46.0),
        temperature_c: Some(54.0),
        power_w: Some(18.8),
        frequency_mhz: Some(3_284),
    }];
    cpu.idle_states = vec![
        CpuIdleState {
            name: "C1".into(),
            description: Some("C1-HLT".into()),
            residency_us: Some(611_700_000),
            residency_pct: Some(12.5),
            usage_count: Some(611_700),
            latency_us: Some(1),
            disabled: Some(false),
        },
        CpuIdleState {
            name: "C10".into(),
            description: Some("深度空闲".into()),
            residency_us: Some(4_300_000_000),
            residency_pct: Some(82.5),
            usage_count: Some(4_300_000),
            latency_us: Some(200),
            disabled: Some(false),
        },
    ];
    cpu.interrupts = Some(CpuInterruptSnapshot {
        total: Some(2_350_000),
        per_logical_cpu: (0..cpu.logical_cores.unwrap_or_default())
            .map(|index| 80_000 + u64::from((index % 4) as u8) * 10_000)
            .collect(),
    });
    SystemSnapshot {
        timestamp_ms: 1_785_292_800_000,
        cpu,
        memory: MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(32 * GIB, 1_785_292_800_000),
                used_bytes: ScalarObservation::available(12 * GIB + 640 * MIB, 1_785_292_800_000),
                available_bytes: ScalarObservation::available(
                    19 * GIB + 384 * MIB,
                    1_785_292_800_000,
                ),
                swap_total_bytes: ScalarObservation::available(8 * GIB, 1_785_292_800_000),
                swap_used_bytes: ScalarObservation::available(620 * MIB, 1_785_292_800_000),
                used_rate_mib_per_sec: ScalarObservation::available(180.2, 1_785_292_800_000),
                ..Default::default()
            },
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    cached_bytes: OptionalObservation::present(7 * GIB, 1_785_292_800_000),
                    buffers_bytes: OptionalObservation::present(24 * MIB, 1_785_292_800_000),
                    free_bytes: OptionalObservation::present(
                        12 * GIB + 384 * MIB,
                        1_785_292_800_000,
                    ),
                    ..Default::default()
                },
                hardware_reserved_bytes: OptionalObservation::present(128 * MIB, 1_785_292_800_000),
                modules: MemoryModuleObservations {
                    speed_mhz: OptionalObservation::present(5600, 1_785_292_800_000),
                    slots_used: OptionalObservation::present(2, 1_785_292_800_000),
                    slots_total: OptionalObservation::present(2, 1_785_292_800_000),
                    module_type: OptionalObservation::present("DDR5".into(), 1_785_292_800_000),
                    form_factor: OptionalObservation::present("SODIMM".into(), 1_785_292_800_000),
                    ..Default::default()
                },
                virtual_memory_commit: VirtualMemoryCommitObservations {
                    committed_bytes: OptionalObservation::present(
                        18 * GIB + 800 * MIB,
                        1_785_292_800_000,
                    ),
                    limit_bytes: OptionalObservation::present(78 * GIB, 1_785_292_800_000),
                },
                compression: MemoryCompressionObservations {
                    compressed_swap_used_bytes: OptionalObservation::present(
                        8 * GIB + 600 * MIB,
                        1_785_292_800_000,
                    ),
                    compressed_swap_capacity_bytes: OptionalObservation::present(
                        30 * GIB + 900 * MIB,
                        1_785_292_800_000,
                    ),
                    compressed_swap_original_bytes: OptionalObservation::present(
                        8 * GIB,
                        1_785_292_800_000,
                    ),
                    compressed_swap_compressed_bytes: OptionalObservation::present(
                        2 * GIB,
                        1_785_292_800_000,
                    ),
                    compressed_swap_memory_used_bytes: OptionalObservation::present(
                        2 * GIB,
                        1_785_292_800_000,
                    ),
                    compressed_swap_cache_enabled: OptionalObservation::present(
                        false,
                        1_785_292_800_000,
                    ),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        disks: vec![{
            let mut disk = DiskMetrics::new("nvme0n1");
            disk.device_id = "disk:demo:nvme0".into();
            disk.disk_type = "NVMe SSD".into();
            disk.model = "TiPro9000 2TB".into();
            disk.mount_point = "/".into();
            disk.device_generation = DeviceGeneration::new(1);
            disk.apply_scalar_observations(DiskScalarObservations {
                capacity_bytes: ScalarObservation::available(2_000 * GIB, 1_785_292_800_000),
                available_bytes: ScalarObservation::available(1_240 * GIB, 1_785_292_800_000),
                read_bytes_per_sec: ScalarObservation::available(84 * MIB, 1_785_292_800_000),
                write_bytes_per_sec: ScalarObservation::available(31 * MIB, 1_785_292_800_000),
                active_time_pct: ScalarObservation::available(12.7, 1_785_292_800_000),
                average_queue_depth: ScalarObservation::available(0.42, 1_785_292_800_000),
                service_time_ms: ScalarObservation::available(1.8, 1_785_292_800_000),
                ..Default::default()
            });
            disk
        }],
        networks: vec![{
            let mut network = NetworkMetrics::new("wlan0");
            network.device_id = "network:demo:wlan0".into();
            network.ipv4_addr = Some("192.168.1.42".into());
            network.device_generation = DeviceGeneration::new(1);
            network.apply_observations(
                NetworkAdapterType::WiFi,
                NetworkScalarObservations {
                    rx_bytes_per_sec: ScalarObservation::available(12 * MIB, 1_785_292_800_000),
                    tx_bytes_per_sec: ScalarObservation::available(2 * MIB, 1_785_292_800_000),
                    link_up: ScalarObservation::available(true, 1_785_292_800_000),
                    ..Default::default()
                },
                NetworkWirelessObservations {
                    association: OptionalObservation::present(true, 1_785_292_800_000),
                    ssid: OptionalObservation::present("TaskForest Lab".into(), 1_785_292_800_000),
                    ..Default::default()
                },
            );
            network
        }],
        gpu: vec![{
            let mut gpu = GpuMetrics::new("gpu:demo:xe", "Intel Graphics (xe)");
            gpu.device_state = DeviceState::healthy(1_785_292_800_000);
            gpu.driver = Some("xe".into());
            gpu.vbios_version = Some("101.0.0.0".into());
            gpu.graphics_api = Some(GpuGraphicsApi {
                opengl_version: Some("4.6".into()),
                vulkan_version: Some("1.4.304".into()),
                mesa_version: Some("25.1.4".into()),
            });
            gpu.device_generation = DeviceGeneration::new(1);
            gpu.apply_scalar_observations(GpuScalarObservations {
                utilization_pct: ScalarObservation::available(18.0, 1_785_292_800_000),
                idle_residency_pct: ScalarObservation::available(78.0, 1_785_292_800_000),
                temperature_c: ScalarObservation::available(48.0, 1_785_292_800_000),
                frequency_mhz: ScalarObservation::available(900, 1_785_292_800_000),
                ..Default::default()
            });
            gpu
        }],
        telemetry_sources: Vec::new(),
        provider_states: Vec::new(),
        device_lifecycles: Default::default(),
        uptime_secs: 6 * 3600 + 42 * 60,
        processes: 347,
        threads: Some(2_816),
        pressure: None,
        load_average: SystemLoadAverage::from_raw(2.4, 1.8, 1.2, 8),
    }
}

pub(super) fn hardware() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("Linux".into()),
        os_version: Some("Arch Linux".into()),
        kernel_version: Some("6.18.7-arch1-1".into()),
        hostname: Some("taskforest-workstation".into()),
        cpu_brand: Some("Intel(R) Core(TM) Ultra 7 358H".into()),
        cpu_types: cpu_types_seed(),
        cpu_cores: Some(demo_cpu_topology().logical_cores()),
        sockets: Some(1),
        total_memory_mb: Some(32 * 1024),
        architecture: Some(std::env::consts::ARCH.into()),
        motherboard_vendor: Some("LENOVO".into()),
        motherboard_model: Some("21L6000CSC".into()),
        firmware_release_date: Some("2025-06-11".into()),
        secure_boot: Some(true),
        ..Default::default()
    }
}
