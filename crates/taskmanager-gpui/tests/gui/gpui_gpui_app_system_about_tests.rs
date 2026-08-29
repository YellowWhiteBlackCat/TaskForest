use super::*;
use taskmanager_core::core::hardware::{CoreBreakdown, FirmwareInfo, HostIdentity, KernelInfo};

fn hardware() -> HardwareInfo {
    HardwareInfo::from_fragments(
        HostIdentity {
            os_name: Some("ExampleOS".into()),
            os_version: Some("1.2".into()),
            hostname: Some("host".into()),
            shell: Some("/bin/fish".into()),
            terminal: Some("rio".into()),
            terminal_version: Some("0.5.25".into()),
            locale: Some("zh_CN.UTF-8".into()),
            init_system: Some("systemd".into()),
            package_manager: Some("apt".into()),
            package_manager_version: Some("2.0".into()),
            package_count: Some(1489),
            desktop_environment: Some("KDE Plasma".into()),
            desktop_environment_version: Some("46".into()),
            windowing_system: Some("wayland".into()),
            virtual_terminal: Some("tty2".into()),
            window_manager: Some("KWin".into()),
            window_manager_version: Some("6.7.4".into()),
            compositor_backend: Some("Wayland".into()),
        },
        KernelInfo {
            version: Some("6.1".into()),
            modules_count: Some(12),
            build: Some("builder".into()),
            ..KernelInfo::default()
        },
        taskmanager_core::core::hardware::ComputeTopology {
            cpu_brand: Some("Example CPU".into()),
            logical_cpu_count: Some(8),
            total_memory_mb: Some(16_384),
            core_breakdown: CoreBreakdown {
                p_cores: 4,
                e_cores: 4,
                lp_cores: 0,
            },
            ..taskmanager_core::core::hardware::ComputeTopology::default()
        },
        FirmwareInfo {
            virtualization: Some("KVM".into()),
            ..FirmwareInfo::default()
        },
    )
}

#[test]
fn groups_keep_provider_facts_and_omit_unavailable_rows() {
    let groups = groups(
        &hardware(),
        DesktopAppearance {
            family: DesktopFamily::Gnome,
            color_scheme: PreferredColorScheme::Dark,
            high_contrast: None,
        },
    );
    assert_eq!(groups.len(), 4);
    assert_eq!(groups[0].rows[0].value, "ExampleOS");
    assert_eq!(groups[0].rows[2].value, "apt");
    assert!(
        groups[0]
            .rows
            .iter()
            .any(|row| { row.label_key == "system_about.package_count" && row.value == "1489" })
    );
    assert_eq!(groups[1].rows[0].value, "6.1");
    assert_eq!(groups[1].rows[1].value, "builder");
    assert_eq!(groups[2].rows[0].value, "GNOME");
    assert_eq!(groups[3].rows[1].value, "8 / 8");
    assert!(
        groups[2]
            .rows
            .iter()
            .any(|row| row.label_key == "system_about.windowing_system")
    );
}

#[test]
fn groups_project_connected_display_facts_into_hardware_details() {
    let mut info = hardware();
    info.displays
        .push(taskmanager_core::core::hardware::DisplayInfo {
            connector: "DP-1".into(),
            manufacturer: Some("DEL".into()),
            model: Some("TaskPanel".into()),
            serial: Some("A-42".into()),
            width_mm: Some(600),
            height_mm: Some(340),
            width_px: Some(1920),
            height_px: Some(1080),
            refresh_hz: Some(60.0),
            hdr_supported: Some(true),
        });
    let groups = groups(&info, DesktopAppearance::default());
    let hardware_group = groups
        .iter()
        .find(|group| group.title_key == "system_about.hardware")
        .expect("hardware group");
    let display = hardware_group
        .rows
        .iter()
        .find(|row| row.label_key == "system.display")
        .expect("display row");
    assert!(display.value.contains("DP-1"));
    assert!(display.value.contains("1920×1080"));
    assert!(display.value.contains("S/N A-42"));
}

#[test]
fn copy_all_is_grouped_and_preserves_measured_zero() {
    // copy_all_text begins with the i18n title; pin English so the
    // structural assertions below hold on any host locale, then restore.
    let prior = i18n::current_language();
    i18n::set_language(i18n::Language::En);
    let mut info = hardware();
    info.total_memory_mb = Some(0);
    let groups = groups(&info, DesktopAppearance::default());
    let text = copy_all_text(&groups);
    assert!(text.starts_with("System Information"));
    assert!(text.contains("Operating System Information"));
    assert!(text.contains("Memory: 0 MiB"));
    i18n::set_language(prior);
}

#[test]
fn display_value_keeps_a_prefix_and_ellipsis_for_long_provider_text() {
    let value = "(linux-cachyos@cachyos) (clang version 22.1.8, LLD 22.1.8) #1 SMP";
    let displayed = display_value(value);
    assert!(displayed.starts_with("(linux-cachyos@cachyos)"));
    assert!(displayed.ends_with('…'));
    assert!(displayed.chars().count() <= 36);
}
