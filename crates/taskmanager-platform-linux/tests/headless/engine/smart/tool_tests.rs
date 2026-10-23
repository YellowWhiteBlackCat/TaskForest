//! Behavior tests for the Linux SMART-tool remediation (CORE-54).
//!
//! The contract owns the probe command and the guidance paragraph; the
//! platform owns the install command, and names one only where it can see
//! the package manager itself.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SCRATCH: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(std::path::PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let unique = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let root = crate::test_support::repo_temp_dir().join(format!(
            "taskforest-smart-tool-{label}-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&root).expect("scratch root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn manager_root(scratch: &ScratchDir, relative: &str) {
    let path = scratch.path().join(relative);
    std::fs::create_dir_all(path.parent().expect("manager parent")).expect("manager dir");
    std::fs::write(path, "#!/bin/sh\n").expect("manager stub");
}

#[test]
fn a_host_with_a_package_manager_names_its_install_command() {
    let scratch = ScratchDir::new("apt");
    manager_root(&scratch, "bin/apt-get");
    assert_eq!(
        install_command(scratch.path()),
        Some("sudo apt install smartmontools")
    );
}

#[test]
fn a_host_without_a_known_package_manager_names_no_install_command() {
    let scratch = ScratchDir::new("unknown");
    manager_root(&scratch, "bin/some-other-tool");
    assert_eq!(
        install_command(scratch.path()),
        None,
        "an unknown distribution must not be given an invented package manager"
    );
}

#[test]
fn only_a_missing_tool_carries_a_remediation() {
    assert!(remediation(SmartAvailability::Available, None).is_none());
    assert!(remediation(SmartAvailability::Unsupported, None).is_none());
    assert!(remediation(SmartAvailability::Unavailable, None).is_none());
    assert!(remediation(SmartAvailability::PermissionDenied, None).is_none());
    assert!(
        remediation(SmartAvailability::MissingTool, None).is_some(),
        "a missing tool always carries the contract's remediation"
    );
}

#[test]
fn the_remediation_names_the_contract_probe_and_the_platform_install_command() {
    let remediation = remediation(
        SmartAvailability::MissingTool,
        Some("sudo apt install smartmontools"),
    )
    .expect("a missing tool carries its remediation");
    assert_eq!(remediation.probe_command, "smartctl --scan");
    assert_eq!(
        remediation.install_command.as_deref(),
        Some("sudo apt install smartmontools")
    );
    assert_eq!(
        remediation.guidance.i18n_key(),
        "device.action_missing_tool"
    );
}
