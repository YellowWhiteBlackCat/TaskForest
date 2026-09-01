//! source-inspection: static-policy
//!
//! Cargo feature matrix gate: every native hardware backend feature must be
//! reachable from the standard artifact, and reduced release profiles must
//! fail compilation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn feature_table(manifest: &str) -> BTreeMap<String, Vec<String>> {
    let mut in_features = false;
    let mut entries = BTreeMap::new();
    let mut pending_key = None::<String>;
    let mut pending_value = String::new();
    let mut bracket_depth = 0_i32;

    for raw_line in manifest.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line == "[features]" {
            in_features = true;
            continue;
        }
        if !in_features {
            continue;
        }
        if line.starts_with('[') && pending_key.is_none() {
            break;
        }
        if line.is_empty() {
            continue;
        }

        if pending_key.is_none() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            pending_key = Some(key.trim().trim_matches('"').to_owned());
            pending_value.push_str(value.trim());
            bracket_depth = bracket_balance(value);
        } else {
            pending_value.push_str(line);
            bracket_depth += bracket_balance(line);
        }

        if bracket_depth <= 0 {
            if let Some(key) = pending_key.take() {
                entries.insert(key, quoted_members(&pending_value));
            }
            pending_value.clear();
            bracket_depth = 0;
        }
    }

    entries
}

fn bracket_balance(value: &str) -> i32 {
    value.chars().fold(0, |depth, character| match character {
        '[' => depth + 1,
        ']' => depth - 1,
        _ => depth,
    })
}

fn quoted_members(value: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut quoted = false;
    let mut current = String::new();
    for character in value.chars() {
        match (quoted, character) {
            (false, '"') => quoted = true,
            (true, '"') => {
                members.push(std::mem::take(&mut current));
                quoted = false;
            }
            (true, character) => current.push(character),
            (false, _) => {}
        }
    }
    members
}

fn read_features(path: impl AsRef<Path>) -> BTreeMap<String, Vec<String>> {
    let manifest = fs::read_to_string(path).expect("Cargo manifest should be readable");
    feature_table(&manifest)
}

fn optional_dependency_names(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut dependencies = Vec::new();
    let mut pending_key = None::<String>;
    let mut pending_value = String::new();
    let mut expression_depth = 0_i32;

    for raw_line in manifest.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && pending_key.is_none() {
            in_dependencies = line == "[dependencies]" || line.ends_with(".dependencies]");
            continue;
        }
        if !in_dependencies || line.is_empty() {
            continue;
        }

        if pending_key.is_none() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            pending_key = Some(key.trim().trim_matches('"').to_owned());
            pending_value.push_str(value.trim());
            expression_depth = expression_balance(value);
        } else {
            pending_value.push_str(line);
            expression_depth += expression_balance(line);
        }

        if expression_depth <= 0 {
            let normalized = pending_value
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            if normalized.contains("optional=true")
                && let Some(key) = pending_key.take()
            {
                dependencies.push(key);
            } else {
                pending_key = None;
            }
            pending_value.clear();
            expression_depth = 0;
        }
    }

    dependencies.sort();
    dependencies
}

fn expression_balance(value: &str) -> i32 {
    value.chars().fold(0, |depth, character| match character {
        '[' | '{' => depth + 1,
        ']' | '}' => depth - 1,
        _ => depth,
    })
}

#[test]
fn every_native_os_hardware_backend_feature_is_in_the_standard_artifact() {
    let repository = repository();
    let native = read_features(repository.join("crates/taskmanager-platform-native/Cargo.toml"));
    // ADR-051: the release feature matrix lives in every product manifest,
    // not in a root dispatch package.
    let product_names = [
        "taskmanager-gpui",
        "taskmanager-iced",
        "taskmanager-tui",
        "taskmanager-bevy-ui",
    ];
    let products: Vec<_> = product_names
        .iter()
        .map(|name| {
            (
                *name,
                read_features(repository.join("crates").join(name).join("Cargo.toml")),
            )
        })
        .collect();

    let mut hardware_feature_count = 0;
    for platform in ["linux", "macos", "windows"] {
        let crate_name = format!("taskmanager-platform-{platform}");
        let features = read_features(
            repository
                .join("crates")
                .join(&crate_name)
                .join("Cargo.toml"),
        );
        assert_eq!(
            features.get("default"),
            Some(&vec!["hardware-all".to_owned()]),
            "{platform} product defaults must select the complete hardware registry"
        );
        let hardware_all = features
            .get("hardware-all")
            .unwrap_or_else(|| panic!("{platform} must declare the hardware-all registry"));

        let hardware_features = features
            .keys()
            .filter(|feature| {
                !matches!(
                    feature.as_str(),
                    "default" | "hardware-all" | "test-support"
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        hardware_feature_count += hardware_features.len();

        for feature in hardware_features {
            assert!(
                hardware_all.contains(&feature),
                "{platform} hardware feature `{feature}` is omitted from the standard hardware-all artifact"
            );

            let native_route = format!("{crate_name}/{feature}");
            if let Some(native_fallback) = native.get(&feature) {
                assert!(
                    native_fallback.contains(&native_route),
                    "declared native fallback feature `{feature}` must route to `{native_route}`"
                );
            }

            for (product, product_features) in &products {
                if let Some(product_fallback) = product_features.get(&feature) {
                    let route = format!("taskmanager-app-host/{feature}");
                    assert!(
                        product_fallback.contains(&route),
                        "declared {product} fallback feature `{feature}` must route to `{route}`"
                    );
                }
            }
        }

        let platform_hardware_all = format!("{crate_name}/hardware-all");
        assert!(
            native
                .get("hardware-all")
                .is_some_and(|members| members.contains(&platform_hardware_all)),
            "the native OS selector must include `{platform_hardware_all}`"
        );
    }

    assert!(
        hardware_feature_count > 0,
        "the guard must exercise at least one runtime-probed hardware backend"
    );

    for (product, product_features) in &products {
        assert_eq!(
            product_features.get("default"),
            Some(&vec!["hardware-all".to_owned()]),
            "{product} must default to the complete hardware registry"
        );
        assert_eq!(
            product_features.get("hardware-all"),
            Some(&vec!["taskmanager-app-host/hardware-all".to_owned()]),
            "{product} must route hardware-all through the shared app host"
        );
    }
}

#[test]
fn every_optional_native_hardware_dependency_is_reachable_from_hardware_all() {
    let repository = repository();
    let mut optional_dependency_count = 0;

    for platform in ["linux", "macos", "windows"] {
        let manifest_path = repository
            .join("crates")
            .join(format!("taskmanager-platform-{platform}"))
            .join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", manifest_path.display()));
        let features = feature_table(&manifest);
        let hardware_all = features
            .get("hardware-all")
            .unwrap_or_else(|| panic!("{platform} must declare hardware-all"));

        for dependency in optional_dependency_names(&manifest) {
            optional_dependency_count += 1;
            let explicit = format!("dep:{dependency}");
            let enabling_features = features
                .iter()
                .filter_map(|(feature, members)| {
                    members
                        .iter()
                        .any(|member| member == &explicit)
                        .then_some(feature)
                })
                .collect::<Vec<_>>();

            if hardware_all.contains(&explicit) {
                continue;
            }
            if enabling_features.is_empty() {
                assert!(
                    hardware_all.contains(&dependency),
                    "{platform} optional dependency `{dependency}` creates an implicit feature \
                     but the standard hardware-all artifact does not enable it"
                );
            } else {
                assert!(
                    enabling_features
                        .iter()
                        .any(|feature| hardware_all.contains(*feature)),
                    "{platform} optional dependency `{dependency}` is enabled only by \
                     {enabling_features:?}, none of which belongs to hardware-all"
                );
            }
        }
    }

    assert!(
        optional_dependency_count > 0,
        "the guard must exercise at least one optional runtime hardware dependency"
    );
}

#[test]
fn every_release_composition_edge_rejects_reduced_hardware_profiles() {
    for relative in [
        "crates/taskmanager-gpui/src/lib.rs",
        "crates/taskmanager-iced/src/lib.rs",
        "crates/taskmanager-tui/src/lib.rs",
        "crates/taskmanager-bevy-ui/src/lib.rs",
        "crates/taskmanager-platform-native/src/lib.rs",
        "crates/taskmanager-platform-linux/src/lib.rs",
        "crates/taskmanager-platform-macos/src/lib.rs",
        "crates/taskmanager-platform-windows/src/lib.rs",
    ] {
        let source = fs::read_to_string(repository().join(relative))
            .unwrap_or_else(|error| panic!("{relative} is unreadable: {error}"));
        assert!(
            source.contains("#[cfg(all(not(debug_assertions), not(feature = \"hardware-all\")))]")
                && source.contains("compile_error!"),
            "{relative} must reject release artifacts that omit hardware-all"
        );
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn feature_parser_handles_multiline_arrays_and_comments() {
        let parsed = feature_table(
            r#"
[package]
name = "fixture"

[features]
default = ["hardware-all"]
hardware-all = [
    "amd",
    "nvidia", # optional runtime backend
]
nvidia = ["dep:nvml"]

[dependencies]
nvml = "1"
"#,
        );

        assert_eq!(
            parsed.get("hardware-all"),
            Some(&vec!["amd".to_owned(), "nvidia".to_owned()])
        );
        assert_eq!(parsed.get("nvidia"), Some(&vec!["dep:nvml".to_owned()]));
        assert!(!parsed.contains_key("nvml"));
    }

    #[test]
    fn optional_dependency_parser_covers_plain_target_and_multiline_tables() {
        let parsed = optional_dependency_names(
            r#"
[dependencies]
plain = "1"
nvml-wrapper = { version = "0.10", optional = true }
"quoted-sdk" = { version = "3", optional = true }

[target.'cfg(target_os = "linux")'.dependencies]
future-sdk = {
    version = "2",
    default-features = false,
    optional = true,
}

[dev-dependencies]
fixture = { version = "1", optional = true }
"#,
        );

        assert_eq!(parsed, ["future-sdk", "nvml-wrapper", "quoted-sdk"]);
    }
}
