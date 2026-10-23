//! Behavior tests for the SMART-tool remediation projection (CORE-54).

use crate::presentation::smart_tool;
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability, SmartToolRemediation};

fn disk_with_smart(availability: SmartAvailability) -> DiskMetrics {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.smart_availability = availability;
    disk
}

#[test]
fn a_missing_tool_projects_the_shared_probe_and_guidance() {
    let disk = disk_with_smart(SmartAvailability::MissingTool);
    let guidance =
        smart_tool::smart_tool_guidance(&disk).expect("a missing tool projects its remediation");
    assert_eq!(guidance.probe_command, "smartctl --scan");
    assert_eq!(guidance.guidance_key, "device.action_missing_tool");
    assert_eq!(guidance.title_key, "disk.smart_missing_tool");
    assert_eq!(guidance.install_command, None);
}

#[test]
fn a_present_tool_projects_no_remediation() {
    let disk = disk_with_smart(SmartAvailability::Available);
    assert!(smart_tool::smart_tool_guidance(&disk).is_none());
}

#[test]
fn the_platform_install_command_reaches_the_projection() {
    let mut disk = disk_with_smart(SmartAvailability::MissingTool);
    disk.smart_tool_remediation = Some(
        SmartToolRemediation::platform_neutral()
            .with_install_command("sudo apt install smartmontools"),
    );
    let guidance =
        smart_tool::smart_tool_guidance(&disk).expect("a missing tool projects its remediation");
    assert_eq!(
        guidance.install_command.as_deref(),
        Some("sudo apt install smartmontools")
    );
}
