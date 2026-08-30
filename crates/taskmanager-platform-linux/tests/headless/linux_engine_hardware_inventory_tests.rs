use std::sync::atomic::{AtomicU64, Ordering};

use super::system_info::{
    detect_package_count_at, detect_package_version_at, normalize_desktop_environment,
    package_manager_candidates, parse_rpm_package_version, parse_version_token,
};
use super::*;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let suffix = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-hardware-inventory-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("fixture directory should be created");
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn context<'a>(probe: &'a SystemProbe, paths: &'a InventoryPaths) -> InventoryContext<'a> {
    InventoryContext {
        system: probe,
        paths,
        virtualization: None,
    }
}

#[test]
fn cpuinfo_x86_flags_line_maps_all_neutral_features_in_canonical_order() {
    let cpuinfo = "processor\t: 0\nflags\t\t: fpu vme sse4_1 sse4_2 avx avx2 avx512f fma aes sha_ni avx_vnni avx512_vnni amx_int8 amx_bf16 hypervisor la57\n";
    let features = compute::parse_cpuinfo_instruction_features(cpuinfo);
    assert_eq!(
        features,
        vec![
            CpuInstructionFeature::Sse41,
            CpuInstructionFeature::Sse42,
            CpuInstructionFeature::Avx,
            CpuInstructionFeature::Avx2,
            CpuInstructionFeature::Avx512F,
            CpuInstructionFeature::Fma3,
            CpuInstructionFeature::AesNi,
            CpuInstructionFeature::ShaNi,
            CpuInstructionFeature::AvxVnni,
            CpuInstructionFeature::Avx512Vnni,
            CpuInstructionFeature::AmxInt8,
            CpuInstructionFeature::AmxBf16,
        ]
    );
}

#[test]
fn cpuinfo_arm_features_line_maps_neon_sve_and_shared_extensions() {
    let cpuinfo =
        "processor\t: 0\nFeatures\t: fp asimd evtstrm aes pmull sha1 sha2 crc32 sve sve2\n";
    let features = compute::parse_cpuinfo_instruction_features(cpuinfo);
    assert_eq!(
        features,
        vec![
            CpuInstructionFeature::AesNi,
            CpuInstructionFeature::Neon,
            CpuInstructionFeature::Sve
        ]
    );
}

#[test]
fn cpuinfo_without_feature_line_is_an_honest_empty_list() {
    assert!(compute::parse_cpuinfo_instruction_features("processor: 0\n").is_empty());
    assert!(compute::parse_cpuinfo_instruction_features("").is_empty());
}

#[test]
fn cpuinfo_repeated_processor_blocks_read_only_the_first_feature_line() {
    let cpuinfo = "processor: 0\nflags: avx2\nprocessor: 1\nflags: avx2 amx_bf16\n";
    let features = compute::parse_cpuinfo_instruction_features(cpuinfo);
    assert_eq!(features, vec![CpuInstructionFeature::Avx2]);
}

#[test]
fn topology_collect_reads_instruction_features_from_proc_root() {
    let fixture = FixtureDir::new();
    fs::write(fixture.0.join("cpuinfo"), "flags: sse4_2 avx2 avx_vnni\n").expect("cpuinfo fixture");
    fs::create_dir_all(fixture.0.join("cpu0")).expect("cpu node");
    let paths = InventoryPaths {
        proc_root: fixture.0.clone(),
        cpu_root: fixture.0.clone(),
        base_frequency: fixture.0.join("missing-base-frequency"),
        dmi_roots: [PathBuf::new(), PathBuf::new()],
        efivars_root: fixture.0.join("efivars"),
        display_root: fixture.0.join("drm"),
        pci_devices_root: fixture.0.join("pci-devices"),
        pci_ids_candidates: [fixture.0.join("pci.ids"), PathBuf::new(), PathBuf::new()],
    };
    let fragment = ComputeTopologySource.collect(&context(&SystemProbe::default(), &paths));
    assert_eq!(
        fragment.value.instruction_features,
        vec![
            CpuInstructionFeature::Sse42,
            CpuInstructionFeature::Avx2,
            CpuInstructionFeature::AvxVnni
        ]
    );
}

#[test]
fn topology_collect_leaves_cpuid_identity_absent_on_fixture_roots() {
    // The CPUID identity is probed only on the native host path: a synthetic
    // fixture root must never leak the test runner's CPU facts into the
    // rendered inventory (same gate as the CPUID frequency fallback).
    let fixture = FixtureDir::new();
    fs::write(fixture.0.join("cpuinfo"), "flags: avx2\n").expect("cpuinfo fixture");
    fs::create_dir_all(fixture.0.join("cpu0")).expect("cpu node");
    let paths = InventoryPaths {
        proc_root: fixture.0.clone(),
        cpu_root: fixture.0.clone(),
        base_frequency: fixture.0.join("missing-base-frequency"),
        dmi_roots: [PathBuf::new(), PathBuf::new()],
        efivars_root: fixture.0.join("efivars"),
        display_root: fixture.0.join("drm"),
        pci_devices_root: fixture.0.join("pci-devices"),
        pci_ids_candidates: [fixture.0.join("pci.ids"), PathBuf::new(), PathBuf::new()],
    };
    let fragment = ComputeTopologySource.collect(&context(&SystemProbe::default(), &paths));
    assert_eq!(fragment.value.cpu_identity, CpuIdentity::default());
    assert!(!fragment.value.cpu_identity.is_present());
}

#[test]
fn unavailable_system_identity_does_not_fabricate_fallback_strings() {
    let paths = InventoryPaths {
        proc_root: PathBuf::from("/definitely-missing-taskmanager-proc"),
        ..InventoryPaths::default()
    };
    let fragment = SystemIdentitySource.collect(&context(&SystemProbe::default(), &paths));

    assert_eq!(fragment.value.os_name, None);
    assert_eq!(fragment.value.os_version, None);
    assert_eq!(fragment.value.hostname, None);
    assert_eq!(
        fragment.status.outcome,
        SourceOutcome::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(fragment.status.item_count, 0);
}

#[test]
fn complete_system_identity_counts_the_current_eighteen_fact_schema() {
    let fixture = FixtureDir::new();
    fs::create_dir(fixture.0.join("1")).expect("pid 1 fixture should be created");
    fs::write(fixture.0.join("1/comm"), "systemd\n").expect("init fixture should be written");
    let probe = SystemProbe {
        os_name: Some("Linux".into()),
        os_version: Some("fixture".into()),
        hostname: Some("host".into()),
        shell: Some("/bin/fish".into()),
        terminal: Some("rio".into()),
        terminal_version: Some("0.5.25".into()),
        locale: Some("zh_CN.UTF-8".into()),
        package_manager: Some("apt".into()),
        package_manager_version: Some("2.0".into()),
        package_count: Some(1489),
        desktop_environment: Some("KDE".into()),
        desktop_environment_version: Some("6".into()),
        windowing_system: Some("wayland".into()),
        virtual_terminal: Some("tty2".into()),
        window_manager: Some("KWin".into()),
        window_manager_version: Some("6".into()),
        compositor_backend: Some("Wayland".into()),
        ..Default::default()
    };
    let paths = InventoryPaths {
        proc_root: fixture.0.clone(),
        ..InventoryPaths::default()
    };
    let fragment = SystemIdentitySource.collect(&context(&probe, &paths));

    assert_eq!(fragment.status.outcome, SourceOutcome::Available);
    assert_eq!(fragment.status.item_count, 18);
}

#[test]
fn package_database_counts_are_bounded_and_manager_specific() {
    let fixture = FixtureDir::new();
    let pacman = fixture.0.join("pacman");
    fs::create_dir_all(pacman.join("alpha-1")).expect("pacman package directory");
    fs::create_dir_all(pacman.join("beta-2")).expect("pacman package directory");
    fs::write(fixture.0.join("status"), "Package: one\nStatus: install ok installed\n\nPackage: two\nStatus: deinstall ok config-files\n")
        .expect("dpkg fixture");
    fs::write(fixture.0.join("apk"), "P:one\nV:1\n\nP:two\nV:2\n").expect("apk fixture");

    assert_eq!(
        detect_package_count_at(
            Some("pacman"),
            &pacman,
            &fixture.0.join("status"),
            &fixture.0.join("apk"),
        ),
        Some(2)
    );
    assert_eq!(
        detect_package_count_at(
            Some("apt"),
            &pacman,
            &fixture.0.join("status"),
            &fixture.0.join("apk"),
        ),
        Some(1)
    );
    assert_eq!(
        detect_package_count_at(
            Some("apk"),
            &pacman,
            &fixture.0.join("status"),
            &fixture.0.join("apk"),
        ),
        Some(2)
    );
    assert_eq!(
        detect_package_count_at(
            Some("rpm"),
            &pacman,
            &fixture.0.join("status"),
            &fixture.0.join("apk"),
        ),
        None
    );
}

#[test]
fn package_manager_version_parser_keeps_the_first_real_version_token() {
    assert_eq!(
        parse_version_token("apt 2.6.1 (amd64)\n"),
        Some("2.6.1".to_owned())
    );
    assert_eq!(
        parse_version_token("Pacman v6.0.2 - libalpm v13.0.2\n"),
        Some("6.0.2".to_owned())
    );
    assert_eq!(parse_version_token("no version here"), None);
}

#[test]
fn package_database_version_lookup_skips_database_files_and_reads_named_packages() {
    let fixture = FixtureDir::new();
    let pacman = fixture.0.join("pacman");
    fs::create_dir_all(&pacman).expect("pacman database should be created");
    fs::write(pacman.join("ALPM_DB_VERSION"), "9\n").expect("database marker");
    fs::create_dir_all(pacman.join("unrelated-1")).expect("unrelated package directory");
    fs::create_dir_all(pacman.join("plasma-desktop-6.7.4-1")).expect("target package directory");
    fs::write(
        pacman.join("plasma-desktop-6.7.4-1/desc"),
        "%NAME%\nplasma-desktop\n\n%VERSION%\n6.7.4-1\n",
    )
    .expect("pacman package metadata");
    let dpkg = fixture.0.join("status");
    fs::write(
        &dpkg,
        "Package: kwin\nStatus: install ok installed\nVersion: 6.7.4-2\n",
    )
    .expect("dpkg package metadata");
    let apk = fixture.0.join("apk");
    fs::write(&apk, "P:plasma-desktop\nV:6.7.4-r0\n\n").expect("apk package metadata");

    assert_eq!(
        detect_package_version_at(Some("pacman"), &pacman, &dpkg, &apk, &["plasma-desktop"],),
        Some("6.7.4-1".to_owned())
    );
    assert_eq!(
        detect_package_version_at(Some("apt"), &pacman, &dpkg, &apk, &["kwin"]),
        Some("6.7.4-2".to_owned())
    );
    assert_eq!(
        detect_package_version_at(Some("apk"), &pacman, &dpkg, &apk, &["plasma-desktop"]),
        Some("6.7.4-r0".to_owned())
    );
}

#[test]
fn rpm_query_parser_accepts_one_bounded_version_and_rejects_noise() {
    assert_eq!(
        parse_rpm_package_version("6.7.4-7.1\n"),
        Some("6.7.4-7.1".to_owned())
    );
    assert_eq!(
        parse_rpm_package_version("package is not installed\n"),
        None
    );
    assert_eq!(parse_rpm_package_version("6.7.4\nextra\n"), None);
}

#[test]
fn package_manager_candidates_follow_distribution_without_claiming_unknowns() {
    assert_eq!(package_manager_candidates("ubuntu")[0].0, "apt");
    assert_eq!(package_manager_candidates("fedora")[0].0, "dnf");
    assert_eq!(package_manager_candidates("arch")[0].0, "pacman");
    assert_eq!(
        package_manager_candidates("unknown-distribution")[0].0,
        "apt"
    );
}

#[test]
fn session_facts_normalize_only_confirmed_virtual_terminal_shapes() {
    assert_eq!(
        normalize_virtual_terminal("2".to_owned()),
        Some("tty2".to_owned())
    );
    assert_eq!(
        normalize_virtual_terminal("tty7".to_owned()),
        Some("tty7".to_owned())
    );
    assert_eq!(normalize_virtual_terminal("desktop".to_owned()), None);
}

#[test]
fn desktop_session_token_is_read_without_launching_the_shell() {
    assert_eq!(
        normalize_desktop_environment("KDE:GNOME".to_owned()),
        Some("KDE".to_owned())
    );
    assert_eq!(
        normalize_desktop_environment("  GNOME  ".to_owned()),
        Some("GNOME".to_owned())
    );
    assert_eq!(normalize_desktop_environment(" : ".to_owned()), None);
}

#[test]
fn topology_uses_enumerated_cpu_count_without_fabricating_other_facts() {
    let fixture = FixtureDir::new();
    fs::create_dir(fixture.0.join("cpu0")).expect("cpu0 fixture should be created");
    fs::create_dir(fixture.0.join("cpu1")).expect("cpu1 fixture should be created");
    fs::create_dir(fixture.0.join("cpufreq")).expect("non-CPU fixture should be created");
    let paths = InventoryPaths {
        cpu_root: fixture.0.clone(),
        base_frequency: fixture.0.join("missing-base-frequency"),
        ..InventoryPaths::default()
    };

    let fragment = ComputeTopologySource.collect(&context(&SystemProbe::default(), &paths));

    assert_eq!(fragment.value.logical_cpu_count, Some(2));
    assert_eq!(fragment.value.cpu_brand, None);
    assert_eq!(fragment.value.total_memory_mb, None);
    assert_eq!(fragment.value.cpu_types, vec![CpuType::Unknown; 2]);
}

#[test]
fn kernel_source_reports_real_file_success_and_missing_root() {
    let fixture = FixtureDir::new();
    fs::write(fixture.0.join("modules"), "a 1 0\nb 1 0\n")
        .expect("modules fixture should be written");
    fs::write(fixture.0.join("cmdline"), "quiet").expect("cmdline fixture should be written");
    fs::write(
            fixture.0.join("version"),
            "Linux version 6.0 (builder@host) (gcc 14.2) #1 SMP PREEMPT_DYNAMIC Mon Jan 1 12:00:00 2026",
        )
        .expect("version fixture should be written");
    let probe = SystemProbe {
        kernel_version: Some("6.0".into()),
        ..SystemProbe::default()
    };
    let mut paths = InventoryPaths {
        proc_root: fixture.0.clone(),
        ..InventoryPaths::default()
    };

    let available = KernelSource.collect(&context(&probe, &paths));
    assert_eq!(available.status.outcome, SourceOutcome::Available);
    assert_eq!(available.value.modules_count, Some(2));
    assert_eq!(
        available.value.build.as_deref(),
        Some("(builder@host) (gcc 14.2) #1 SMP PREEMPT_DYNAMIC Mon Jan 1 12:00:00 2026")
    );

    paths.proc_root = fixture.0.join("missing");
    let unavailable = KernelSource.collect(&context(&SystemProbe::default(), &paths));
    assert_eq!(
        unavailable.status.outcome,
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn linux_kernel_build_parser_removes_release_and_preserves_build_description() {
    let raw = "Linux version 6.1.5-cachyos (user@host) (gcc 13.0) #1 SMP PREEMPT_DYNAMIC Mon Jan 1 12:00:00 2026";

    assert_eq!(
        parse_linux_kernel_build_description(raw).as_deref(),
        Some("(user@host) (gcc 13.0) #1 SMP PREEMPT_DYNAMIC Mon Jan 1 12:00:00 2026")
    );
}

#[test]
fn linux_kernel_compiler_parser_extracts_name_and_version_only() {
    // gcc with the parenthesized qualifier and a trailing date: only the
    // toolchain identity survives.
    let raw = "Linux version 6.1.5 (user@host) (gcc (GCC) 13.2.1 20240614) #1 SMP";
    assert_eq!(parse_linux_kernel_compiler(raw), "gcc 13.2.1");
    // Plain gcc spelling.
    let raw = "Linux version 6.6 (build@farm) (gcc 12.3.0) #1";
    assert_eq!(parse_linux_kernel_compiler(raw), "gcc 12.3.0");
    // clang spells the version with an explicit `version` word.
    let raw = "Linux version 6.12 (user@host) (clang version 18.1.8) #1";
    assert_eq!(parse_linux_kernel_compiler(raw), "clang version 18.1.8");
    // No recognizable toolchain token: an honest empty absence.
    assert_eq!(parse_linux_kernel_compiler("Linux version 6.1 #1"), "");
    // A bare compiler name without a following version is not an identity.
    assert_eq!(
        parse_linux_kernel_compiler("Linux version 6.1 (gcc) #1"),
        ""
    );
}

#[test]
fn linux_kernel_build_parser_rejects_raw_records_without_a_build_tail() {
    assert_eq!(
        parse_linux_kernel_build_description("not a version line"),
        None
    );
    assert_eq!(parse_linux_kernel_build_description(""), None);
    assert_eq!(
        parse_linux_kernel_build_description("Linux version 6.1.0"),
        None
    );
    assert_eq!(
        parse_linux_kernel_build_description("Linux version 6.1.0   "),
        None
    );
}

#[test]
fn malformed_linux_kernel_record_is_a_typed_partial_without_raw_build_text() {
    let fixture = FixtureDir::new();
    fs::write(fixture.0.join("modules"), "a 1 0\n").expect("modules fixture should be written");
    fs::write(fixture.0.join("cmdline"), "quiet").expect("cmdline fixture should be written");
    fs::write(fixture.0.join("version"), "unexpected provider record")
        .expect("version fixture should be written");
    let paths = InventoryPaths {
        proc_root: fixture.0.clone(),
        ..InventoryPaths::default()
    };
    let probe = SystemProbe {
        kernel_version: Some("6.0".into()),
        ..SystemProbe::default()
    };

    let fragment = KernelSource.collect(&context(&probe, &paths));

    assert_eq!(
        fragment.status.outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(fragment.status.item_count, 3);
    assert_eq!(fragment.value.build, None);
}

#[test]
fn readable_but_empty_firmware_root_is_authoritative_empty() {
    let fixture = FixtureDir::new();
    let paths = InventoryPaths {
        dmi_roots: [fixture.0.clone(), fixture.0.join("fallback")],
        efivars_root: fixture.0.join("efivars"),
        // The PCI sysfs tree and pci.ids database are injected too, so this
        // fixture never reads the host's chipset identity.
        pci_devices_root: fixture.0.join("pci-devices"),
        pci_ids_candidates: [fixture.0.join("pci.ids"), PathBuf::new(), PathBuf::new()],
        ..InventoryPaths::default()
    };

    let fragment = FirmwareSource.collect(&context(&SystemProbe::default(), &paths));
    assert_eq!(fragment.status.outcome, SourceOutcome::Empty);
    assert_eq!(fragment.status.item_count, 0);
    assert_eq!(fragment.value.chipset, None);
}

#[test]
fn io_failure_classification_is_typed() {
    assert_eq!(
        classify_io_failure(&io::Error::from(io::ErrorKind::PermissionDenied)),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        classify_io_failure(&io::Error::from(io::ErrorKind::NotFound)),
        FailureKind::Unsupported
    );
}
