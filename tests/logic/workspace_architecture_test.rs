//! source-inspection: static-policy
//!
//! Compiler-external dependency rules for the multi-crate architecture: the
//! sanctioned dependency firewall and the escalation-seam policy gates.
//! Positive composition/registration claims are behavior-tested in the crate
//! suites; this file only carries negative/policy guards.

use std::fs;
use std::path::{Path, PathBuf};

#[path = "workspace_architecture_test/dependency_firewall.rs"]
mod dependency_firewall;

#[path = "workspace_architecture_test/escalation_framework.rs"]
mod escalation_framework;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn rust_sources(root: &Path) -> String {
    fn visit(path: &Path, output: &mut String) {
        let entries = fs::read_dir(path).unwrap_or_else(|error| {
            panic!(
                "failed to read architecture path {}: {error}",
                path.display()
            )
        });
        for entry in entries {
            let path = entry.expect("directory entry should be readable").path();
            if path.is_dir() {
                visit(&path, output);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                output.push_str(&fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("failed to read Rust source {}: {error}", path.display())
                }));
                output.push('\n');
            }
        }
    }

    let mut output = String::new();
    visit(root, &mut output);
    output
}
