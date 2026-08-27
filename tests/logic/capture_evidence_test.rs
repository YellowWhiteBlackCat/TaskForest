use std::process::Command;

/// These evidence/quality gates shell out to `timeout … python3 <repo-script>`.
/// They are meaningful on developer machines and the Linux CI image (which
/// ships python3), but they must not turn a build RED on a host that lacks the
/// interpreter — a minimal container, a non-Linux runner, or a chroot without
/// `timeout`/`python3`. Probe both binaries once; when either is absent the
/// caller passes with a note instead of panicking inside `Command::output()`.
fn external_validator_available() -> bool {
    let python = Command::new("python3").arg("--version").output();
    let timeout = Command::new("timeout").arg("--version").output();
    matches!(python, Ok(out) if out.status.success())
        && matches!(timeout, Ok(out) if out.status.success())
}

#[test]
fn niri_capture_disables_the_startup_overlay_and_validates_its_config() {
    let script = include_str!("../../scripts/capture-niri.sh");
    let validator = include_str!("../../scripts/validate_capture_evidence.py");
    assert!(script.contains("hotkey-overlay {\n    skip-at-startup\n}"));
    assert!(script.contains("niri validate --config \"$CONF\""));
    assert!(script.contains("XDG_CONFIG_HOME=\"$config_home\""));
    assert!(script.contains("CAPTURE_MARKER event=theme_ready"));
    assert!(validator.contains("CAPTURE_MARKER event=theme_ready"));
}

#[test]
fn host_wayland_diagnostic_is_non_publishing_and_has_strict_receipts() {
    let script = include_str!("../../scripts/capture-host-wayland-diagnostic.sh");
    let validator = include_str!("../../scripts/validate_host_wayland_diagnostic.py");
    assert!(script.contains("target/host-wayland-diagnostic"));
    assert!(script.contains("agent-workdir.sh\" enter host-wayland-diagnostic"));
    assert!(script.contains("spectacle --activewindow"));
    assert!(script.contains("app_pid_exe_verified=true"));
    assert!(script.contains("parity_evidence=false"));
    assert!(script.contains("durable_output=none"));
    assert!(!script.contains("docs/screenshots/"));
    assert!(validator.contains("display scale"));
    assert!(validator.contains("skeleton text detected by OCR"));
    assert!(validator.contains("current binary hash differs from capture receipt"));

    if !external_validator_available() {
        eprintln!(
            "skipping host_wayland_diagnostic_validator_self_test: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = match Command::new("timeout")
        .args([
            "30s",
            "python3",
            "scripts/validate_host_wayland_diagnostic.py",
            "--self-test",
        ])
        .current_dir(repository)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("skipping host diagnostic validator self-test: {error}");
            return;
        }
    };
    assert!(
        output.status.success(),
        "host diagnostic validator self-test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn tui_capture_script_keeps_required_evidence_markers() {
    let script = include_str!("../../scripts/capture-tui.sh");
    assert!(script.contains("hotkey-overlay {\n    skip-at-startup\n}"));
    assert!(script.contains("niri validate --config \"$CONF\""));
    assert!(script.contains("TM_TUI_CAPTURE_MARKER_FILE"));
    assert!(script.contains("alacritty --class taskmanager-tui"));
    assert!(script.contains("--source-manifest \"$SOURCE_MANIFEST\""));
    // Receipt freshness (committed source-manifest hashes vs current sources)
    // is enforced by the capture flow itself: `scripts/capture-tui.sh` runs
    // `validate_tui_evidence.py` against the freshly captured artifacts, and
    // `--with-gui` / ui-route demands a fresh receipt. The default test suite
    // must not compare committed receipts, or every production change would
    // redden ordinary `cargo test` runs until a Wayland capture is re-run.
}

#[test]
fn frontend_source_manifests_are_scoped_to_the_selected_shape() {
    if !external_validator_available() {
        eprintln!(
            "skipping frontend_source_manifests_are_scoped_to_the_selected_shape: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output_path = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-frontend-scope-{}.sha256",
        std::process::id()
    ));
    for frontend in ["tui", "iced", "gpui"] {
        let output = Command::new("python3")
            .args([
                "scripts/frontend_source_manifest.py",
                "--frontend",
                frontend,
                "--repo-root",
                repository,
                "--output",
            ])
            .arg(&output_path)
            .current_dir(repository)
            .output()
            .expect("python3 must be available for the frontend scope manifest");
        assert!(
            output.status.success(),
            "frontend scope generation failed for {frontend}:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest = std::fs::read_to_string(&output_path)
            .expect("frontend source manifest must be readable");
        match frontend {
            "tui" => {
                assert!(manifest.contains("crates/taskmanager-tui/src/"));
                assert!(!manifest.contains("crates/taskmanager-gpui/src/gpui_app/"));
                assert!(!manifest.contains("crates/taskmanager-gpui/src/"));
                assert!(!manifest.contains("src/frontend/iced.rs"));
            }
            "iced" => {
                assert!(manifest.contains("crates/taskmanager-iced/src/"));
                assert!(!manifest.contains("crates/taskmanager-gpui/src/gpui_app/"));
                assert!(!manifest.contains("crates/taskmanager-gpui/src/"));
                assert!(!manifest.contains("src/frontend/tui.rs"));
            }
            "gpui" => {
                assert!(manifest.contains("crates/taskmanager-gpui/src/"));
                assert!(manifest.contains("crates/taskmanager-gpui/src/gpui_app/"));
                assert!(!manifest.contains("crates/taskmanager-tui/src/"));
                assert!(!manifest.contains("crates/taskmanager-iced/src/"));
            }
            _ => unreachable!("the table above is exhaustive"),
        }
    }
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn tui_evidence_validator_rejects_uniform_black_frames() {
    if !external_validator_available() {
        eprintln!(
            "skipping tui_evidence_validator_rejects_uniform_black_frames: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("timeout")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .args([
            "30s",
            "python3",
            "scripts/validate_tui_evidence.py",
            "--self-test",
        ])
        .current_dir(repository)
        .output()
        .expect("python3 must be available for the TUI evidence validator");

    assert!(
        output.status.success(),
        "TUI visual-content self-test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn capture_evidence_validator_rejects_corrupt_png_receipts() {
    if !external_validator_available() {
        eprintln!(
            "skipping capture_evidence_validator_rejects_corrupt_png_receipts: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("timeout")
        .args([
            "30s",
            "python3",
            "scripts/validate_capture_evidence.py",
            "--self-test",
        ])
        .current_dir(repository)
        .output()
        .expect("python3 must be available for the screenshot evidence validator");

    assert!(
        output.status.success(),
        "validator self-test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn rust_line_guard_counts_code_instead_of_comments() {
    if !external_validator_available() {
        eprintln!(
            "skipping rust_line_guard_counts_code_instead_of_comments: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("timeout")
        .args([
            "30s",
            "python3",
            "scripts/quality/rust_line_guard.py",
            "--self-test",
        ])
        .current_dir(repository)
        .output()
        .expect("python3 must be available for the Rust line guard");

    assert!(
        output.status.success(),
        "line-guard self-test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn automation_safety_guard_rejects_the_runaway_process_pattern() {
    if !external_validator_available() {
        eprintln!(
            "skipping automation_safety_guard_rejects_the_runaway_process_pattern: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("timeout")
        .args([
            "30s",
            "python3",
            "scripts/quality/automation_safety_guard.py",
            "--self-test",
        ])
        .current_dir(repository)
        .output()
        .expect("timeout and python3 must be available for the automation safety guard");

    assert!(
        output.status.success(),
        "automation safety self-test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn repository_automation_passes_the_liveness_guard() {
    if !external_validator_available() {
        eprintln!(
            "skipping repository_automation_passes_the_liveness_guard: \
             timeout/python3 unavailable in this environment"
        );
        return;
    }
    let repository = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("timeout")
        .args([
            "30s",
            "python3",
            "scripts/quality/automation_safety_guard.py",
            "--repo-root",
            repository,
        ])
        .current_dir(repository)
        .output()
        .expect("timeout and python3 must be available for the automation safety guard");

    assert!(
        output.status.success(),
        "repository automation safety guard failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
