use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SCRATCH: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(std::path::PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let unique = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let root = crate::test_support::repo_temp_dir().join(format!(
            "taskforest-npu-{label}-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&root).expect("scratch root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn write_standard_intel_shape(root: &Path) {
    let node = root.join("accel0");
    fs::create_dir_all(node.join("device")).expect("device dir");
    fs::write(node.join("device/vendor"), "0x8086\n").expect("vendor");
    fs::write(node.join("device/device"), "0x27fa\n").expect("device id");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../pci_driver", node.join("device/driver")).expect("driver");
}

#[test]
fn full_sysfs_shape_reports_driver_and_typed_unsupported_utilization() {
    let scratch = ScratchDir::new("full");
    write_standard_intel_shape(scratch.path());
    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.devices.len(), 1);
    let device = &snapshot.devices[0];
    let expected_identity = format!(
        "linux:npu:sysfs:{}",
        fs::canonicalize(scratch.path().join("accel0/device"))
            .expect("canonical physical fixture")
            .to_string_lossy()
    );
    assert_eq!(device.device_id.as_str(), expected_identity);
    assert_eq!(device.driver.as_deref(), Some("pci_driver"));
    assert_eq!(device.brand, None);
    assert_eq!(
        device.utilization_pct.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn unbound_driver_shape_keeps_driver_none_without_failing() {
    let scratch = ScratchDir::new("unbound");
    fs::create_dir_all(scratch.path().join("accel3")).expect("node dir");
    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert!(snapshot.is_success());
    assert_eq!(snapshot.devices.len(), 1);
    assert!(
        snapshot.devices[0]
            .device_id
            .as_str()
            .starts_with("linux:npu:sysfs:")
    );
    assert_ne!(snapshot.devices[0].device_id.as_str(), "accel3");
    assert_eq!(snapshot.devices[0].driver, None);
}

#[test]
fn missing_accel_class_is_the_honest_empty_host() {
    let scratch = ScratchDir::new("empty-root");
    let root = scratch.path().join("does-not-exist");
    let snapshot = discover_accelerators(&root, 42).expect("missing class is a success");
    assert!(snapshot.is_success());
    assert!(snapshot.devices.is_empty());
    assert_eq!(snapshot.failure, None);
}

#[test]
fn non_accel_class_members_are_skipped_not_errors() {
    let scratch = ScratchDir::new("mixed");
    fs::create_dir_all(scratch.path().join("accel0")).expect("accel node");
    fs::create_dir_all(scratch.path().join("watchdog0")).expect("watchdog node");
    fs::create_dir_all(scratch.path().join("accelerometer1")).expect("unrelated node");
    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    let ids: Vec<&str> = snapshot
        .devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect();
    assert_eq!(ids.len(), 1);
    assert!(ids[0].starts_with("linux:npu:sysfs:"));
    assert_ne!(ids[0], "accel0");
}

#[cfg(unix)]
#[test]
fn unreadable_class_dir_is_a_typed_permission_failure() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = ScratchDir::new("denied");
    let root = scratch.path().join("accel");
    fs::create_dir_all(&root).expect("class dir");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).expect("drop read bit");
    // Only meaningful when the test runner is not root (root bypasses DAC).
    if !nix::unistd::Uid::effective().is_root() {
        let failure = discover_accelerators(&root, 42).expect_err("denied dir must fail typed");
        assert_eq!(failure.kind(), FailureKind::PermissionDenied);
    }
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("restore");
}

#[test]
fn enumeration_is_bounded_and_sorted() {
    let scratch = ScratchDir::new("bounded");
    for index in 0..(MAX_ACCEL_NODES + 4) {
        fs::create_dir_all(scratch.path().join(format!("accel{index}"))).expect("node dir");
    }
    let snapshot = discover_accelerators(scratch.path(), 42).expect("discovery succeeds");
    assert_eq!(snapshot.devices.len(), MAX_ACCEL_NODES);
    let mut sorted: Vec<&str> = snapshot
        .devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect();
    sorted.sort_unstable();
    let reported: Vec<&str> = snapshot
        .devices
        .iter()
        .map(|device| device.device_id.as_str())
        .collect();
    assert_eq!(reported, sorted, "discovered() must return devices sorted");
}
