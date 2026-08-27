//! source-inspection: static-policy
//!
//! Architecture guard for the Linux adapter's reduced production surface.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Linux adapter must remain under the workspace crates directory")
        .to_path_buf()
}

#[test]
fn test_support_helpers_do_not_require_crate_wide_lint_suppression() {
    let repository = repository();
    let facade =
        fs::read_to_string(repository.join("crates/taskmanager-platform-linux/src/lib.rs"))
            .expect("Linux facade should be readable");
    let process = fs::read_to_string(
        repository.join("crates/taskmanager-platform-linux/src/engine/process.rs"),
    )
    .expect("Linux process facade should be readable");
    let batch = fs::read_to_string(
        repository.join("crates/taskmanager-platform-linux/src/engine/process/batch.rs"),
    )
    .expect("Linux process batch implementation should be readable");
    let services = fs::read_to_string(
        repository.join("crates/taskmanager-platform-linux/src/engine/services.rs"),
    )
    .expect("Linux service facade should be readable");
    let smart = fs::read_to_string(
        repository.join("crates/taskmanager-platform-linux/src/engine/smart.rs"),
    )
    .expect("Linux SMART facade should be readable");

    for forbidden in [
        "allow(dead_code",
        "allow(unused_imports",
        "cfg_attr(\n    not(any(test, feature = \"test-support\"))",
        "UnsupportedNetworkAccountingBackend",
        "url_encode_query",
    ] {
        assert!(
            !facade.contains(forbidden),
            "Linux facade restored suppressed or retired compatibility surface: {forbidden}"
        );
    }
    assert!(
        facade.contains("#[cfg(feature = \"test-support\")]\npub use engine::process::telemetry")
            && facade.contains("#[cfg(feature = \"test-support\")]\npub use engine::process::{"),
        "low-level Linux compatibility APIs must stay behind test-support"
    );
    assert!(
        batch.contains("#[cfg(feature = \"test-support\")]\npub struct ProcessBatchWorker")
            && process.contains("#[cfg(feature = \"test-support\")]\npub use batch::{"),
        "legacy process worker must not compile into the reduced product surface"
    );
    assert!(
        services.contains("#[cfg(not(feature = \"test-support\"))]\npub(crate) use log_fetch::{")
            && services.contains("#[cfg(feature = \"test-support\")]\npub use log_fetch::{"),
        "service product execution and test facade must have explicit visibility profiles"
    );
    assert!(
        smart.contains("#[cfg(feature = \"test-support\")]\npub fn read_disk_smart(")
            && smart.contains("pub(crate) use provider::SmartProviderRegistry"),
        "legacy SMART entry point must be feature-gated without hiding the product registry"
    );
}
