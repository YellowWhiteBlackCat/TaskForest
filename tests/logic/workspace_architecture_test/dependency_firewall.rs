//! source-inspection: static-policy
//!
//! Workspace dependency firewall: sanctioned crate edges, feature boundaries
//! and negative legacy/vendor guards.

use std::collections::BTreeSet;
use std::fs;

use super::{repository, rust_sources};

#[path = "dependency_firewall/frontend_safety.rs"]
mod frontend_safety;

fn rust_code_without_line_comments(root: &std::path::Path) -> String {
    rust_sources(root)
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn frontend_sources(repository: &std::path::Path) -> (String, String) {
    (
        rust_code_without_line_comments(&repository.join("crates/taskmanager-gpui/src/gpui_app")),
        rust_code_without_line_comments(&repository.join("crates/taskmanager-tui/src")),
    )
}

const FORBIDDEN_NATIVE_IO_AND_COMMANDS: &[&str] = &[
    "std::process::Command",
    "File::open",
    "OpenOptions",
    "std::fs::read",
    "read_to_string(",
    "\"/proc/",
    "\"/sys/",
    "\"/etc/",
    "\"systemctl\"",
    "\"journalctl\"",
    "\"rc-service\"",
    "\"smartctl\"",
    "\"nvme\"",
    "\"nvidia-smi\"",
    "\"rocm-smi\"",
    "\"intel_gpu_top\"",
    "\"ps -",
];

/// Detect a call on the independent Rust identifier `Command`, without
/// mistaking a longer type such as `SpawnedCommand::new` for that identifier.
///
/// The caller has already removed line comments. The identifier boundary and
/// exact call suffix make this a small structural policy check instead of the
/// former raw-substring test.
fn contains_command_constructor(source: &str) -> bool {
    const CONSTRUCTOR: &str = "Command::new";
    source.match_indices(CONSTRUCTOR).any(|(offset, _)| {
        let before_is_identifier = source[..offset]
            .chars()
            .next_back()
            .is_some_and(|character| character == '_' || character.is_alphanumeric());
        let call_follows = source[offset + CONSTRUCTOR.len()..]
            .trim_start()
            .starts_with('(');
        !before_is_identifier && call_follows
    })
}

/// Extract every `taskmanager-*` workspace dependency declared in a Cargo
/// manifest, across BOTH TOML forms and all tracked kinds:
/// * inline — `taskmanager-foo = { path = ".." }` under `[dependencies]`,
///   `[build-dependencies]`, or `[target."<cfg>".dependencies]`;
/// * table — `[dependencies.taskmanager-foo]` / `[build-dependencies.taskmanager-foo]`
///   (the header itself names the dep).
///
/// `dev-dependencies` (test-only) are excluded. Handles target-spec quoting.
/// This MUST agree with Cargo's own resolution — the reverse firewalls route
/// exclusively through it, so an unrecognized form would let an unsanctioned
/// crate wire to an audited `unsafe` trust root undetected (regression test
/// below pins both forms).
fn production_workspace_dependencies(manifest: &str) -> BTreeSet<String> {
    let mut section_path: Vec<&str> = Vec::new();
    let mut dependencies = BTreeSet::new();

    let is_tracked_dep_section = |path: &[&str]| {
        (path
            .iter()
            .any(|s| *s == "dependencies" || *s == "build-dependencies"))
            && !path.contains(&"dev-dependencies")
    };

    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            // Split the section path, stripping target-spec quotes ('...' / "...").
            section_path = header
                .split('.')
                .map(|seg| seg.trim_matches(|c| c == '\'' || c == '"'))
                .collect();
            // Table form `[<kind>.taskmanager-foo]`: the segment(s) after the
            // dependency-kind name the dependency directly.
            if is_tracked_dep_section(&section_path)
                && let Some(idx) = section_path
                    .iter()
                    .position(|s| *s == "dependencies" || *s == "build-dependencies")
                && idx + 1 < section_path.len()
            {
                let name = section_path[idx + 1..].join(".");
                if let Some(dep) = taskmanager_dep_name(&name) {
                    dependencies.insert(dep.to_owned());
                }
            }
            continue;
        }
        // Inline form: `name = ...` under a tracked dependency section.
        if !is_tracked_dep_section(&section_path) {
            continue;
        }
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        // Take the key before any `.` (e.g. `foo.workspace`/`foo.version` inheritance).
        let name = name.split_once('.').map_or(name, |(n, _)| n).trim();
        if let Some(dep) = taskmanager_dep_name(name) {
            dependencies.insert(dep.to_owned());
        }
    }
    dependencies
}

/// Accept a manifest dependency key only if it names a `taskmanager-*` crate
/// (strips trailing inheritance suffixes already handled by the caller).
fn taskmanager_dep_name(name: &str) -> Option<&str> {
    let trimmed = name.trim();
    trimmed.starts_with("taskmanager-").then_some(trimmed)
}

/// Every workspace package manifest: the root `Cargo.toml` plus each
/// `crates/*/Cargo.toml`, keyed by package name.
fn workspace_crate_manifests(repository: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    let mut manifests = vec![("taskmanager".to_owned(), repository.join("Cargo.toml"))];
    for entry in fs::read_dir(repository.join("crates")).expect("crates dir readable") {
        let entry = entry.expect("directory entry readable");
        if entry.path().is_dir() {
            manifests.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path().join("Cargo.toml"),
            ));
        }
    }
    manifests
}

/// Workspace packages whose production deps include `boundary`, plus the
/// subset of those not in `sanctioned` (a SUBSET check — the boundary crate
/// existing with zero dependents passes, any unsanctioned wire fails).
fn unsanctioned_dependents(
    repository: &std::path::Path,
    boundary: &str,
    sanctioned: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut dependents = Vec::new();
    let mut unsanctioned = Vec::new();
    for (package, manifest_path) in workspace_crate_manifests(repository) {
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        if production_workspace_dependencies(&manifest).contains(boundary) {
            if !sanctioned.contains(&package.as_str()) {
                unsanctioned.push(package.clone());
            }
            dependents.push(package);
        }
    }
    dependents.sort();
    (dependents, unsanctioned)
}

/// The audited boundary crates depend only on non-workspace crates.
fn assert_boundary_crate_has_no_workspace_dependencies(
    repository: &std::path::Path,
    crate_dir: &str,
) {
    let boundary_manifest = fs::read_to_string(repository.join(crate_dir).join("Cargo.toml"))
        .expect("boundary crate manifest readable");
    let boundary_deps = production_workspace_dependencies(&boundary_manifest);
    assert!(
        boundary_deps.is_empty(),
        "the audited boundary crate must have zero workspace dependencies, got {boundary_deps:?}"
    );
}

/// The four audited boundary crates — the ONLY production sources where
/// `unsafe` is permitted (ADR-022 perf_event_open, ADR-024 AF_PACKET,
/// ADR-025 SCM_RIGHTS, ADR-031 Windows system APIs).
const BOUNDARY_CRATE_SRC_DIRS: &[&str] = &[
    "crates/taskmanager-perf-ioctl/src",
    "crates/taskmanager-afpacket/src",
    "crates/taskmanager-fd-bridge/src",
    "crates/taskmanager-windows-api/src",
];

/// Read a repository-relative source file with a uniform failure message.
fn read_source(repository: &std::path::Path, relative: &str) -> String {
    fs::read_to_string(repository.join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_crate_root(path: &std::path::Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "lib.rs" || name == "main.rs")
        && path.parent().is_some_and(|parent| parent.ends_with("src"))
}

fn is_boundary_crate_path(path: &std::path::Path, repository: &std::path::Path) -> bool {
    let relative = path.strip_prefix(repository).unwrap_or(path);
    BOUNDARY_CRATE_SRC_DIRS
        .iter()
        .any(|prefix| relative.starts_with(prefix))
}

/// All `.rs` sources of the four audited boundary crates, failing loudly if
/// one of the directories disappears.
fn boundary_source_files(repository: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let mut files = Vec::new();
    for dir in BOUNDARY_CRATE_SRC_DIRS {
        let walked = frontend_safety::walk_rust_files(&repository.join(dir));
        assert!(
            !walked.is_empty(),
            "boundary crate has no sources — did {dir} move?"
        );
        files.extend(walked);
    }
    files
}

/// SUBSET reverse firewall for an audited boundary crate: only `sanctioned`
/// consumers may depend on it (zero dependents passes; an unsanctioned wire
/// fails), and the crate itself carries zero workspace dependencies.
fn assert_boundary_consumers_are_sanctioned(
    repository: &std::path::Path,
    boundary: &str,
    sanctioned: &[&str],
) {
    let (dependents, unsanctioned) = unsanctioned_dependents(repository, boundary, sanctioned);
    assert!(
        unsanctioned.is_empty(),
        "{boundary} must be reachable only from sanctioned consumers (current dependents: {dependents:?}); unsanctioned: {unsanctioned:?}"
    );
    assert_boundary_crate_has_no_workspace_dependencies(repository, &format!("crates/{boundary}"));
}

#[test]
fn dependency_parser_catches_both_toml_forms_and_build_deps() {
    // Regression: the reverse firewalls route exclusively through
    // production_workspace_dependencies. The TOML *table* form
    // `[dependencies.NAME]` and `[build-dependencies]` were once invisible to
    // it — an unsanctioned crate could reach an audited unsafe trust root by
    // simply using the idiomatic table syntax. This pins both forms + dev-exclusion.
    let manifest = "\
[dependencies]
taskmanager-core = { path = \"../taskmanager-core\" }
[dependencies.taskmanager-afpacket]
path = \"../taskmanager-afpacket\"
[build-dependencies]
taskmanager-perf-ioctl = { path = \"../taskmanager-perf-ioctl\" }
[build-dependencies.taskmanager-fd-bridge]
path = \"../taskmanager-fd-bridge\"
[dev-dependencies]
taskmanager-test-support = { path = \"../taskmanager-test-support\" }
[target.'cfg(target_os = \"linux\")'.dependencies]
taskmanager-platform-linux = { path = \"../taskmanager-platform-linux\" }
";
    let deps = production_workspace_dependencies(manifest);
    for (crate_name, reason) in [
        (
            "taskmanager-core",
            "inline [dependencies] form must be caught",
        ),
        (
            "taskmanager-afpacket",
            "table [dependencies.NAME] form must be caught",
        ),
        (
            "taskmanager-perf-ioctl",
            "[build-dependencies] inline must be caught",
        ),
        (
            "taskmanager-fd-bridge",
            "[build-dependencies.NAME] table form must be caught",
        ),
        (
            "taskmanager-platform-linux",
            "target-qualified [target.<cfg>.dependencies] must be caught",
        ),
    ] {
        assert!(deps.contains(crate_name), "{reason}");
    }
    assert!(
        !deps.contains("taskmanager-test-support"),
        "dev-dependencies must NOT be tracked (test-only)"
    );
}

#[test]
fn command_constructor_policy_uses_an_independent_identifier_boundary() {
    for source in [
        "let child = Command::new(\"smartctl\");",
        "let child = std::process::Command::new (\"smartctl\");",
    ] {
        assert!(
            contains_command_constructor(source),
            "an independent Command constructor must be detected: {source}"
        );
    }
    for source in [
        "let child = SpawnedCommand::new(child, stdout, stderr);",
        "let child = TaskCommand::new(\"typed\");",
        "let child = Command::newer(\"typed\");",
    ] {
        assert!(
            !contains_command_constructor(source),
            "a longer identifier or method must not be reported as Command::new: {source}"
        );
    }
}

/// macOS and the Windows third-OS adapter share the same inward edges.
const MACOS_WINDOWS_ADAPTER_DEPS: &[&str] = &[
    "taskmanager-application",
    "taskmanager-core",
    "taskmanager-platform-contract",
    "taskmanager-platform-portable",
    "taskmanager-platform-provider",
    "taskmanager-platform-runtime",
    "taskmanager-tray-muda",
];

fn assert_workspace_dependencies(repository: &std::path::Path, package: &str, expected: &[&str]) {
    let manifest_path = if package == "taskmanager" {
        repository.join("Cargo.toml")
    } else {
        repository.join("crates").join(package).join("Cargo.toml")
    };
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
    let actual = production_workspace_dependencies(&manifest);
    let expected: BTreeSet<String> = expected.iter().map(ToString::to_string).collect();
    assert_eq!(
        actual, expected,
        "{package} crossed its allowed workspace dependency firewall"
    );
}

#[test]
fn workspace_dependency_dag_matches_the_inward_firewall() {
    let repository = repository();
    for (package, expected) in [
        ("taskmanager-core", &[][..]),
        ("taskmanager-telemetry-store", &["taskmanager-core"][..]),
        // ADR-028: opt-in history persistence is a pure-safe core-only leaf;
        // the composition edge (root crate) wires it into the ingestor sink.
        ("taskmanager-history-store", &["taskmanager-core"][..]),
        ("taskmanager-platform-contract", &["taskmanager-core"][..]),
        (
            "taskmanager-application",
            &["taskmanager-core", "taskmanager-platform-contract"][..],
        ),
        (
            "taskmanager-platform-provider",
            &["taskmanager-core", "taskmanager-platform-contract"][..],
        ),
        (
            "taskmanager-platform-portable",
            &[
                "taskmanager-core",
                "taskmanager-platform-contract",
                "taskmanager-platform-provider",
            ][..],
        ),
        (
            "taskmanager-platform-runtime",
            &["taskmanager-application", "taskmanager-core"][..],
        ),
        ("taskmanager-ui-contract", &["taskmanager-application"][..]),
        (
            "taskmanager-accessibility-linux",
            &["taskmanager-ui-contract"][..],
        ),
        ("taskmanager-assets", &[][..]),
        // ADR-031: the Windows ABI wrapper is an isolated trust root with no
        // workspace dependencies; only the Windows adapter may consume it.
        ("taskmanager-windows-api", &[][..]),
        // ADR-017: the theme crate is gpui-only — no taskmanager-* edges.
        ("taskmanager-theme", &[][..]),
        // ADR-027: renderer-independent shell state may consume only the
        // application reducer/ports, the bounded telemetry read model and the
        // toolkit-neutral UI contract.
        (
            "taskmanager-shell",
            &[
                "taskmanager-application",
                "taskmanager-telemetry-store",
                "taskmanager-ui-contract",
            ][..],
        ),
        // ADR-023: the privileged helper binary depends ONLY on the audited perf
        // boundary crate (the workspace's sole unsafe trust root, reached through
        // its safe API) — not on the whole Linux adapter — so the privileged
        // attack surface stays minimal. serde/serde_json are non-workspace deps
        // and not tracked by this DAG assertion.
        (
            "taskmanager-privilege-helper",
            &["taskmanager-perf-ioctl"][..],
        ),
        // ADR-023/031: process-control helper depends only on the audited
        // boundary crates — windows-api on Windows, fd-bridge on Linux (the
        // pinned pidfd for routing foreign signals, 4dfe73ef) — keeping the
        // privileged attack surface minimal.
        (
            "taskmanager-process-control-helper",
            &["taskmanager-windows-api", "taskmanager-fd-bridge"][..],
        ),
        (
            "taskmanager-platform-linux",
            &[
                "taskmanager-application",
                "taskmanager-core",
                "taskmanager-escalation",
                "taskmanager-afpacket",
                "taskmanager-perf-ioctl",
                "taskmanager-platform-contract",
                "taskmanager-platform-portable",
                "taskmanager-platform-provider",
                "taskmanager-platform-runtime",
            ][..],
        ),
        (
            "taskmanager-platform-native",
            &[
                "taskmanager-core",
                "taskmanager-platform-contract",
                "taskmanager-platform-linux",
                "taskmanager-platform-macos",
                "taskmanager-platform-windows",
            ][..],
        ),
        (
            "taskmanager-app-host",
            &[
                "taskmanager-application",
                "taskmanager-core",
                "taskmanager-history-store",
                "taskmanager-platform-native",
                "taskmanager-platform-runtime",
            ][..],
        ),
        ("taskmanager-platform-macos", MACOS_WINDOWS_ADAPTER_DEPS),
        (
            "taskmanager-platform-windows",
            // Third-OS adapter: same inward edges as macOS (second-OS contract
            // proof) plus its dedicated Windows ABI wrapper and escalation seam —
            // no Linux shapes, no store.
            &[
                "taskmanager-application",
                "taskmanager-core",
                "taskmanager-escalation",
                "taskmanager-platform-contract",
                "taskmanager-platform-portable",
                "taskmanager-platform-provider",
                "taskmanager-platform-runtime",
                "taskmanager-tray-muda",
                "taskmanager-windows-api",
            ][..],
        ),
        (
            "taskmanager-tui",
            &[
                "taskmanager-app-host",
                "taskmanager-application",
                "taskmanager-assets",
                "taskmanager-shell",
                // The neutral design system (ADR-026): the TUI takes the theme
                // with default features, so this edge links zero toolkit.
                "taskmanager-theme",
                "taskmanager-ui-contract",
            ][..],
        ),
        (
            "taskmanager-iced",
            &[
                "taskmanager-app-host",
                "taskmanager-application",
                // ADR-026 fonts policy: run.rs registers the bundled font bytes
                // into iced's font database — the same pure leaf GPUI embeds.
                "taskmanager-assets",
                // The registry half of taskmanager-icons is toolkit-neutral;
                // taskmanager-iced disables its optional GPUI adapter feature.
                "taskmanager-icons",
                "taskmanager-shell",
                "taskmanager-theme",
                "taskmanager-ui-contract",
            ][..],
        ),
        (
            "taskmanager-gpui",
            &[
                "taskmanager-accessibility-linux",
                "taskmanager-app-host",
                "taskmanager-application",
                "taskmanager-assets",
                "taskmanager-core",
                "taskmanager-icons",
                "taskmanager-shell",
                "taskmanager-telemetry-store",
                "taskmanager-theme",
                "taskmanager-ui",
                "taskmanager-ui-contract",
            ][..],
        ),
        (
            "taskmanager",
            // `taskmanager-escalation` is the per-feature privilege-escalation
            // seam (ADR-023): the CLI `--gpu-engines` surface drives the
            // polkit/pkexec helper crossing from this composition edge. It is a
            // pure safe-Rust leaf with zero dependencies, so adding it here
            // cannot leak a platform adapter or a trust root.
            &[
                "taskmanager-app-host",
                "taskmanager-application",
                "taskmanager-core",
                "taskmanager-escalation",
                "taskmanager-gpui",
                "taskmanager-assets",
                "taskmanager-shell",
                "taskmanager-telemetry-store",
                // Root GPUI integration tests use the owned component layer
                // directly. It is optional and activated only by ui-gpui;
                // the resolved all-target closure gate proves it is absent
                // from the TUI/Iced shapes.
                "taskmanager-ui",
                "taskmanager-ui-contract",
                // ADR-029: the other two UI shapes are optional dependencies
                // gated behind `ui-tui`/`ui-iced`; exactly one is enabled per
                // build (build.rs enforces it). The DAG tracks manifest
                // edges, so both optional edges appear here.
                "taskmanager-tui",
                "taskmanager-iced",
            ][..],
        ),
    ] {
        assert_workspace_dependencies(&repository, package, expected);
    }
}

#[test]
fn linux_engine_does_not_depend_on_application_use_cases() {
    let repository = repository();
    let engine = repository.join("crates/taskmanager-platform-linux/src/engine");
    let code = rust_sources(&engine);

    assert!(
        !code.contains("taskmanager_application"),
        "Linux engine code must import shared facts from taskmanager-core or taskmanager-platform-contract, not from the application use-case layer"
    );
}

#[test]
fn shared_layers_cannot_select_linux_io_or_hardware_vendor_binaries() {
    let repository = repository();
    for package in [
        "taskmanager-core",
        "taskmanager-telemetry-store",
        "taskmanager-history-store",
        "taskmanager-platform-contract",
        "taskmanager-application",
        "taskmanager-platform-portable",
        "taskmanager-platform-provider",
        "taskmanager-platform-runtime",
        "taskmanager-ui-contract",
    ] {
        let root = repository.join("crates").join(package);
        let code = rust_code_without_line_comments(&root.join("src"));
        for forbidden in [
            "\"/proc/",
            "\"/sys/",
            "std::process::Command",
            "\"systemctl\"",
            "\"journalctl\"",
            "\"rc-service\"",
            "\"smartctl\"",
            "\"nvme\"",
            "\"nvidia-smi\"",
            "\"rocm-smi\"",
            "\"intel_gpu_top\"",
        ] {
            assert!(
                !code.contains(forbidden),
                "{package} leaked native path, command, or vendor binary selection: {forbidden}"
            );
        }
        assert!(
            !contains_command_constructor(&code),
            "{package} selected a native command with an independent `Command::new` call"
        );
        assert!(
            !code.contains("cfg(target_os") && !code.contains("cfg!(target_os"),
            "{package} must not select an OS inside a shared layer"
        );

        let manifest = read_source(&repository, &format!("crates/{package}/Cargo.toml"));
        for forbidden_feature in ["hardware-all =", "nvidia =", "amd =", "intel ="] {
            assert!(
                !manifest.contains(forbidden_feature),
                "{package} must not expose a hardware-vendor product feature: {forbidden_feature}"
            );
        }
    }
}

#[test]
fn gpui_frontend_never_reaches_platform_adapter_crates() {
    // ADR-005: frontends consume application/ui-contract, never platform
    // adapters. The root Cargo.toml is the only place wired to
    // taskmanager-app-host is the shared composition edge and is the only
    // crate that names taskmanager-platform-native. Every UI source must
    // route platform behavior through taskmanager_application /
    // taskmanager_ui_contract / taskmanager_core.
    let repository = repository();
    let mut code =
        rust_code_without_line_comments(&repository.join("crates/taskmanager-gpui/src/gpui_app"));
    for entry in
        fs::read_dir(repository.join("src")).expect("top-level src directory should be readable")
    {
        let path = entry.expect("directory entry should be readable").path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && path != repository.join("src/main.rs")
        {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            code.push_str(&strip_line_comments(&source));
        }
    }

    for forbidden in [
        "taskmanager_platform_linux",
        "taskmanager_platform_native",
        "taskmanager_platform_runtime",
        "taskmanager_platform_provider",
        "taskmanager_platform_contract",
        "taskmanager_ebpf_",
    ] {
        assert!(
            !code.contains(forbidden),
            "GPUI frontend reached a platform-adapter crate ({forbidden}); UI sources must cross \
             taskmanager_application / taskmanager_ui_contract / taskmanager_core, and only \
             src/main.rs may wire the native adapter"
        );
    }
}

#[test]
fn standard_artifacts_enable_hardware_all_without_vendor_skus() {
    let repository = repository();
    let root_manifest = read_source(&repository, "Cargo.toml");
    let native_manifest = read_source(&repository, "crates/taskmanager-platform-native/Cargo.toml");
    let linux_manifest = read_source(&repository, "crates/taskmanager-platform-linux/Cargo.toml");
    let macos_manifest = read_source(&repository, "crates/taskmanager-platform-macos/Cargo.toml");
    let windows_manifest = read_source(
        &repository,
        "crates/taskmanager-platform-windows/Cargo.toml",
    );
    assert!(root_manifest.contains("default = [\"hardware-all\", \"ui-gpui\"]"));
    assert!(root_manifest.contains("hardware-all = [\"taskmanager-app-host/hardware-all\"]"));
    assert!(native_manifest.contains("default = [\"hardware-all\"]"));
    for adapter_feature in [
        "taskmanager-platform-linux/hardware-all",
        "taskmanager-platform-macos/hardware-all",
        "taskmanager-platform-windows/hardware-all",
    ] {
        assert!(
            native_manifest.contains(adapter_feature),
            "native standard artifact omitted {adapter_feature}"
        );
    }
    assert!(linux_manifest.contains("default = [\"hardware-all\"]"));
    assert!(linux_manifest.contains("hardware-all = [\"nvidia\"]"));
    for (adapter, manifest) in [
        ("macOS", macos_manifest.as_str()),
        ("Windows", windows_manifest.as_str()),
    ] {
        assert!(
            manifest.contains("default = [\"hardware-all\"]"),
            "{adapter} standard artifact must enable hardware-all"
        );
        assert!(
            manifest.contains("hardware-all = []"),
            "{adapter} must represent its currently empty provider registry honestly"
        );
    }
    assert!(
        !linux_manifest
            .lines()
            .any(|line| line.trim_start().starts_with("amd ="))
    );
    assert!(
        !linux_manifest
            .lines()
            .any(|line| line.trim_start().starts_with("intel ="))
    );

    // Runtime provider registration is behavior-covered by the Linux adapter's
    // provider tests; this gate only pins the manifest feature surface.
}
