//! source-inspection: static-policy
//!
//! Static guards for compile-time native OS adapter composition.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn native_selector_physically_selects_linux_windows_or_macos() {
    let source =
        fs::read_to_string(repository().join("crates/taskmanager-platform-native/src/lib.rs"))
            .expect("native selector should be readable");

    for target in ["linux", "windows", "macos"] {
        assert!(
            source.contains(&format!("target_os = \"{target}\"")),
            "native selector omitted {target}"
        );
    }
    for runtime in [
        "taskmanager_platform_linux::NativePlatformRuntime",
        "taskmanager_platform_windows::NativePlatformRuntime",
        "taskmanager_platform_macos::NativePlatformRuntime",
    ] {
        assert!(
            source.contains(runtime),
            "native selector omitted physical runtime selection {runtime}"
        );
    }
    assert!(
        source.contains(
            "#[cfg(not(any(target_os = \"linux\", target_os = \"macos\", target_os = \"windows\")))]"
        ) && source.contains("compile_error!"),
        "unknown operating systems must fail fast instead of selecting a fallback adapter"
    );
}

#[test]
fn transition_adapter_exposes_absence_instead_of_fake_platform_facts() {
    let absent =
        fs::read_to_string(repository().join("crates/taskmanager-platform-runtime/src/absent.rs"))
            .expect("absent runtime is readable");
    // The absence rejection itself is behavior-tested in
    // taskmanager-platform-runtime tests/headless/runtime_absent_tests.rs
    // (absent_handle_rejects_submission_as_unsupported); only the negative
    // fabrication guard belongs here.
    for forbidden in [
        "CapabilityDescriptor",
        "CapabilityStatus::Available",
        "ProviderId",
        "impl RequestPort",
        "SystemSnapshot::default",
        "HardwareInfo::default",
    ] {
        assert!(
            !absent.contains(forbidden),
            "capability-absent composition fabricated provider support via {forbidden}"
        );
    }

    // Windows is the third OS adapter: it composes the complete provider
    // surface through `windows_provider_registry()` (same shape as macOS),
    // with safe-crate implementations and typed unsupported gaps — it must
    // never borrow Linux provider shapes or fabricate data.
    let windows =
        fs::read_to_string(repository().join("crates/taskmanager-platform-windows/src/lib.rs"))
            .expect("Windows adapter should be readable");
    assert!(windows.contains("#![forbid(unsafe_code)]"));
    for forbidden in [
        "taskmanager_platform_linux",
        "taskmanager_telemetry_store",
        "TelemetryStore",
        "spawn_with_telemetry",
        "\"/proc/",
        "\"/sys/",
    ] {
        assert!(
            !windows.contains(forbidden),
            "Windows adapter inherited Linux/provider shape {forbidden}"
        );
    }
}

#[test]
fn windows_child_spawns_never_flash_a_console_window() {
    let command_module =
        fs::read_to_string(repository().join("crates/taskmanager-platform-windows/src/command.rs"))
            .expect("Windows bounded command runner should be readable");
    assert!(
        command_module.contains("CREATE_NO_WINDOW") && command_module.contains("0x0800_0000"),
        "bounded Windows helper spawns must use CREATE_NO_WINDOW"
    );
    let integration = fs::read_to_string(
        repository().join("crates/taskmanager-platform-windows/src/provider/integration.rs"),
    )
    .expect("Windows provider integration should be readable");
    assert!(
        integration.contains("creation_flags(0x0800_0000)"),
        "the Windows command-launch provider must never flash a console window"
    );
}

#[test]
fn linux_hardware_inventory_never_spawns_the_desktop_shell() {
    // Probing `plasmashell --version` / `gnome-shell --version` starts a
    // second instance of the running desktop shell and triggers repeated KDE
    // "cannot write plasmashellrc" dialogs. The desktop-version probe is
    // therefore banned outright; the System page renders the desktop name
    // from the session environment and leaves the version row absent.
    let source = fs::read_to_string(
        repository()
            .join("crates/taskmanager-platform-linux/src/engine/hardware/inventory/system_info.rs"),
    )
    .expect("Linux hardware inventory system_info should be readable");
    for forbidden in ["plasmashell", "gnome-shell", "desktop_version_command"] {
        assert!(
            !source.contains(forbidden),
            "hardware inventory must never spawn the desktop shell via {forbidden}"
        );
    }
}

#[test]
fn macos_second_os_adapter_composes_the_full_surface_without_linux_shapes() {
    // The macOS adapter is the second-OS contract proof: it implements the
    // complete provider SPI and composes the standard product surface through
    // the shared runtime instead of borrowing Linux providers.
    let macos =
        fs::read_to_string(repository().join("crates/taskmanager-platform-macos/src/lib.rs"))
            .expect("macOS adapter should be readable");
    assert!(!macos.contains("capability_absent_handle()"));
    for forbidden in [
        "taskmanager_platform_linux",
        "taskmanager_telemetry_store",
        "TelemetryStore",
        "spawn_with_telemetry",
        "\"/proc/",
        "\"/sys/",
    ] {
        assert!(
            !macos.contains(forbidden),
            "macOS adapter inherited Linux shape {forbidden}"
        );
    }

    let registry =
        fs::read_to_string(repository().join("crates/taskmanager-platform-macos/src/provider.rs"))
            .expect("macOS provider registry should be readable");
    // Every registration carries a macOS-attributed identity; the adapter must
    // never present Linux provider shapes under its own name.
    assert!(!registry.contains("\"linux."));
}

#[test]
fn each_native_adapter_owns_its_configuration_path_convention() {
    let windows =
        fs::read_to_string(repository().join("crates/taskmanager-platform-windows/src/config.rs"))
            .expect("Windows adapter should be readable");
    let macos =
        fs::read_to_string(repository().join("crates/taskmanager-platform-macos/src/config.rs"))
            .expect("macOS adapter should be readable");
    let linux =
        fs::read_to_string(repository().join("crates/taskmanager-platform-linux/src/config.rs"))
            .expect("Linux config selector should be readable");

    assert!(windows.contains("\"APPDATA\""));
    assert!(windows.contains("\"LOCALAPPDATA\""));
    assert!(windows.contains("\"USERPROFILE\""));
    assert!(!windows.contains("\"XDG_CONFIG_HOME\""));
    assert!(macos.contains("\"HOME\""));
    assert!(macos.contains("\"Application Support\""));
    assert!(!macos.contains("\"APPDATA\""));
    assert!(linux.contains("\"XDG_CONFIG_HOME\""));

    let application =
        fs::read_to_string(repository().join("crates/taskmanager-application/src/config_store.rs"))
            .expect("application config store should be readable");
    for forbidden in [
        "XDG_CONFIG_HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "USERPROFILE",
        "cfg(target_os",
    ] {
        assert!(
            !application.contains(forbidden),
            "application selected native config convention {forbidden}"
        );
    }
}

#[test]
fn native_manifest_keeps_os_and_hardware_axes_orthogonal() {
    let manifest =
        fs::read_to_string(repository().join("crates/taskmanager-platform-native/Cargo.toml"))
            .expect("native manifest should be readable");
    let source =
        fs::read_to_string(repository().join("crates/taskmanager-platform-native/src/lib.rs"))
            .expect("native selector should be readable");
    assert!(manifest.contains("[target.'cfg(target_os = \"linux\")'.dependencies]"));
    assert!(manifest.contains("[target.'cfg(target_os = \"macos\")'.dependencies]"));
    assert!(manifest.contains("[target.'cfg(target_os = \"windows\")'.dependencies]"));
    assert!(manifest.contains("default = [\"hardware-all\"]"));
    for adapter_feature in [
        "taskmanager-platform-linux/hardware-all",
        "taskmanager-platform-macos/hardware-all",
        "taskmanager-platform-windows/hardware-all",
    ] {
        assert!(
            manifest.contains(adapter_feature),
            "native standard artifact omitted {adapter_feature}"
        );
    }
    for forbidden in [
        "taskmanager-application",
        "taskmanager-telemetry-store",
        "PlatformHandle",
        "TelemetryStore",
        "std::env",
        "APPDATA",
        "XDG_CONFIG_HOME",
    ] {
        assert!(
            !manifest.contains(forbidden) && !source.contains(forbidden),
            "native selector owns adapter/runtime detail {forbidden}"
        );
    }
    assert!(
        !manifest
            .lines()
            .any(|line| line.trim_start().starts_with("amd =")
                || line.trim_start().starts_with("intel =")),
        "native selector must not expose vendor product SKUs"
    );

    for adapter in [
        "crates/taskmanager-platform-windows/Cargo.toml",
        "crates/taskmanager-platform-macos/Cargo.toml",
    ] {
        let adapter_manifest = fs::read_to_string(repository().join(adapter))
            .unwrap_or_else(|error| panic!("{adapter} should be readable: {error}"));
        assert!(adapter_manifest.contains("default = [\"hardware-all\"]"));
        assert!(adapter_manifest.contains("hardware-all = []"));
        for vendor_feature in ["nvidia =", "amd =", "intel ="] {
            assert!(
                !adapter_manifest
                    .lines()
                    .any(|line| line.trim_start().starts_with(vendor_feature)),
                "{adapter} exposed vendor product feature {vendor_feature}"
            );
        }
    }
}
