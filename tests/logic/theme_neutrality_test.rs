//! source-inspection: static-policy
//!
//! Negative gate for theme-toolkit neutrality (ADR-026, ADR-051, CORE-07).
//!
//! `taskmanager-theme` is the single design source for every frontend: its
//! neutral modules (color/skins/palette/theme/tokens/platform/fonts/
//! detection) may never name a toolkit type, and since ADR-051 the crate
//! carries NO toolkit dependency at all — there are no optional binding
//! features left to enable. Each frontend owns its token→toolkit binding in
//! its own dependency closure (`taskmanager-ui::theme_binding` for GPUI,
//! `taskmanager-iced::theme_binding` for iced), so the manifest below is
//! the neutrality proof.
//!
//! Guards:
//! 1. The theme manifest declares zero toolkit dependencies — not even
//!    optional ones (`gpui`, `iced_core` must be absent).
//! 2. The theme crate declares zero `[features]` — the frontend dimension
//!    carries no conditional compilation (ADR-051).
//! 3. The neutral theme modules never `use` a toolkit.
//! 4. The TUI (the lossy terminal-mapping frontend) takes the theme with
//!    default features — trivially true and still asserted, since default
//!    features are the only features.
//! 5. No frontend manifest may route a toolkit through the theme: the
//!    `taskmanager-theme` dependency line names no feature anywhere.

use std::fs;
use std::path::{Path, PathBuf};

const THEME_MANIFEST: &str = "crates/taskmanager-theme/Cargo.toml";
const TUI_MANIFEST: &str = "crates/taskmanager-tui/Cargo.toml";
const ICED_MANIFEST: &str = "crates/taskmanager-iced/Cargo.toml";
const UI_MANIFEST: &str = "crates/taskmanager-ui/Cargo.toml";
const GPUI_MANIFEST: &str = "crates/taskmanager-gpui/Cargo.toml";
const BEVY_MANIFEST: &str = "crates/taskmanager-bevy-ui/Cargo.toml";
const NEUTRAL_MODULES: &[&str] = &[
    "crates/taskmanager-theme/src/color.rs",
    "crates/taskmanager-theme/src/skins.rs",
    "crates/taskmanager-theme/src/palette.rs",
    "crates/taskmanager-theme/src/theme.rs",
    "crates/taskmanager-theme/src/tokens.rs",
    "crates/taskmanager-theme/src/platform.rs",
    "crates/taskmanager-theme/src/fonts.rs",
    "crates/taskmanager-theme/src/detection.rs",
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn manifest(name: &str) -> String {
    fs::read_to_string(repository().join(name))
        .unwrap_or_else(|error| panic!("failed to read {name}: {error}"))
}

fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source.lines().map(|line| {
        // Strip both Rust (`//`) and TOML (`#`) comments — manifests and
        // module docs legitimately discuss the toolkit boundary in prose.
        let after_rust = line.split_once("//").map_or(line, |(code, _)| code);
        after_rust
            .split_once('#')
            .map_or(after_rust, |(code, _)| code)
    })
}

fn theme_dependency(manifest_source: &str, who: &str) -> String {
    manifest_source
        .lines()
        .find(|line| line.trim_start().starts_with("taskmanager-theme"))
        .unwrap_or_else(|| panic!("{who} must depend on taskmanager-theme"))
        .to_string()
}

#[test]
fn theme_manifest_has_no_toolkit_dependency_at_all() {
    let source = manifest(THEME_MANIFEST);
    let in_dependencies = source
        .split("[dependencies]")
        .nth(1)
        .expect("theme manifest has a [dependencies] section")
        .split('[')
        .next()
        .unwrap_or("");
    for toolkit in ["gpui", "iced_core", "iced", "ratatui"] {
        for line in code_lines(in_dependencies) {
            assert!(
                !line.trim_start().starts_with(toolkit),
                "the theme must not depend on {toolkit} in any form — optional bindings were \
                 deleted by ADR-051: {line}"
            );
        }
    }
}

#[test]
fn theme_crate_declares_no_features() {
    let source = manifest(THEME_MANIFEST);
    assert!(
        !code_lines(source.as_str()).any(|line| line.trim_start() == "[features]"),
        "the theme crate must carry zero features — the frontend dimension is \
         crate composition, not conditional compilation (ADR-051)"
    );
}

#[test]
fn neutral_theme_modules_never_name_a_toolkit() {
    for module in NEUTRAL_MODULES {
        let source = manifest(module);
        for needle in ["gpui", "ratatui", "iced"] {
            for (index, line) in source.lines().enumerate() {
                let code = line.split_once("//").map_or(line, |(code, _)| code);
                assert!(
                    !code.contains(needle),
                    "{}:{} must not name `{needle}` (ADR-026): {line}",
                    module,
                    index + 1
                );
            }
        }
    }
}

#[test]
fn no_frontend_routes_a_toolkit_through_the_theme() {
    for (manifest_name, crate_name) in [
        (TUI_MANIFEST, "taskmanager-tui"),
        (ICED_MANIFEST, "taskmanager-iced"),
        (UI_MANIFEST, "taskmanager-ui"),
        (GPUI_MANIFEST, "taskmanager-gpui"),
        (BEVY_MANIFEST, "taskmanager-bevy-ui"),
    ] {
        let line = theme_dependency(&manifest(manifest_name), crate_name);
        assert!(
            !line.contains("features"),
            "{crate_name} must take taskmanager-theme with default features — the theme has \
             no toolkit bindings to enable (ADR-026/051): {line}"
        );
    }
}
