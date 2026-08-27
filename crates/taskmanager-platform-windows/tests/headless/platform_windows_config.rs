use std::path::Path;

use super::*;

#[test]
fn resolved_path_has_a_stable_application_suffix() {
    let path = user_config_path();
    assert!(
        path.ends_with(PathBuf::from("TaskForest").join("config.json"))
            || path == Path::new("taskmanager-config.json")
    );
}

#[test]
fn profile_fallback_is_absolute_and_selects_the_requested_appdata_root() {
    // `/profile/demo` is absolute on Unix but root-less on Windows (no
    // drive prefix), so the fixture carries each platform's absolute shape.
    let profile = if cfg!(windows) {
        PathBuf::from(r"C:\profile\demo")
    } else {
        PathBuf::from("/profile/demo")
    };
    let roaming = profile_app_data_directory(profile.clone(), "Roaming");
    assert_eq!(roaming, Some(profile.join("AppData").join("Roaming")));
    assert_eq!(
        profile_app_data_directory(PathBuf::from("relative-profile"), "Local"),
        None
    );
}
