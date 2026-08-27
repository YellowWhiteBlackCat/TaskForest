//! source-inspection: static-policy
//!
//! Long-term dependency boundary for gpui-component (ADR-017).
//!
//! The migration is complete: the crate, its `[patch.crates-io]` entry, and
//! `patches/gpui-component/` are gone, and repository code carries zero
//! `gpui_component` references. This test makes the removal permanent — any
//! new reference (import, inline path, or test) fails CI.
//!
//! Line comments are stripped first, then every identifier reached
//! through `gpui_component::` (module paths, top-level imports, and inline
//! type paths) is collected. The allowlist fixture was deleted with the final
//! reference; the empty inventory is now the only accepted state.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 3] = ["src", "crates", "tests"];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repository())
        .expect("scanned path is inside the repository")
        .to_string_lossy()
        .replace('\\', "/")
}

/// Extract the sorted, de-duplicated `gpui_component::<ref>` identifiers from
/// one source file. Line comments are stripped first so documentation does
/// not trip the gate on incidental prose.
fn references(source: &str) -> Vec<String> {
    let code = source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = Vec::new();
    let mut rest = code.as_str();
    while let Some(position) = rest.find("gpui_component") {
        rest = &rest[position + "gpui_component".len()..];
        let Some(after) = rest.strip_prefix("::") else {
            continue;
        };
        if let Some(brace) = after.strip_prefix('{') {
            let items = brace.split_once('}').map_or("", |(items, _)| items);
            for item in items.split(',') {
                let name = item.split_whitespace().next().unwrap_or("");
                if is_identifier(name) {
                    found.push(name.to_owned());
                }
            }
        } else {
            let name: String = after
                .chars()
                .take_while(|character| is_identifier_char(*character))
                .collect();
            if !name.is_empty() {
                found.push(name);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_identifier_char)
}

/// Inventory of every repository `.rs` file that references
/// `gpui_component`, keyed by slash-relative path.
fn scan() -> BTreeMap<String, Vec<String>> {
    let repository = repository();
    let boundary_test = repository.join("tests/logic/ui_component_boundary.rs");
    let mut inventory = BTreeMap::new();
    let mut pending = SCAN_ROOTS
        .iter()
        .map(|root| repository.join(root))
        .collect::<Vec<_>>();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).unwrap_or_else(|error| {
            panic!(
                "failed to read architecture path {}: {error}",
                directory.display()
            )
        });
        for entry in entries {
            let path = entry.expect("directory entry should be readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && path != boundary_test
            {
                let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("failed to read Rust source {}: {error}", path.display())
                });
                let found = references(&source);
                if !found.is_empty() {
                    inventory.insert(relative(&path), found);
                }
            }
        }
    }
    inventory
}

#[test]
fn gpui_component_is_fully_removed_and_stays_removed() {
    let actual = scan();
    assert!(
        actual.is_empty(),
        "gpui_component references must be zero (ADR-017, P6):\n{}",
        actual
            .iter()
            .map(|(file, refs)| format!("  {file}: {}", refs.join(",")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
