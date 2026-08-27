use super::*;

#[test]
fn fragment_assembly_is_a_lossless_pure_mapping() {
    let info = HardwareInfo::from_fragments(
        HostIdentity {
            os_name: Some("ExampleOS".into()),
            os_version: Some("1".into()),
            hostname: Some("host".into()),
            shell: Some("/bin/fish".into()),
            terminal: Some("rio".into()),
            terminal_version: Some("0.5.25".into()),
            locale: Some("zh_CN.UTF-8".into()),
            init_system: Some("systemd".into()),
            package_manager: Some("apt".into()),
            package_manager_version: Some("2.0".into()),
            package_count: Some(1489),
            desktop_environment: Some("KDE".into()),
            desktop_environment_version: Some("46".into()),
            windowing_system: Some("wayland".into()),
            virtual_terminal: Some("2".into()),
            window_manager: Some("KWin".into()),
            window_manager_version: Some("6.7.4".into()),
            compositor_backend: Some("Wayland".into()),
        },
        KernelInfo {
            version: Some("6.0".into()),
            modules_count: Some(7),
            command_line: Some("quiet".into()),
            build: Some("build".into()),
        },
        ComputeTopology {
            cpu_brand: Some("CPU".into()),
            logical_cpu_count: Some(8),
            socket_count: Some(2),
            total_memory_mb: Some(16_384),
            core_breakdown: CoreBreakdown {
                p_cores: 4,
                e_cores: 4,
                lp_cores: 0,
            },
            cpu_types: vec![CpuType::Performance; 8],
            base_frequency_mhz: Some(2_400),
            instruction_features: vec![CpuInstructionFeature::Avx2],
        },
        FirmwareInfo {
            virtualization: Some("KVM".into()),
            product_name: Some("Machine".into()),
            product_version: Some("v1".into()),
            firmware_vendor: Some("Vendor".into()),
            firmware_version: Some("B1".into()),
            motherboard_vendor: Some("BoardCo".into()),
            motherboard_model: Some("BX-9000".into()),
            firmware_release_date: Some("08/01/2026".into()),
            secure_boot: Some(true),
        },
    );

    assert_eq!(info.os_name.as_deref(), Some("ExampleOS"));
    assert_eq!(info.kernel_modules_count, Some(7));
    assert_eq!(info.cpu_cores, Some(8));
    assert_eq!(info.package_manager.as_deref(), Some("apt"));
    assert_eq!(info.package_count, Some(1489));
    assert_eq!(info.shell.as_deref(), Some("/bin/fish"));
    assert_eq!(info.terminal.as_deref(), Some("rio"));
    assert_eq!(info.terminal_version.as_deref(), Some("0.5.25"));
    assert_eq!(info.locale.as_deref(), Some("zh_CN.UTF-8"));
    assert_eq!(info.init_system.as_deref(), Some("systemd"));
    assert_eq!(info.desktop_environment.as_deref(), Some("KDE"));
    assert_eq!(info.desktop_environment_version.as_deref(), Some("46"));
    assert_eq!(info.sockets, Some(2));
    assert_eq!(info.product_name.as_deref(), Some("Machine"));
    assert_eq!(info.firmware_vendor.as_deref(), Some("Vendor"));
    assert_eq!(info.firmware_version.as_deref(), Some("B1"));
    assert_eq!(info.architecture.as_deref(), Some(HardwareInfo::HOST_ARCH));
    assert_eq!(info.motherboard_vendor.as_deref(), Some("BoardCo"));
    assert_eq!(info.motherboard_model.as_deref(), Some("BX-9000"));
    assert_eq!(info.firmware_release_date.as_deref(), Some("08/01/2026"));
    assert_eq!(info.secure_boot, Some(true));
    assert_eq!(
        info.instruction_features,
        vec![CpuInstructionFeature::Avx2],
        "instruction features must flow losslessly from the topology fragment"
    );
}

#[test]
fn missing_topology_does_not_fabricate_one_socket_or_performance_cores() {
    let topology = ComputeTopology {
        logical_cpu_count: Some(8),
        cpu_types: vec![CpuType::Unknown; 8],
        ..ComputeTopology::default()
    };
    let info = HardwareInfo::from_fragments(
        HostIdentity::default(),
        KernelInfo::default(),
        topology,
        FirmwareInfo::default(),
    );

    assert_eq!(CpuType::default(), CpuType::Unknown);
    assert_eq!(info.cpu_brand, None);
    assert_eq!(info.cpu_cores, Some(8));
    assert_eq!(info.total_memory_mb, None);
    assert_eq!(info.sockets, None);
    assert_eq!(info.base_freq_mhz, None);
    assert_eq!(info.core_breakdown.total(), 0);
    assert!(info.cpu_types.iter().all(|kind| *kind == CpuType::Unknown));
    assert!(
        info.instruction_features.is_empty(),
        "missing topology must not fabricate instruction features"
    );
}

#[test]
fn legacy_present_hardware_scalars_decode_as_optional_facts() {
    let mut value =
        serde_json::to_value(HardwareInfo::default()).expect("hardware should serialize");
    let fields = value
        .as_object_mut()
        .expect("hardware should serialize as an object");
    fields.insert("os_name".into(), serde_json::json!("ExampleOS"));
    fields.insert("os_version".into(), serde_json::json!("1"));
    fields.insert("kernel_version".into(), serde_json::json!("6.0"));
    fields.insert("hostname".into(), serde_json::json!("host"));
    fields.insert("cpu_brand".into(), serde_json::json!("CPU"));
    fields.insert("cpu_cores".into(), serde_json::json!(8));
    fields.insert("total_memory_mb".into(), serde_json::json!(16_384));
    fields.insert("bios_vendor".into(), serde_json::json!("Vendor"));
    fields.insert("bios_version".into(), serde_json::json!("B1"));

    let decoded: HardwareInfo =
        serde_json::from_value(value).expect("legacy scalar facts should decode");

    assert_eq!(decoded.os_name.as_deref(), Some("ExampleOS"));
    assert_eq!(decoded.kernel_version.as_deref(), Some("6.0"));
    assert_eq!(decoded.cpu_cores, Some(8));
    assert_eq!(decoded.total_memory_mb, Some(16_384));
    assert_eq!(decoded.firmware_vendor.as_deref(), Some("Vendor"));
    assert_eq!(decoded.firmware_version.as_deref(), Some("B1"));

    let encoded = serde_json::to_value(&decoded).expect("hardware should serialize");
    assert_eq!(encoded["bios_vendor"], "Vendor");
    assert_eq!(encoded["bios_version"], "B1");
    assert!(encoded.get("firmware_vendor").is_none());
}

#[test]
fn missing_hardware_scalars_decode_as_unavailable() {
    let mut value =
        serde_json::to_value(HardwareInfo::default()).expect("hardware should serialize");
    let fields = value
        .as_object_mut()
        .expect("hardware should serialize as an object");
    for field in [
        "os_name",
        "os_version",
        "kernel_version",
        "hostname",
        "cpu_brand",
        "cpu_cores",
        "total_memory_mb",
    ] {
        fields.remove(field);
    }

    let decoded: HardwareInfo =
        serde_json::from_value(value).expect("missing optional facts should decode");

    assert_eq!(decoded.os_name, None);
    assert_eq!(decoded.hostname, None);
    assert_eq!(decoded.cpu_brand, None);
    assert_eq!(decoded.cpu_cores, None);
    assert_eq!(decoded.total_memory_mb, None);
}
