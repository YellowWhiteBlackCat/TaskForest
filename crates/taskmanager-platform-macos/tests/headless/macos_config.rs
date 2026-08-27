use std::path::Path;

use super::*;

#[test]
fn resolved_path_has_a_stable_application_suffix() {
    let path = user_config_path();
    assert!(
        path.ends_with(
            PathBuf::from("Application Support")
                .join("TaskForest")
                .join("config.json")
        ) || path == Path::new("taskmanager-config.json")
    );
}
