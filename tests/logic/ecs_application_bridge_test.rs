//! source-inspection: static-policy
//!
//! ECS/application bridge architecture gate.
//!
//! ECS owns runtime scheduling only. Domain facts cross into application
//! reducers through the existing correlated event publisher and
//! `PlatformClient::try_drain` path; no ECS module may become a projection or
//! frontend dependency.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    fs::read_to_string(repository().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn ecs_sources() -> Vec<(String, String)> {
    let root = repository().join("crates/taskmanager-platform-runtime/src");
    let ecs_root = root.join("ecs.rs");
    let mut sources = vec![(
        ecs_root.to_string_lossy().into_owned(),
        fs::read_to_string(&ecs_root).expect("ecs facade source"),
    )];
    let domain_root = root.join("ecs");
    let mut paths = fs::read_dir(&domain_root)
        .expect("ecs domain directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        sources.push((
            path.to_string_lossy().into_owned(),
            fs::read_to_string(&path).expect("ecs domain source"),
        ));
    }
    sources
}

#[test]
fn ecs_sources_cannot_bypass_the_application_projection_boundary() {
    let forbidden = [
        "PlatformEventBatch",
        "SystemProjectionStore",
        "taskmanager-shell",
        "taskmanager-gpui",
        "taskmanager-iced",
        "taskmanager-tui",
        "taskmanager-bevy-ui",
    ];
    let sources = ecs_sources();
    assert!(
        !sources.is_empty(),
        "ECS source inventory must not be empty"
    );
    for (path, source) in sources {
        for token in forbidden {
            assert!(
                !source.contains(token),
                "ECS source {path} bypasses the application/UI boundary through {token}"
            );
        }
    }
}

#[test]
fn non_bevy_frontends_consume_shared_projection_without_ecs_dependency() {
    // Bevy is intentionally excluded: its own product contract owns Bevy ECS
    // as the renderer/runtime surface and has a separate dependency-closure
    // gate. This assertion is specifically for GPUI, Iced and Ratatui.
    for frontend in ["taskmanager-gpui", "taskmanager-iced", "taskmanager-tui"] {
        let source_root = repository().join("crates").join(frontend).join("src");
        let manifest = read(&format!("crates/{frontend}/Cargo.toml"));
        assert!(
            !manifest.contains("bevy_ecs") && !manifest.contains("bevy_app"),
            "{frontend} must depend on the shell/application projection, not Bevy ECS"
        );
        let mut pending = vec![source_root];
        while let Some(path) = pending.pop() {
            let metadata = fs::metadata(&path).expect("frontend source metadata");
            if metadata.is_dir() {
                for entry in fs::read_dir(&path).expect("frontend source directory") {
                    pending.push(entry.expect("frontend source entry").path());
                }
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("frontend Rust source");
            for token in ["bevy_ecs", "bevy_app", "bevy::ecs", "bevy::app"] {
                assert!(
                    !source.contains(token),
                    "frontend source {} bypasses the shared projection with {token}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn runtime_defaults_to_the_headless_bevy_app_kernel() {
    let ecs = read("crates/taskmanager-platform-runtime/src/ecs.rs");
    assert!(!ecs.contains("unsafe"));
}

#[test]
fn ecs_behavior_tests_are_mounted_from_the_headless_test_tree() {
    let ecs = read("crates/taskmanager-platform-runtime/src/ecs.rs");
    assert!(!ecs.contains("mod tests {"));
    assert!(!ecs.contains("#[test]"));
    for path in [
        "crates/taskmanager-platform-runtime/tests/headless/ecs_scheduler.rs",
        "crates/taskmanager-platform-runtime/tests/headless/ecs_replay.rs",
        "crates/taskmanager-platform-runtime/tests/headless/ecs_benchmark.rs",
    ] {
        assert!(
            Path::new(&repository().join(path)).is_file(),
            "missing {path}"
        );
    }
    for path in [
        "crates/taskmanager-platform-runtime/src/ecs/benchmark.rs",
        "crates/taskmanager-platform-runtime/src/ecs/replay.rs",
    ] {
        assert!(
            !Path::new(&repository().join(path)).exists(),
            "stale test source {path}"
        );
    }
}
