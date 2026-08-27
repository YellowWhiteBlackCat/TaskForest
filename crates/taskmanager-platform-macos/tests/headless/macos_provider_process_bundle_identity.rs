use std::path::Path;

use super::{application_identity_observation, bundle_identity_from_path};
use taskmanager_core::ProcessMetadataAvailability;

#[test]
fn canonical_bundle_path_yields_the_bundle_name() {
    let identity =
        bundle_identity_from_path(Path::new("/Applications/Safari.app/Contents/MacOS/Safari"))
            .expect("canonical bundle layout must produce an identity");
    assert_eq!(identity.launcher_id, "Safari.app");
    assert_eq!(identity.display_name, "Safari");
    assert!(!identity.has_icon_token());
}

#[test]
fn display_name_strips_only_the_app_suffix() {
    let identity = bundle_identity_from_path(Path::new(
        "/Applications/My Cool-App 2.app/Contents/MacOS/launcher",
    ))
    .expect("named bundle must produce an identity");
    assert_eq!(identity.launcher_id, "My Cool-App 2.app");
    assert_eq!(identity.display_name, "My Cool-App 2");
}

#[test]
fn helper_below_the_macos_directory_belongs_to_the_outer_bundle() {
    let identity = bundle_identity_from_path(Path::new(
        "/Applications/Foo.app/Contents/MacOS/Helpers/tool",
    ))
    .expect("nested executable stays inside the owning bundle");
    assert_eq!(identity.display_name, "Foo");
}

#[test]
fn innermost_bundle_wins_for_nested_bundles() {
    let identity = bundle_identity_from_path(Path::new(
        "/Applications/Foo.app/Contents/Bundles/Bar.app/Contents/MacOS/Bar",
    ))
    .expect("a nested bundle executable belongs to the inner bundle");
    assert_eq!(identity.launcher_id, "Bar.app");
    assert_eq!(identity.display_name, "Bar");
}

#[test]
fn bundle_layout_matching_is_ascii_case_insensitive() {
    for path in [
        "/Applications/FOO.APP/Contents/MacOS/foo",
        "/Applications/Foo.app/contents/macos/foo",
        "/Volumes/Work/fOO.App/cOnTeNtS/MaCoS/helper",
    ] {
        let identity = bundle_identity_from_path(Path::new(path))
            .unwrap_or_else(|| panic!("{path} must match case-insensitively"));
        assert_eq!(identity.launcher_id.to_lowercase(), "foo.app");
    }
}

#[test]
fn trailing_slashes_and_relative_bundle_paths_still_match() {
    let with_trailing =
        bundle_identity_from_path(Path::new("/Applications/Foo.app/Contents/MacOS/Foo/"))
            .expect("a trailing slash is normalized away by path components");
    assert_eq!(with_trailing.display_name, "Foo");

    let relative = bundle_identity_from_path(Path::new("Foo.app/Contents/MacOS/Foo"))
        .expect("the layout, not the absolute prefix, identifies a bundle");
    assert_eq!(relative.display_name, "Foo");
}

#[test]
fn non_bundle_paths_return_none() {
    let plain = [
        "/usr/bin/top",
        "/Applications/Installer.dmg",
        "/Applications/Foo.app/Contents/Resources/script.sh",
        "/Applications/Foo.app/MacOS/Foo",
        "C:\\Program Files\\app.exe",
        "",
        "/Applications/.app/Contents/MacOS/x",
    ];
    for path in plain {
        assert!(
            bundle_identity_from_path(Path::new(path)).is_none(),
            "{path} must not be classified as an application bundle"
        );
    }
}

#[test]
fn observation_reports_available_for_bundle_executables() {
    let observation = application_identity_observation(
        Some(Path::new("/Applications/Editor.app/Contents/MacOS/Editor")),
        1_000,
    );
    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Available
    );
    assert_eq!(
        observation
            .current_value()
            .map(|id| id.display_name.as_str()),
        Some("Editor")
    );
    assert_eq!(observation.last_success_ms(), Some(1_000));
}

#[test]
fn observation_reports_absent_for_confirmed_non_bundle_executables() {
    let observation = application_identity_observation(Some(Path::new("/usr/sbin/syslogd")), 2_000);
    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Absent
    );
    assert_eq!(observation.current_value(), None);
    assert_eq!(observation.last_success_ms(), Some(2_000));
}

#[test]
fn observation_stays_unknown_when_the_executable_path_is_missing() {
    let observation = application_identity_observation(None, 3_000);
    assert_eq!(
        observation.availability(),
        ProcessMetadataAvailability::Unknown
    );
    assert_eq!(observation.current_value(), None);
    assert_eq!(
        observation.last_success_ms(),
        None,
        "unknown must not fabricate a success timestamp"
    );
}
