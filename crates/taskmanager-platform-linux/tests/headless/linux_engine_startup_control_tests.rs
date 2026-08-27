use super::*;
use taskmanager_core::core::startup::{
    StartupEntryId, StartupEntryLocator, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason,
};

fn entry(locator: &str, enabled: bool) -> StartupEntry {
    StartupEntry {
        id: StartupEntryId::new("desktop:demo.desktop"),
        name: "Fixture".into(),
        exec: "/usr/bin/fixture".into(),
        enabled,
        source: StartupSource::DesktopEntry,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(locator),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    }
}

fn system_entry(locator: &str, enabled: bool) -> StartupEntry {
    StartupEntry {
        scope: StartupScope::System,
        control_policy: StartupControlPolicy::UserOverride,
        ..entry(locator, enabled)
    }
}

#[cfg(target_family = "unix")]
struct TestDirectory(PathBuf);

#[cfg(target_family = "unix")]
impl TestDirectory {
    fn new(label: &str) -> Self {
        let sequence = OVERRIDE_TEMP_SEQUENCE
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-startup-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create startup fixture directory");
        Self(path)
    }
}

#[cfg(target_family = "unix")]
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn desktop_locator_must_be_a_direct_absolute_child_of_the_user_root() {
    let root = Path::new("/home/<user>/.config/autostart");
    assert_eq!(
        validate_desktop_locator(
            Path::new("/home/<user>/.config/autostart/demo.desktop"),
            root
        ),
        Ok(())
    );
    for invalid in [
        Path::new("/etc/xdg/autostart/demo.desktop"),
        Path::new("/home/<user>/.config/autostart/nested/demo.desktop"),
        Path::new("/home/<user>/.config/autostart/demo.txt"),
        Path::new("demo.desktop"),
    ] {
        assert_eq!(
            validate_desktop_locator(invalid, root),
            Err(ProviderFailure::Rejected)
        );
    }
}

#[test]
fn final_identity_check_rejects_replacement_or_state_drift() {
    let expected = entry("/home/<user>/.config/autostart/demo.desktop", true);
    assert_eq!(
        validate_desktop_identity(
            &expected,
            "[Desktop Entry]\nName=Fixture\nExec=/usr/bin/fixture\n"
        ),
        Ok(())
    );
    assert_eq!(
        validate_desktop_identity(
            &expected,
            "[Desktop Entry]\nName=Replacement\nExec=/usr/bin/fixture\n"
        ),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(
        validate_desktop_identity(
            &expected,
            "[Desktop Entry]\nName=Fixture\nExec=/usr/bin/fixture\nHidden=true\n"
        ),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
fn system_desktop_locator_requires_a_direct_configured_system_child() {
    let roots = vec![
        PathBuf::from("/etc/xdg/autostart"),
        PathBuf::from("/opt/vendor/xdg/autostart"),
    ];
    assert_eq!(
        validate_system_desktop_locator(
            Path::new("/opt/vendor/xdg/autostart/demo.desktop"),
            &roots
        ),
        Ok(())
    );
    for invalid in [
        Path::new("/home/<user>/.config/autostart/demo.desktop"),
        Path::new("/etc/xdg/autostart/nested/demo.desktop"),
        Path::new("/etc/xdg/autostart/demo.txt"),
        Path::new("demo.desktop"),
    ] {
        assert_eq!(
            validate_system_desktop_locator(invalid, &roots),
            Err(ProviderFailure::Rejected)
        );
    }
}

#[test]
fn system_override_identity_is_revalidated_before_copying() {
    let expected = system_entry("/etc/xdg/autostart/demo.desktop", true);
    assert_eq!(
        validate_desktop_identity(
            &expected,
            "[Desktop Entry]\nName=Fixture\nExec=/usr/bin/fixture\n"
        ),
        Ok(())
    );
    assert_eq!(
        validate_desktop_identity(
            &expected,
            "[Desktop Entry]\nName=Fixture\nExec=/usr/bin/replacement\n"
        ),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[cfg(target_family = "unix")]
#[test]
fn user_override_install_is_no_replace_and_leaves_no_temporary_link() {
    let root = TestDirectory::new("override");
    let desktop_id = OsStr::new("demo.desktop");
    let first = "[Desktop Entry]\nName=First\nHidden=true\n";

    install_user_override(&root.0, desktop_id, first).expect("install first override");
    assert_eq!(
        fs::read_to_string(root.0.join(desktop_id)).expect("read installed override"),
        first
    );
    assert_eq!(
        install_user_override(&root.0, desktop_id, "[Desktop Entry]\nName=Replacement\n"),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(
        fs::read_to_string(root.0.join(desktop_id)).expect("read retained override"),
        first
    );
    assert_eq!(
        fs::read_dir(&root.0)
            .expect("list override root")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[cfg(target_family = "unix")]
#[test]
fn current_system_directory_order_selects_the_authoritative_desktop_source() {
    let root = TestDirectory::new("source-order");
    let first = root.0.join("first");
    let second = root.0.join("second");
    fs::create_dir_all(&first).expect("create first system root");
    fs::create_dir_all(&second).expect("create second system root");
    let first_candidate = first.join("demo.desktop");
    let second_candidate = second.join("demo.desktop");
    fs::write(&first_candidate, "first").expect("write first candidate");
    fs::write(&second_candidate, "second").expect("write second candidate");

    let roots = vec![first.clone(), second.clone()];
    let authoritative = resolve_system_desktop_source(&roots, OsStr::new("demo.desktop"))
        .expect("resolve higher-priority candidate");
    assert_eq!(authoritative, first_candidate);
    assert_eq!(
        fs::read_to_string(&authoritative).expect("read authoritative candidate"),
        "first"
    );

    fs::remove_file(&authoritative).expect("remove higher-priority candidate");
    let fallback = resolve_system_desktop_source(&roots, OsStr::new("demo.desktop"))
        .expect("resolve lower-priority candidate");
    assert_eq!(fallback, second_candidate);
    assert_eq!(
        fs::read_to_string(&fallback).expect("read fallback candidate"),
        "second"
    );
}

#[test]
fn hidden_rewrite_is_idempotent_and_reenables_nodisplay() {
    let base = "[Desktop Entry]\nName=A\nExec=a\n";
    let once = rewrite_with_hidden(base, true);
    let twice = rewrite_with_hidden(&once, true);
    assert_eq!(twice.matches("Hidden=true").count(), 1);

    let disabled = "[Desktop Entry]\nName=A\nExec=a\nHidden=true\nNoDisplay=true\n";
    let reenabled = rewrite_with_hidden(disabled, false);
    assert!(!reenabled.contains("NoDisplay"));
    assert!(!reenabled.contains("Hidden"));
    assert!(reenabled.contains("Name=A"));
}
