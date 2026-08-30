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
            compiler: Some("gcc 14.2.0".into()),
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
            cpu_identity: CpuIdentity::from_cpuid_parts(
                Some("GenuineIntel".into()),
                0x6,
                0x0,
                0x7,
                0xB,
                0x1,
            ),
        },
        FirmwareInfo {
            virtualization: Some("KVM".into()),
            product_name: Some("Machine".into()),
            product_version: Some("v1".into()),
            firmware_vendor: Some("Vendor".into()),
            firmware_version: Some("B1".into()),
            motherboard_vendor: Some("BoardCo".into()),
            motherboard_model: Some("BX-9000".into()),
            motherboard_version: Some("Rev 1.02".into()),
            chipset: Some("Z690 Chipset".into()),
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
    assert_eq!(info.motherboard_version.as_deref(), Some("Rev 1.02"));
    assert_eq!(
        info.chipset.as_deref(),
        Some("Z690 Chipset"),
        "the chipset model must flow losslessly from the firmware fragment"
    );
    assert_eq!(info.kernel_compiler.as_deref(), Some("gcc 14.2.0"));
    assert_eq!(info.secure_boot, Some(true));
    assert_eq!(
        info.instruction_features,
        vec![CpuInstructionFeature::Avx2],
        "instruction features must flow losslessly from the topology fragment"
    );
    assert_eq!(
        info.cpu_identity.vendor_id.as_deref(),
        Some("GenuineIntel"),
        "the CPUID vendor string must flow losslessly from the topology fragment"
    );
    assert_eq!(
        info.cpu_identity.display_model(),
        Some(183),
        "the SDM model combination (7 + B<<4) must survive the fragment fold"
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
fn cpuid_identity_display_combination_follows_the_sdm_rule() {
    // AMD Vermeer (EAX = 0xA20F10): base family 0xF is exhausted, so the
    // display family adds the extension (F + A = 25) and the display model
    // shifts the extended model into place (2<<4 | 1 = 33).
    let vermeer =
        CpuIdentity::from_cpuid_parts(Some("AuthenticAMD".into()), 0xF, 0xA, 0x1, 0x2, 0x0);
    assert_eq!(vermeer.display_family(), Some(25));
    assert_eq!(vermeer.display_model(), Some(33));
    assert_eq!(vermeer.code().as_deref(), Some("25 / 33 / 0"));

    // Intel Raptor Lake (EAX = 0xB0671): base family 6, model 7 extended by
    // ext model B into the familiar 183.
    let raptor_lake =
        CpuIdentity::from_cpuid_parts(Some("GenuineIntel".into()), 0x6, 0x0, 0x7, 0xB, 0x1);
    assert_eq!(raptor_lake.display_family(), Some(6));
    assert_eq!(raptor_lake.display_model(), Some(183));
    assert_eq!(raptor_lake.code().as_deref(), Some("6 / 183 / 1"));

    // A family that is neither 6 nor 0xF never combines: the classic Pentium
    // model stays bare even though an extension field was reported.
    let pentium =
        CpuIdentity::from_cpuid_parts(Some("GenuineIntel".into()), 0x5, 0x0, 0x2, 0x0, 0x7);
    assert_eq!(pentium.display_family(), Some(5));
    assert_eq!(pentium.display_model(), Some(2));

    // An exhausted family without its reported extension is not a displayable
    // number: presenting a bare 15 would misidentify every modern processor.
    let partial = CpuIdentity {
        family: Some(0xF),
        ..CpuIdentity::default()
    };
    assert_eq!(partial.display_family(), None);
    assert_eq!(partial.code(), None);
    assert!(!CpuIdentity::default().is_present());
    assert!(partial.is_present());
}

#[test]
fn legacy_topology_snapshot_without_identity_decodes_to_an_absent_identity() {
    let topology = ComputeTopology {
        cpu_identity: CpuIdentity::from_cpuid_parts(
            Some("AuthenticAMD".into()),
            0xF,
            0xA,
            0x1,
            0x2,
            0x3,
        ),
        ..ComputeTopology::default()
    };
    let json = serde_json::to_value(&topology).expect("topology should serialize");

    // A snapshot written before the identity existed has no key at all; the
    // decoded identity stays absent instead of surfacing fabricated fields.
    let mut legacy = json.clone();
    legacy
        .as_object_mut()
        .expect("topology should serialize as an object")
        .remove("cpu_identity");
    let decoded: ComputeTopology =
        serde_json::from_value(legacy).expect("legacy snapshot should decode");
    assert_eq!(decoded.cpu_identity, CpuIdentity::default());

    let round: ComputeTopology = serde_json::from_value(json).expect("identity should roundtrip");
    assert_eq!(round, topology);
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
