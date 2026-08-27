//! source-inspection: static-policy
//!
//! Negative gate for theme-toolkit neutrality (ADR-026).
//!
//! `taskmanager-theme` is the single design source for every frontend: its
//! neutral modules (color/skins/palette/theme/tokens/fonts/detection) may
//! never name a toolkit type, and the crate may carry a toolkit dependency
//! only as the optional `gpui` feature that gates the one quarantined
//! binding module (`src/gpui.rs`). Rust's orphan rule forces the
//! `From<Color> for gpui::Rgba` conversions to live in the crate that owns
//! `Color`, so the toolkit-dependent code is quarantined by feature instead
//! of a separate adapter crate.
//!
//! Guards:
//! 1. `gpui` in the theme manifest is optional (`optional = true`) — a
//!    required toolkit dependency fails CI.
//! 2. The neutral theme modules never `use gpui` (only the cfg'd `gpui.rs`
//!    module may).
//! 3. The TUI (the non-gpui frontend in the tree) does not enable the
//!    theme's `gpui` feature — its `taskmanager-theme` dependency line must
//!    not request features, so the isolated TUI build links zero gpui.
//! 4. The gpui-side frontends (`taskmanager-gpui`, `taskmanager-ui`) DO enable
//!    the feature — forgetting it breaks `.bg(palette.fg)` silently, so the
//!    gate also fails if the feature is missing there.

use std::fs;
use std::path::{Path, PathBuf};

const THEME_MANIFEST: &str = "crates/taskmanager-theme/Cargo.toml";
const TUI_MANIFEST: &str = "crates/taskmanager-tui/Cargo.toml";
const ICED_MANIFEST: &str = "crates/taskmanager-iced/Cargo.toml";
const UI_MANIFEST: &str = "crates/taskmanager-ui/Cargo.toml";
const GPUI_MANIFEST: &str = "crates/taskmanager-gpui/Cargo.toml";
const NEUTRAL_MODULES: &[&str] = &[
    "crates/taskmanager-theme/src/color.rs",
    "crates/taskmanager-theme/src/skins.rs",
    "crates/taskmanager-theme/src/palette.rs",
    "crates/taskmanager-theme/src/theme.rs",
    "crates/taskmanager-theme/src/tokens.rs",
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
fn theme_manifest_has_no_required_toolkit_dependency() {
    let source = manifest(THEME_MANIFEST);
    // Only the `[dependencies]` declaration makes gpui a required dependency;
    // the `[features]` alias and the `[dev-dependencies]` test-support entry
    // are not transitive toolkit couplings.
    let in_dependencies = source
        .split("[dependencies]")
        .nth(1)
        .expect("theme manifest has a [dependencies] section")
        .split('[')
        .next()
        .unwrap_or("");
    let mut gpui_dep_lines = code_lines(in_dependencies).filter(|line| line.starts_with("gpui"));
    let declaration = gpui_dep_lines.next().unwrap_or_else(|| {
        panic!("theme must declare the optional gpui binding feature (ADR-026)")
    });
    assert!(
        gpui_dep_lines.next().is_none(),
        "only one gpui dependency declaration is expected"
    );
    assert!(
        declaration.contains("optional"),
        "the gpui dependency must be optional (ADR-026): {declaration}"
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
fn tui_consumes_the_theme_without_the_gpui_feature() {
    let line = theme_dependency(&manifest(TUI_MANIFEST), "taskmanager-tui");
    assert!(
        !line.contains("features"),
        "the TUI must take taskmanager-theme with default features (no gpui, ADR-026): {line}"
    );
}

#[test]
fn iced_frontend_consumes_the_theme_without_the_gpui_feature() {
    let line = theme_dependency(&manifest(ICED_MANIFEST), "taskmanager-iced");
    assert!(
        !line.contains("features"),
        "the iced frontend must take taskmanager-theme with default features (no gpui, ADR-026/027): {line}"
    );
}

#[test]
fn gpui_frontends_enable_the_theme_gpui_feature() {
    for (manifest_name, crate_name) in [
        (UI_MANIFEST, "taskmanager-ui"),
        (GPUI_MANIFEST, "taskmanager-gpui"),
    ] {
        let line = theme_dependency(&manifest(manifest_name), crate_name);
        assert!(
            line.contains("gpui"),
            "{crate_name} must enable taskmanager-theme's gpui feature for the palette/token conversions (ADR-026): {line}"
        );
    }
}
