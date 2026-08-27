//! source-inspection: static-policy
//!
//! Negative boundary gate: core must never contain Linux provider
//! implementations or native/vendor policy.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn core_contains_no_linux_provider_implementation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir.join("src");
    let mut sources = Vec::new();
    rust_sources(&source_root, &mut sources);

    let forbidden = [
        "std::process::Command",
        "Command::new(",
        "nix::",
        "rustix::",
        "sysinfo::",
        "#[cfg(target_os = \"linux\")]",
        "#[cfg(unix)]",
    ];
    for path in sources {
        let text = fs::read_to_string(&path).expect("read Rust source");
        for pattern in forbidden {
            assert!(
                !text.contains(pattern),
                "platform provider token {pattern:?} leaked into {}",
                path.display()
            );
        }
    }
}

#[test]
fn linux_provider_entry_points_cannot_return_to_core() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir.join("src");
    let mut sources = Vec::new();
    rust_sources(&source_root, &mut sources);

    let forbidden = [
        "struct MetricsCollector",
        "struct ProcessManager",
        "struct ServiceManager",
        "struct SessionManager",
        "struct StartupManager",
        "fn kill_process(",
        "fn terminate_process(",
        "fn pause_process(",
        "fn resume_process(",
        "fn read_disk_smart(",
        "fn read_nvme_smart(",
        "fn collect_sensor_center(",
        "fn collect_filesystem_health(",
    ];
    for path in sources {
        let text = fs::read_to_string(&path).expect("read Rust source");
        for pattern in forbidden {
            assert!(
                !text.contains(pattern),
                "Linux provider entry point {pattern:?} leaked into {}",
                path.display()
            );
        }
    }
}

#[test]
fn core_manifest_has_no_linux_provider_dependencies_or_features() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read manifest");
    for dependency in ["nix =", "rustix =", "sysinfo =", "nvml-wrapper ="] {
        assert!(
            !manifest.contains(dependency),
            "Linux provider dependency remained in core: {dependency}"
        );
    }
    assert!(
        !manifest.contains("nvidia ="),
        "Linux feature remained in core"
    );
}

#[test]
fn historical_provider_implementation_paths_are_absent() {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core");
    for relative in [
        "bounded_command.rs",
        "collector.rs",
        "collector",
        "hardware/cpu.rs",
        "hardware/disk.rs",
        "hardware/gpu.rs",
        "hardware/network.rs",
        "hardware/platform.rs",
        "process/manager.rs",
        "process/procfs.rs",
        "process/signal.rs",
        "sensors/provider.rs",
        "services/manager.rs",
        "startup/provider.rs",
    ] {
        assert!(
            !core.join(relative).exists(),
            "historical Linux provider implementation remains at {relative}"
        );
    }
}

#[test]
fn system_domain_models_have_no_native_paths_or_vendor_whitelists() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core/metrics/system");
    let mut sources = Vec::new();
    rust_sources(&root, &mut sources);
    let forbidden = [
        "/proc/",
        "/sys/",
        "nvidia",
        "nvml",
        "radeon",
        "amdgpu",
        "intel",
        "drm",
        "hwmon",
        "known_models",
        "supported_models",
        "vendor_id ==",
    ];

    for path in sources {
        let text = fs::read_to_string(&path)
            .expect("read system domain source")
            .to_ascii_lowercase();
        for pattern in forbidden {
            assert!(
                !text.contains(pattern),
                "native or vendor whitelist token {pattern:?} leaked into {}",
                path.display()
            );
        }
    }
}
