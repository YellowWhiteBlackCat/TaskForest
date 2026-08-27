//! source-inspection: static-policy
//!
//! Architecture test for the per-feature privilege-escalation seam
//! (`crates/taskmanager-escalation`, ADR-023, permission-model Boundary 2
//! operationalized).
//!
//! This is a CI contract, not a claim. The crate is a pure-safe-Rust leaf with
//! zero dependencies, so the checks are structural/source-level (consistent
//! with every other test in this suite) and complement the crate's own unit
//! tests, which execute `UnprivilegedGate::probe` for each variant. Adding a
//! new escalation-column feature MUST land in the enum, in
//! `docs/PERMISSION_MODEL.md` Boundary 3, and in `EscalationFeature::ALL`.

use std::fs;

use super::repository;

fn escalation_manifest() -> String {
    fs::read_to_string(repository().join("crates/taskmanager-escalation/Cargo.toml"))
        .expect("taskmanager-escalation Cargo.toml should be readable")
}

fn escalation_lib() -> String {
    fs::read_to_string(repository().join("crates/taskmanager-escalation/src/lib.rs"))
        .expect("taskmanager-escalation src/lib.rs should be readable")
}

/// Strip `//` line comments so doc/comment text mentioning "Available" or
/// "Denied" does not trip the gate-impl assertion. Same idiom the rest of the
/// suite uses.
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract the brace-balanced body of the `impl PrivilegeGate for
/// UnprivilegedGate` block from `lib`, starting at the impl's opening brace and
/// scanning to its matching close. Robust against reformatting inside the impl.
fn unprivileged_gate_impl_body(lib: &str) -> String {
    let after_header = lib
        .split("impl PrivilegeGate for UnprivilegedGate")
        .nth(1)
        .expect("UnprivilegedGate must implement PrivilegeGate");
    let open = after_header
        .find('{')
        .expect("UnprivilegedGate impl body must open with a brace");
    let mut depth = 0usize;
    let mut close = open;
    for (idx, ch) in after_header.char_indices().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = idx;
                    break;
                }
            }
            _ => {}
        }
    }
    after_header[open..=close].to_owned()
}

#[test]
fn escalation_seam_crate_exists_as_a_forbid_unsafe_leaf() {
    let manifest = escalation_manifest();
    let lib = escalation_lib();

    assert!(
        manifest.contains("name = \"taskmanager-escalation\""),
        "the escalation seam crate must be named taskmanager-escalation",
    );
    assert!(
        manifest.contains("edition = \"2024\""),
        "the escalation seam crate uses edition 2024",
    );
    // Boundary 1: business crates forbid unsafe. The seam is a business crate,
    // so it carries #![forbid(unsafe_code)] at its root.
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "taskmanager-escalation/src/lib.rs must carry #![forbid(unsafe_code)]",
    );
    // Boundary 2 default-unprivileged principle: the main app stays
    // unprivileged; the seam must not reach for an OS capability itself. The
    // crate doc legitimately NAMES pkexec/polkit as the planned helper path, so
    // strip comments first and check only production code for an actual grant
    // or invocation.
    let production = strip_line_comments(&lib);
    for capability in ["setcap", "setuid", "pkexec", "Command::new"] {
        assert!(
            !production.contains(capability),
            "the escalation seam must not grant or invoke a capability itself ({capability}); \
             it is only the boundary the future helper crosses",
        );
    }
}

#[test]
fn escalation_seam_exposes_the_documented_contract_types() {
    // The variant set comes from the crate's own `EscalationFeature::ALL`
    // (single source of truth); the probe behavior for every variant is
    // executed by the crate's unit tests.
    use taskmanager_escalation::EscalationFeature;
    assert_eq!(
        EscalationFeature::ALL.len(),
        7,
        "EscalationFeature::ALL must list every Boundary-3 escalation-column feature",
    );
}

#[test]
fn unprivileged_gate_probe_requires_escalation_for_every_feature() {
    // ADR-023 / Boundary 2 honest default: the app runs unprivileged, so
    // UnprivilegedGate::probe MUST report RequiresEscalation for EVERY variant
    // — never Available (fabricated access) and never Denied (hides a real
    // opportunity behind a hard refusal). This is the source-level CI gate; the
    // crate's own unit tests execute probe() for each variant and are the
    // authoritative behavioural assertion.
    let lib = escalation_lib();
    let impl_body = unprivileged_gate_impl_body(&lib);
    let code = strip_line_comments(&impl_body);

    assert!(
        !code.contains("::Available"),
        "UnprivilegedGate must never fabricate Available access to an escalation feature",
    );
    assert!(
        !code.contains("::Denied"),
        "UnprivilegedGate must not hard-deny an escalation feature; it must offer \
         RequiresEscalation so the UI can prompt",
    );
    // The gate must not branch on individual variants — the honest default is
    // uniform, so a probe of ANY feature yields RequiresEscalation for that
    // same feature. A per-variant `match` here would risk routing one feature
    // to Available/Denied.
    assert!(
        !code.contains("match "),
        "UnprivilegedGate::probe must be uniform (RequiresEscalation for every variant), \
         not branch per feature",
    );
}
