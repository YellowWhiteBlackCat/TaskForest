//! Deterministic metadata for the optional native setup review.

use std::path::PathBuf;
use taskmanager_core::core::setup::SetupScriptInfo;

/// Synthetic descriptor of the fixed optional setup asset; demo never executes it.
pub fn setup_script_info() -> SetupScriptInfo {
    SetupScriptInfo {
        path: PathBuf::from("/usr/share/taskforest/setup/99-taskforest.rules"),
        run_command: "pkexec /usr/libexec/taskforest-setup-helper install".into(),
        revert_command: "pkexec /usr/libexec/taskforest-setup-helper revert".into(),
    }
}
