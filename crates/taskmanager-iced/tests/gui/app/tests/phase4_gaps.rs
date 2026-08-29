//! Phase 4 gap-closing tests: verifying the 10 deep parity additions in Iced.

use crate::IcedApp;
use crate::app::{FocusTarget, Message};
use taskmanager_core::core::hardware::{CoreBreakdown, CpuType, HardwareInfo};
use taskmanager_core::core::metrics::{
    CpuScalarObservations, GpuMetrics, GpuScalarObservations, MemoryCompressionObservations,
    MemoryMetrics, MemoryOptionalObservations, OptionalObservation, ScalarObservation,
    SystemSnapshot,
};
use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind,
    ServiceStatus,
};

#[test]
fn test_disk_partition_panel_renders_active_partitions_and_usage() {
    let app = IcedApp::demo();
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .model("Samsung SSD 990 PRO 2TB".to_string())
        .partitions(vec![
            taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                .device_id(String::new())
                .parent_device_id(String::new())
                .device_generation(Default::default())
                .device_state(Default::default())
                .name("nvme0n1p1".to_string())
                .mount_point("/".to_string())
                .fs_type("ext4".to_string())
                .current_capacity_bytes(1_000_000_000_000)
                .current_used_bytes(400_000_000_000)
                .current_free_bytes(600_000_000_000)
                .build(),
            taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                .device_id(String::new())
                .parent_device_id(String::new())
                .device_generation(Default::default())
                .device_state(Default::default())
                .name("nvme0n1p2".to_string())
                .mount_point("/home".to_string())
                .fs_type("btrfs".to_string())
                .current_capacity_bytes(1_000_000_000_000)
                .current_used_bytes(950_000_000_000)
                .current_free_bytes(50_000_000_000)
                .build(),
        ])
        .build();

    let units = crate::ui::UnitPrefs {
        use_bytes: false,
        use_base2: true,
    };
    let panel = crate::ui::perf_devices::disk::partition_panel(&disk, app.theme(), units);
    assert!(
        panel.is_some(),
        "Partition panel must render when partitions are present"
    );
}

#[test]
fn test_gpu_vram_meters_dedicated_and_shared() {
    let app = IcedApp::demo();
    let mut gpu = GpuMetrics::new("", "NVIDIA GeForce RTX 4090");
    gpu.apply_scalar_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(8_000_000_000, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(24_000_000_000, 1),
        shared_vram_used_bytes: ScalarObservation::available(2_000_000_000, 1),
        shared_vram_total_bytes: ScalarObservation::available(32_000_000_000, 1),
        ..Default::default()
    });

    let panel = crate::ui::perf_devices::gpu::gpu_vram_meters_panel(&gpu, app.theme());
    assert!(
        panel.is_some(),
        "VRAM meters panel must render when VRAM metrics are reported"
    );
}

#[test]
fn test_memory_compression_card_savings() {
    let app = IcedApp::demo();
    let memory = MemoryMetrics::from_observations(
        Default::default(),
        MemoryOptionalObservations {
            compression: MemoryCompressionObservations {
                compressed_memory_used_bytes: OptionalObservation::present(1_500_000_000, 1),
                compressed_swap_used_bytes: OptionalObservation::present(500_000_000, 1),
                compressed_swap_cache_enabled: OptionalObservation::present(true, 1),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let card = crate::ui::perf_overview::memory::compression_card_view(&memory, app.theme());
    assert!(
        card.is_some(),
        "Memory compression card must render when ZRAM/compression is active"
    );
}

#[test]
fn test_service_details_interactive_dependency_links() {
    let mut app = IcedApp::demo();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            ServiceItem::from_inventory(
                "network.target",
                "network.target",
                ServiceStatus::Active,
                "Network Target",
                "loaded",
                "active",
                "active",
            ),
            ServiceItem::from_inventory(
                "sshd.service",
                "sshd.service",
                ServiceStatus::Active,
                "OpenSSH Server",
                "loaded",
                "active",
                "running",
            ),
        ])),
    );

    let deps = ServiceDeps::from_relations(ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "network.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Wants, "syslog.target"),
        ServiceRelationEdge::new(ServiceRelationKind::WantedBy, "multi-user.target"),
        ServiceRelationEdge::new(ServiceRelationKind::After, "network.target"),
    ]));

    let service_id = taskmanager_core::core::target::ServiceId::new("systemd:demo.service");
    let mut lifecycle = taskmanager_application::ServiceDependenciesLifecycle::default();
    lifecycle.begin(
        taskmanager_platform_contract::RequestId::MIN,
        service_id.clone(),
    );
    lifecycle.resolve(
        taskmanager_platform_contract::RequestId::MIN,
        service_id,
        deps,
    );
    let panel = crate::ui::service_details::dependency_panel(&app, &lifecycle);
    drop(panel);
}

#[test]
fn test_thermal_heatmap_and_sensor_badges() {
    let app = IcedApp::demo();
    let mut snapshot = SystemSnapshot {
        cpu: taskmanager_core::core::metrics::CpuMetrics::from_observations(
            CpuScalarObservations {
                temperature_c: ScalarObservation::available(65.0, 1),
                ..Default::default()
            },
        ),
        ..Default::default()
    };

    let mut gpu = GpuMetrics::new("", "Discrete GPU");
    gpu.apply_scalar_observations(GpuScalarObservations {
        temperature_c: ScalarObservation::available(82.0, 1),
        ..Default::default()
    });
    snapshot.gpu = vec![gpu];

    let panel = crate::ui::health::sensors_and_thermal_panel(&snapshot, app.theme());
    assert!(
        panel.is_some(),
        "Sensors and thermal heatmap panel must render when temperatures exist"
    );
}

#[test]
fn test_heterogeneous_cpu_core_breakdown_and_tags() {
    let mut app = IcedApp::demo();
    let hw = HardwareInfo {
        core_breakdown: CoreBreakdown {
            p_cores: 8,
            e_cores: 16,
            lp_cores: 0,
        },
        cpu_types: vec![
            CpuType::Performance,
            CpuType::Performance,
            CpuType::Efficient,
            CpuType::Efficient,
        ],
        ..Default::default()
    };
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Hardware(Some(Box::new(hw))),
    );

    let panel = crate::ui::core_grid::per_core_grid_panel(&app, app.theme());
    drop(panel);
}

#[test]
fn test_run_new_task_modal_lifecycle_and_elevation() {
    let mut app = IcedApp::demo();
    assert!(!app.run_task_open());

    // 1. Open
    let _ = app.update(Message::OpenRunTask);
    assert!(app.run_task_open());
    assert!(!app.run_task.as_admin);
    assert!(app.run_task.error_msg.is_none());

    // 2. Edit command
    let _ = app.update(Message::UpdateRunTaskCommand("pwsh.exe".to_string()));
    assert_eq!(app.run_task.command, "pwsh.exe");

    // 3. Toggle admin
    let _ = app.update(Message::ToggleRunTaskAdmin);
    assert!(app.run_task.as_admin);
    let _ = app.update(Message::ToggleRunTaskAdmin);
    assert!(!app.run_task.as_admin);

    // 4. Submit non-empty
    let _ = app.update(Message::SubmitRunTask);
    assert!(!app.run_task_open());
    assert!(app.run_task.command.is_empty());

    // 5. Submit empty triggers error
    let _ = app.update(Message::OpenRunTask);
    let _ = app.update(Message::SubmitRunTask);
    assert!(app.run_task_open());
    assert!(app.run_task.error_msg.is_some());

    // 6. Close
    let _ = app.update(Message::CloseRunTask);
    assert!(!app.run_task_open());
}

#[test]
fn test_focus_targets_all_contains_phase4_targets() {
    let all = FocusTarget::ALL;
    assert!(all.contains(&FocusTarget::RunTaskOpen));
    assert!(all.contains(&FocusTarget::RunTaskCommandInput));
    assert!(all.contains(&FocusTarget::RunTaskAdminToggle));
    assert!(all.contains(&FocusTarget::RunTaskSubmit));
    assert!(all.contains(&FocusTarget::RunTaskCancel));
}
