//! Resolved frontend dependency-closure guards.
//!
//! Direct workspace allowlists are necessary but insufficient: a neutral
//! workspace crate can still pull a toolkit through another workspace crate.
//! These checks ask Cargo for the actual production tree for each product
//! shape and guard the resolved closure, including external packages.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn cargo_tree_packages(
    feature: Option<&str>,
    package: &str,
    edges: &str,
    all_targets: bool,
) -> Option<BTreeSet<String>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command.current_dir(repository()).args([
        "tree",
        "--locked",
        "--package",
        package,
        "--edges",
        edges,
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--color",
        "never",
    ]);
    if all_targets {
        command.args(["--target", "all"]);
    }
    if let Some(feature) = feature {
        command.args(["--no-default-features", "--features", feature]);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping dependency-closure check: cargo is unavailable: {error}");
            return None;
        }
        Err(error) => panic!("failed to invoke cargo tree for {package}: {error}"),
    };

    assert!(
        output.status.success(),
        "cargo tree failed for {package} ({feature:?}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

#[test]
fn frontend_production_closures_match_toolkit_boundaries() {
    let forbidden_for_non_gpui = ["gpui", "taskmanager-gpui", "taskmanager-ui"];

    for (feature, expects_gpui) in [("ui-tui", false), ("ui-iced", false), ("ui-gpui", true)] {
        let Some(closure) = cargo_tree_packages(Some(feature), "taskmanager", "no-dev", false)
        else {
            return;
        };

        if expects_gpui {
            assert!(
                closure.contains("gpui"),
                "GPUI product closure lost gpui: {closure:?}"
            );
            assert!(
                closure.contains("taskmanager-gpui"),
                "GPUI product closure lost taskmanager-gpui: {closure:?}"
            );
        } else {
            for forbidden in forbidden_for_non_gpui {
                assert!(
                    !closure.contains(forbidden),
                    "{feature} production closure reached forbidden {forbidden}: {closure:?}"
                );
            }
        }
    }
}

#[test]
fn neutral_assets_have_no_toolkit_in_their_production_closure() {
    let Some(closure) = cargo_tree_packages(None, "taskmanager-assets", "no-dev", false) else {
        return;
    };
    for forbidden in ["gpui", "iced", "ratatui"] {
        assert!(
            !closure.contains(forbidden),
            "neutral assets reached forbidden {forbidden}: {closure:?}"
        );
    }
}

#[test]
fn frontend_all_target_dev_closures_match_toolkit_boundaries() {
    let forbidden_for_non_gpui = ["gpui", "taskmanager-gpui", "taskmanager-ui"];

    for (feature, expects_gpui) in [("ui-tui", false), ("ui-iced", false), ("ui-gpui", true)] {
        let Some(closure) = cargo_tree_packages(Some(feature), "taskmanager", "all", true) else {
            return;
        };

        if expects_gpui {
            assert!(
                closure.contains("gpui"),
                "GPUI all-target dev closure lost gpui: {closure:?}"
            );
            assert!(
                closure.contains("taskmanager-gpui"),
                "GPUI all-target dev closure lost taskmanager-gpui: {closure:?}"
            );
        } else {
            for forbidden in forbidden_for_non_gpui {
                assert!(
                    !closure.contains(forbidden),
                    "{feature} all-target dev closure reached forbidden {forbidden}: {closure:?}"
                );
            }
        }
    }
}

#[test]
fn neutral_assets_have_no_toolkit_in_their_all_target_dev_closure() {
    let Some(closure) = cargo_tree_packages(None, "taskmanager-assets", "all", true) else {
        return;
    };
    for forbidden in [
        "gpui",
        "iced",
        "ratatui",
        "taskmanager-gpui",
        "taskmanager-ui",
        "taskmanager-icons",
    ] {
        assert!(
            !closure.contains(forbidden),
            "neutral assets all-target dev closure reached forbidden {forbidden}: {closure:?}"
        );
    }
}

#[test]
fn bevy_frontend_closures_keep_the_read_only_composition_boundary() {
    let required = [
        "taskmanager-bevy-ui",
        "taskmanager-application",
        "taskmanager-app-host",
        "taskmanager-shell",
        "taskmanager-theme",
        "taskmanager-ui-contract",
        "taskmanager-assets",
        "bevy",
        "bevy_ui",
        "bevy_ui_widgets",
        "bevy_scene",
    ];
    let forbidden = [
        "gpui",
        "iced",
        "ratatui",
        "taskmanager-gpui",
        "taskmanager-iced",
        "taskmanager-tui",
    ];

    for (edges, all_targets) in [("no-dev", false), ("all", true)] {
        let Some(closure) = cargo_tree_packages(None, "taskmanager-bevy-ui", edges, all_targets)
        else {
            return;
        };
        for package in required {
            assert!(
                closure.contains(package),
                "Bevy {edges} closure lost required package {package}: {closure:?}"
            );
        }
        for package in forbidden {
            assert!(
                !closure.contains(package),
                "Bevy {edges} closure reached peer toolkit {package}: {closure:?}"
            );
        }
    }
}
