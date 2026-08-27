use std::path::Path;

use super::*;

#[test]
fn resolved_path_has_a_stable_application_suffix() {
    let path = user_config_path();
    assert!(
        path.ends_with(PathBuf::from("taskmanager").join("config.json"))
            || path == Path::new("taskmanager-config.json")
    );
}

#[test]
fn history_dir_has_a_stable_application_suffix() {
    let dir = user_history_dir();
    assert!(
        dir.ends_with(PathBuf::from("taskmanager").join("history"))
            || dir == Path::new("taskmanager-history")
    );
}
