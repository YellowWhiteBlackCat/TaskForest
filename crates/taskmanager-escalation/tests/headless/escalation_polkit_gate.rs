use super::*;

/// Stage one probe fixture: a `bin/pkexec` executable file, the installed
/// polkit action, and the launcher at its install path, all under a unique
/// temp root. Returns the root (for cleanup) plus the three locations the
/// probe takes.
fn stage_fixture() -> (
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-esc-gate-net-launcher-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    ));
    let pkexec = root.join("bin/pkexec");
    let action = root.join("actions/com.taskforest.net-launcher.policy");
    let helper = root.join("libexec/taskmanager-net-launcher");
    for path in [&pkexec, &action, &helper] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|error| {
                panic!("fixture dir {} should create: {error}", parent.display())
            });
        }
        std::fs::write(path, b"fixture").unwrap_or_else(|error| {
            panic!("fixture file {} should write: {error}", path.display())
        });
    }
    write_policy(&action, &helper);
    use std::os::unix::fs::PermissionsExt;
    for path in [&pkexec, &helper] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap_or_else(
            |error| {
                panic!(
                    "fixture executable {} should chmod: {error}",
                    path.display()
                )
            },
        );
    }
    (root, pkexec, action, helper)
}

fn write_policy(action: &std::path::Path, helper: &std::path::Path) {
    let contents = format!(
        "<policyconfig><action id=\"fixture\"><annotate key=\"org.freedesktop.policykit.exec.path\">{}</annotate></action></policyconfig>",
        helper.display(),
    );
    std::fs::write(action, contents).unwrap_or_else(|error| {
        panic!("fixture policy {} should write: {error}", action.display())
    });
}

/// Best-effort fixture cleanup — a probe test never asserts on the temp
/// tree's lifetime, so leftover fixtures are only disk noise.
fn cleanup(root: std::path::PathBuf) {
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "linux")]
fn fixture_uid(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::MetadataExt;

    std::fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("fixture metadata {} should read: {error}", path.display()))
        .uid()
}

#[cfg(target_os = "linux")]
#[test]
fn net_launcher_probe_all_pieces_present_offers_the_prompt() {
    let (root, pkexec, action, helper) = stage_fixture();
    assert_eq!(
        probe_net_launcher_at(Some(pkexec), &action, &helper),
        EscalationAvailability::RequiresEscalation(EscalationFeature::PerProcessNet),
        "an installed pkexec + action + helper means the prompt is available",
    );
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn installed_crossing_probe_is_feature_exact_and_shared() {
    let (root, pkexec, action, helper) = stage_fixture();
    for feature in [
        EscalationFeature::IntelPmu,
        EscalationFeature::PerProcessNet,
        EscalationFeature::ForeignProcessControl,
    ] {
        assert_eq!(
            probe_installed_crossing_at(
                feature,
                Some(pkexec.clone()),
                &action,
                &helper,
                fixture_uid(&helper),
            ),
            EscalationAvailability::RequiresEscalation(feature),
        );
    }
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn net_launcher_probe_each_missing_piece_is_helper_unavailable() {
    let (root, pkexec, action, helper) = stage_fixture();

    // pkexec missing entirely (not resolvable on PATH).
    assert_eq!(
        probe_net_launcher_at(None, &action, &helper),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
        "no pkexec on PATH -> the prompt cannot be offered",
    );
    // pkexec location points at a nonexistent file.
    let absent_pkexec = root.join("bin/absent-pkexec");
    assert_eq!(
        probe_net_launcher_at(Some(absent_pkexec.clone()), &action, &helper),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
        "a pkexec candidate that is not a file -> HelperUnavailable",
    );
    // The polkit action is missing from the actions dir.
    std::fs::remove_file(&action).unwrap_or_else(|error| {
        panic!("fixture action {} should remove: {error}", action.display())
    });
    assert_eq!(
        probe_net_launcher_at(Some(pkexec.clone()), &action, &helper),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
        "an installed helper without its polkit action is unusable",
    );
    // Restore the action, then remove the helper binary.
    write_policy(&action, &helper);
    std::fs::remove_file(&helper).unwrap_or_else(|error| {
        panic!("fixture helper {} should remove: {error}", helper.display())
    });
    assert_eq!(
        probe_net_launcher_at(Some(pkexec.clone()), &action, &helper),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
        "the launcher binary missing at its annotated path -> HelperUnavailable",
    );
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn installed_crossing_rejects_non_executable_and_symlink_artifacts() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let (root, pkexec, action, helper) = stage_fixture();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o644))
        .unwrap_or_else(|error| panic!("fixture helper should chmod: {error}"));
    assert_eq!(
        probe_installed_crossing_at(
            EscalationFeature::IntelPmu,
            Some(pkexec.clone()),
            &action,
            &helper,
            fixture_uid(&helper),
        ),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
    );

    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("fixture helper should chmod: {error}"));
    let linked_action = root.join("actions/linked.policy");
    symlink(&action, &linked_action)
        .unwrap_or_else(|error| panic!("fixture action symlink should create: {error}"));
    assert_eq!(
        probe_installed_crossing_at(
            EscalationFeature::IntelPmu,
            Some(pkexec),
            &linked_action,
            &helper,
            fixture_uid(&helper),
        ),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
    );
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn installed_crossing_rejects_policy_for_a_different_helper() {
    let (root, pkexec, action, helper) = stage_fixture();
    write_policy(&action, &root.join("libexec/different-helper"));

    assert_eq!(
        probe_installed_crossing_at(
            EscalationFeature::IntelPmu,
            Some(pkexec),
            &action,
            &helper,
            fixture_uid(&helper),
        ),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
    );
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn installed_crossing_rejects_an_untrusted_owner() {
    let (root, pkexec, action, helper) = stage_fixture();
    assert_eq!(
        probe_installed_crossing_at(
            EscalationFeature::IntelPmu,
            Some(pkexec),
            &action,
            &helper,
            fixture_uid(&helper).wrapping_add(1),
        ),
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        },
    );
    cleanup(root);
}

#[cfg(target_os = "linux")]
#[test]
fn net_launcher_probe_never_claims_available_or_a_prompt_denial() {
    // The host in the test lane may have any subset of the three pieces;
    // either way a probe must never fabricate Available access nor report
    // PermissionDenied (probing never asks the user anything).
    let gate = PolkitGate::new();
    match gate.probe(EscalationFeature::PerProcessNet) {
        EscalationAvailability::RequiresEscalation(EscalationFeature::PerProcessNet) => {}
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::HelperUnavailable,
        } => {}
        other => panic!("probe(PerProcessNet) returned an overclaiming {other:?}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn pkexec_in_path_resolves_only_real_file_entries_in_order() {
    // A PATH value containing a pkexec FILE, a pkexec DIRECTORY, and an
    // empty dir: only the file resolves, and an earlier directory entry
    // does not abort the scan.
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-esc-gate-pkexec-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    ));
    let with_file = root.join("with-file");
    let with_dir = root.join("with-dir");
    let empty = root.join("empty");
    std::fs::create_dir_all(&empty)
        .unwrap_or_else(|error| panic!("fixture should create: {error}"));
    std::fs::create_dir_all(with_dir.join("pkexec"))
        .unwrap_or_else(|error| panic!("fixture should create: {error}"));
    std::fs::create_dir_all(&with_file)
        .unwrap_or_else(|error| panic!("fixture should create: {error}"));
    std::fs::write(with_file.join("pkexec"), b"fixture")
        .unwrap_or_else(|error| panic!("fixture should write: {error}"));
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        with_file.join("pkexec"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap_or_else(|error| panic!("fixture should chmod: {error}"));

    // The pkexec DIRECTORY comes first; the scan must continue to the file.
    let path_value = std::env::join_paths([&with_dir, &with_file, &empty]).unwrap_or_default();
    assert_eq!(
        pkexec_in_path(&path_value),
        Some(with_file.join("pkexec")),
        "the first pkexec FILE on PATH resolves, skipping pkexec-named dirs",
    );

    // No file anywhere -> None (honest "no pkexec").
    let dir_only = std::env::join_paths([&with_dir, &empty]).unwrap_or_default();
    assert_eq!(
        pkexec_in_path(&dir_only),
        None,
        "a pkexec DIRECTORY is not an executable candidate",
    );

    let _ = std::fs::remove_dir_all(&root);
}
