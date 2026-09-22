//! test-intest: behavior
//!
//! Headless behavior tests for the System page (`src/pages/system.rs`): what
//! the page states about the host, and how honestly it states it. Pure
//! assertions pin the fact projection; the wired test proves the whole
//! mount→paint→drain loop against the real app composition.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::hardware::HardwareInfo;

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;
use taskmanager_theme::Theme;

use super::{clean_memory_size, content, paint_system, system_fact_rows, system_summary_model};
use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::pages::history::HistoryProjectionResource;
use crate::window::WindowPalette;

/// Hardware fixture: every optional identity fact present, so both the
/// populated rows and the joined-value grammar are exercised for free.
fn fixture_hardware() -> HardwareInfo {
    HardwareInfo {
        hostname: Some("taskforest-workstation".into()),
        os_name: Some("CachyOS".into()),
        os_version: Some("rolling".into()),
        kernel_version: Some("7.2.0".into()),
        kernel_build: Some("ARCH".into()),
        product_name: Some("TiPro 9000".into()),
        firmware_vendor: Some("American Megatrends".into()),
        cpu_brand: Some("Intel Core Ultra 7".into()),
        cpu_cores: Some(22),
        total_memory_mb: Some(32768),
        desktop_environment: Some("KDE".into()),
        windowing_system: Some("wayland".into()),
        window_manager: Some("kwin".into()),
        init_system: Some("systemd".into()),
        package_manager: Some("pacman".into()),
        package_manager_version: Some("7.0".into()),
        shell: Some("/bin/zsh".into()),
        locale: Some("zh_CN.UTF-8".into()),
        ..Default::default()
    }
}

fn shell_with_hardware(hardware: Option<HardwareInfo>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_hardware(&mut shell, |slot| *slot = hardware);
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::System));
    shell
}

#[test]
fn host_facts_project_with_shared_labels_and_honest_dashes() {
    let rows = system_fact_rows(Some(&fixture_hardware()), None, None);
    let value_of = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .map(|row| row.value.clone())
            .unwrap_or_else(|| panic!("the {label} fact must exist"))
    };
    assert_eq!(value_of(t("system.hostname")), "taskforest-workstation");
    assert_eq!(
        value_of(t("system.os")),
        "CachyOS rolling",
        "two-part facts join with one space, never a separator guess"
    );
    assert_eq!(value_of(t("system.field.cpu")), "Intel Core Ultra 7");
    assert_eq!(value_of(t("system.field.cores")), "22");
    assert_eq!(
        value_of(t("system.section.memory")),
        "32.0 GiB",
        "memory size must be cleanly formatted"
    );

    // A platform that supplied NO desktop fact renders the shared dash —
    // never an empty string that looks like a value, never a guess.
    let mut sparse = fixture_hardware();
    sparse.desktop_environment = None;
    sparse.desktop_environment_version = None;
    let sparse_rows = system_fact_rows(Some(&sparse), None, None);
    let desktop = sparse_rows
        .iter()
        .find(|row| row.label == t("system.desktop_environment"))
        .expect("the desktop fact row stays visible");
    assert_eq!(
        desktop.value,
        taskmanager_shell::presentation::MISSING_VALUE,
        "an absent fact is the shared missing value"
    );
}

#[test]
fn a_missing_inventory_states_waiting_and_states_no_facts() {
    let rows = system_fact_rows(None, None, None);
    assert!(rows.is_empty(), "no hardware, no fabricated fact rows");
}

#[test]
fn the_mounted_page_paints_the_host_once_and_survives_refolds() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    let palette = crate::palette::ui_palette(&Theme::dark());
    app.insert_resource(WindowPalette { inner: palette });
    app.insert_non_send(FrontendTrack {
        shell: shell_with_hardware(Some(fixture_hardware())),
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    app.init_resource::<HistoryProjectionResource>();
    // Mount the REAL page scene: the root's on-insert hook binds the paint
    // pass, which authors the body container. The context borrows locals;
    // the shell moves into the track right after the spawn.
    let shell = shell_with_hardware(Some(fixture_hardware()));
    let palette = crate::palette::ui_palette(&Theme::dark());
    let history = HistoryProjectionResource::default();
    let process_tree_expansion = crate::pages::process_tree::ProcessTreeExpansion::default();
    let context = crate::app::PageContext {
        shell: &shell,
        process_tree_expansion: &process_tree_expansion,
        palette: &palette,
        history: &history.0,
    };
    let world = app.world_mut();
    world
        .spawn_scene(content(&context))
        .expect("the system scene resolves without assets");
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    // NO manual paint: the on-insert bind hook must author the body by
    // itself, exactly as the windowed composition does.
    app.update();

    let world = app.world_mut();
    let mut texts = world.query::<&Text>();
    let mut label_seen = 0;
    let mut value_seen = 0;
    for text in texts.iter(world) {
        if text.0 == "Hostname" || text.0 == "主机名" || text.0 == t("system.hostname") {
            label_seen += 1;
        }
        if text.0 == "taskforest-workstation" {
            value_seen += 1;
        }
    }
    assert_eq!(label_seen, 1, "each host fact is stated exactly once");
    assert_eq!(value_seen, 1);

    // A refold (the page's only refresh trigger) repaints in place — the
    // fact is still stated exactly once, never duplicated.
    app.world_mut().trigger(ShellProjectionFolded);
    app.update();
    paint_system(app.world_mut());
    let world = app.world_mut();
    let mut texts = world.query::<&Text>();
    let value_seen = texts
        .iter(world)
        .filter(|text| text.0 == "taskforest-workstation")
        .count();
    assert_eq!(value_seen, 1, "a refold repaints in place, never doubles");

    // The four KPI summary cards (CPU, Memory, Processes, Active Alerts) are painted
    let all_texts: Vec<String> = texts.iter(world).map(|t| t.0.clone()).collect();
    assert!(
        all_texts.iter().any(|s| s == t("common.cpu")),
        "CPU summary card is mounted"
    );
    assert!(
        all_texts.iter().any(|s| s == t("common.memory")),
        "Memory summary card is mounted"
    );
    assert!(
        all_texts.iter().any(|s| s == t("dashboard.processes")),
        "Processes summary card is mounted"
    );
    assert!(
        all_texts.iter().any(|s| s == t("dashboard.active_alerts")),
        "Active Alerts summary card is mounted"
    );
}

#[test]
fn system_fact_rows_include_smbios_slots_and_npu_when_provided() {
    use taskmanager_core::core::metrics::{SmbiosMemorySnapshot, SmbiosModuleRow};
    use taskmanager_core::core::npu::{NpuDevice, NpuInventorySnapshot};

    let smbios = SmbiosMemorySnapshot {
        slots_total: 4,
        slots_used: 2,
        modules: vec![SmbiosModuleRow {
            slot: 0,
            size_mb: Some(16384),
            configured_speed_mts: Some(6000),
            manufacturer: Some("Corsair".into()),
            part_number: Some("CMK32GX5M2B6000C30".into()),
            locator: Some("DIMM_A2".into()),
            memory_type: Some("DDR5".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let npu = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: "npu0".into(),
            brand: Some("Intel AI Boost".into()),
            ..Default::default()
        }],
        1,
    );

    let rows = system_fact_rows(Some(&fixture_hardware()), Some(&smbios), Some(&npu));
    let has_slot_header = rows.iter().any(|r| r.label == t("system.memory_slots"));
    assert!(has_slot_header, "memory slots summary must be present");
    let has_dimm = rows.iter().any(|r| r.label == "DIMM_A2");
    assert!(has_dimm, "DIMM_A2 module must be present");
    let has_npu = rows.iter().any(|r| r.label.contains("npu0"));
    assert!(has_npu, "NPU device must be present");
}

#[test]
fn clean_memory_size_formats_cleanly() {
    assert_eq!(clean_memory_size(32768), "32.0 GiB");
    assert_eq!(clean_memory_size(16384), "16.0 GiB");
    assert_eq!(clean_memory_size(8192), "8.0 GiB");
    assert_eq!(clean_memory_size(1024), "1.0 GiB");
    assert_eq!(clean_memory_size(512), "512.0 MiB");
}

#[test]
fn system_summary_model_mirrors_gpui_and_iced_parity() {
    let projection = taskmanager_shell::SystemProjectionStore::default();
    let model = system_summary_model(&projection);
    assert_eq!(
        model.cpu,
        taskmanager_shell::presentation::MISSING_VALUE,
        "an unobserved CPU renders the dash"
    );
    assert_eq!(
        model.memory,
        taskmanager_shell::presentation::MISSING_VALUE,
        "an unobserved memory renders the dash"
    );
    assert_eq!(model.processes, None, "no inventory means no process count");
    assert_eq!(model.active_alerts, 0, "an empty alert mirror is zero");

    // Live shell from demo fixture
    let demo = crate::demo_fixture::demo_shell();
    let demo_model = system_summary_model(demo.projection());
    assert_ne!(
        demo_model.cpu,
        taskmanager_shell::presentation::MISSING_VALUE,
        "demo shell provides observed CPU percentage"
    );
    assert_ne!(
        demo_model.memory,
        taskmanager_shell::presentation::MISSING_VALUE,
        "demo shell provides observed memory percentage"
    );
}
