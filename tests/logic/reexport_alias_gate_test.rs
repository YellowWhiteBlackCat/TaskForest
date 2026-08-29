//! source-inspection: static-policy
//!
//! Negative architecture gate: no renamed `pub use` re-exports and no
//! cross-crate `pub use` forwarding facades in production code.
//!
//! A `pub use path::Name as OtherName;` puts one symbol on the public API
//! under two different names (the origin path and the alias), which is the
//! ambiguity hazard this gate exists to prevent. Aggregate facades may still
//! re-export a coherent module surface with `pub use module::*`; only renamed
//! bindings are forbidden. The parser-level check below keeps its scope narrow
//! because the workspace-wide guard owns the complete import policy.
//!
//! * Re-export under the ORIGINAL name (`pub use path::relative_luminance;`)
//!   — the single-source pattern used across `taskmanager-theme`.
//!
//! The workspace-wide `scripts/quality/rust_surface_guard.py` rejects private
//! import aliases and the anonymous `as _` form as well. This Rust test keeps
//! the parser-level public-facade regression check close to the logic test
//! target; neither check carries a per-file exemption.
//!
//! The second test enforces AGENTS.md's architecture rule: a `pub use` whose
//! first path segment names another crate is a cross-crate forwarding facade —
//! a second public address for a type the owner crate already exports.
//! Consumers must import the owner module (`taskmanager-core`,
//! `taskmanager-platform-contract`, `taskmanager-theme`, …) directly. The one
//! sanctioned exception is `taskmanager-platform-native`, whose whole purpose
//! is `cfg`-selecting the OS adapter behind one composition name; that is
//! adapter selection, not a shared-type second address.

use std::fs;
use std::path::{Path, PathBuf};

/// Root of the repository relative to the test working directory.
const REPO_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[test]
fn production_code_has_no_renamed_pub_use_reexports() {
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let crates_dir = Path::new(REPO_ROOT).join("crates");
    let src_dir = Path::new(REPO_ROOT).join("src");
    for root in [&crates_dir, &src_dir] {
        if !root.is_dir() {
            continue;
        }
        for path in rust_files(root) {
            let relative = path
                .strip_prefix(REPO_ROOT)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            scanned += 1;
            let text = fs::read_to_string(&path).unwrap_or_default();
            for alias in renamed_pub_use_aliases(&text) {
                offenders.push(format!("{relative}: {alias}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "renamed `pub use ... as ...` re-exports found (each puts one symbol \
         on the public API under two names):\n  {}\nRe-export under the \
         original name or use a fully qualified path; aggregate facades may \
         continue to use a wildcard re-export.",
        offenders.join("\n  ")
    );
    // The gate must actually see the tree; a silently-empty scan would be a
    // vacuous pass.
    assert!(
        scanned > 100,
        "gate scanned only {scanned} production files; the tree moved"
    );
}

#[test]
fn production_code_has_no_cross_crate_pub_use_forwards() {
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let crates_dir = Path::new(REPO_ROOT).join("crates");
    let crate_roots: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    for crate_root in &crate_roots {
        // platform-native is the sanctioned cfg adapter-composition route
        // (AGENTS.md): its re-exports select an OS adapter behind one name.
        if crate_root.file_name().and_then(|n| n.to_str()) == Some("taskmanager-platform-native") {
            continue;
        }
        let local_modules = declared_module_names(crate_root);
        let src_files = rust_files(&crate_root.join("src"));
        scanned += src_files.len();
        for path in src_files {
            let relative = path
                .strip_prefix(REPO_ROOT)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let text = fs::read_to_string(&path).unwrap_or_default();
            for statement in pub_use_statements(&text) {
                let Some(head) = statement.split("::").next() else {
                    continue;
                };
                let head = head.trim();
                if matches!(head, "crate" | "self" | "super")
                    || local_modules.iter().any(|m| m == head)
                {
                    continue;
                }
                offenders.push(format!("{relative}: pub use {statement}…"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "cross-crate `pub use` forwarding facades found (each is a second public \
         address for another crate's item; import the owner module directly):\n  {}",
        offenders.join("\n  ")
    );
    // The gate must actually see the tree; a silently-empty scan would be a
    // vacuous pass.
    assert!(
        scanned > 100,
        "gate scanned only {scanned} production files; the tree moved"
    );
}

/// All module names declared anywhere in the crate (`mod x;` or `pub mod x;`).
/// Over-approximates the per-file module scope on purpose: a local module
/// name never collides with a workspace crate name, so an over-permissive
/// allowlist cannot hide a real facade.
fn declared_module_names(crate_root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for path in rust_files(&crate_root.join("src")) {
        let text = fs::read_to_string(&path).unwrap_or_default();
        for line in text.lines() {
            let trimmed = line.trim_start();
            let rest = trimmed
                .strip_prefix("pub ")
                .or_else(|| trimmed.strip_prefix("pub(crate) "))
                .unwrap_or(trimmed);
            if let Some(rest) = rest.strip_prefix("mod ") {
                // both `mod x;` declarations and inline `mod x { … }` bodies
                // name a crate-local module
                let name = rest
                    .split([';', '{'])
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .trim_end()
                    .trim_end_matches('{')
                    .trim();
                if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names
}

/// Full `pub use …;` statement bodies (path part only, braces joined).
fn pub_use_statements(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if let Some(rest) = trimmed.strip_prefix("pub use ") {
            let mut statement = rest.to_string();
            while !statement.contains(';') && index + 1 < lines.len() {
                index += 1;
                statement.push(' ');
                statement.push_str(lines[index].trim());
            }
            let statement = statement.split(';').next().unwrap_or(&statement);
            found.push(statement.to_owned());
        }
        index += 1;
    }
    found
}

/// Collect `X as Y` (Y != `_`) bindings inside truly public `pub use`
/// statements, joining multi-line brace imports. `pub(crate)`/`pub(super)`
/// and private `use` renames stay inside one crate (module-local
/// disambiguation) and are out of scope. Returns human-readable
/// `original as alias` fragments with their line numbers.
fn renamed_pub_use_aliases(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("pub use ") {
            let start_line = index + 1;
            let mut statement = rest.to_string();
            while !statement.contains(';') && index + 1 < lines.len() {
                index += 1;
                statement.push(' ');
                statement.push_str(lines[index].trim());
            }
            for (original, alias) in alias_bindings(&statement) {
                found.push(format!("line {start_line}: {original} as {alias}"));
            }
        }
        index += 1;
    }
    found
}

/// Extract `name as other` pairs from one use-statement body, skipping the
/// anonymous `as _` form (no new name is introduced).
fn alias_bindings(statement: &str) -> Vec<(&str, &str)> {
    let mut bindings = Vec::new();
    for token in statement.split(',') {
        let token = token.trim().trim_end_matches(';').trim();
        if let Some((original, alias)) = token.split_once(" as ")
            && !alias.is_empty()
            && alias != "_"
        {
            bindings.push((original.trim(), alias.trim()));
        }
    }
    bindings
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some(".git" | ".tmp" | "patches" | "target")
                )
            {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_and_multiline_renames_are_detected() {
        let text = "pub use a::B as C;\nfn f() {}\npub use d::{\n    e as f,\n    g,\n};\n";
        let found = renamed_pub_use_aliases(text);
        assert!(found.iter().any(|hit| hit.contains("B as C")), "{found:?}");
        assert!(found.iter().any(|hit| hit.contains("e as f")), "{found:?}");
        assert!(found.len() == 2, "{found:?}");
    }

    #[test]
    fn parser_scope_ignores_non_public_bindings() {
        let text = "pub use a::B;\npub use c::D as _;\nuse e::F as G;\npub(crate) use h::I as J;\npub(super) use k::L as M;\n";
        assert!(renamed_pub_use_aliases(text).is_empty());
    }

    #[test]
    fn cross_crate_heads_are_extracted_from_joined_statements() {
        let text = "pub use taskmanager_theme::*;\npub use crate::x::Y;\npub use self::z;\npub use local_mod::{A, B};\npub use taskmanager_shell::\n    presentation::{command_help};\n";
        let heads: Vec<String> = pub_use_statements(text)
            .iter()
            .map(|s| s.split("::").next().unwrap_or("").trim().to_owned())
            .collect();
        assert_eq!(
            heads,
            vec![
                "taskmanager_theme",
                "crate",
                "self",
                "local_mod",
                "taskmanager_shell",
            ]
        );
    }

    #[test]
    fn module_declaration_collector_sees_nested_and_path_modules() {
        let text = "pub mod alpha;\nmod beta;\n#[path = \"x.rs\"]\npub mod gamma;\nfn f() {}\n";
        let names: Vec<String> = text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                let rest = trimmed
                    .strip_prefix("pub ")
                    .or_else(|| trimmed.strip_prefix("pub(crate) "))
                    .unwrap_or(trimmed);
                rest.strip_prefix("mod ")
                    .and_then(|r| r.split(';').next())
                    .map(str::trim)
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }
}
