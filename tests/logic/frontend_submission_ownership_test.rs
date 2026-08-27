//! source-inspection: static-policy
//!
//! Structural gate: the TUI and Iced frontends must never call
//! `PlatformClient` submit methods directly — every platform request crosses
//! the shared `ShellApp`/`PlatformEffect`/`queue_effect` seam (ADR-027, the
//! G-03 convergence). GPUI is the sanctioned dual-track consumer and is
//! explicitly out of scope here.
//!
//! This is the meta-gate class CLAUDE.md rule 6 sanctions: an
//! absence/ownership invariant asserted over source shape, complementing the
//! per-lane behavior tests (e.g. the TUI directory-usage round-trip) that
//! prove the queued path works but would not catch a NEW direct call site.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn tui_and_iced_submit_only_through_the_shell_effect_seam() {
    for frontend in ["crates/taskmanager-tui", "crates/taskmanager-iced"] {
        let root = repository().join(frontend);
        let mut violations = Vec::new();
        for path in rust_sources(&root) {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                // Any direct submit/request call on a platform client bypasses
                // the shell's typed effect dispatch (status reporting,
                // correlation bookkeeping, and the shared lane pacing).
                if trimmed.contains("platform.submit_")
                    || trimmed.contains("client.submit_")
                    || trimmed.contains(".request_refresh(")
                {
                    violations.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(repository()).unwrap_or(&path).display(),
                        index + 1,
                        trimmed.trim()
                    ));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "{frontend} must route platform requests through \
             ShellApp/PlatformEffect/queue_effect, found direct calls:\n{}",
            violations.join("\n")
        );
    }
}
