//! Structural gate for the GPUI interaction acceptance contract.
//!
//! The actual behavior is proved by the GPUI tests named in the matrix. This
//! gate keeps the matrix synchronized with the public interaction requirements
//! and the canonical screenshot scenarios; the acceptance script then
//! resolves every test name through nextest and verifies an `ok` event.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

const MATRIX_HEADER: &str = "case_id\tp0_id\ttarget\ttest_name\tpaths\tcapture_scenarios";
const CAPTURE_HEADER: &str =
    "name\tskin\tpage\tdevice\tsettings\tscenario\twindow_size\tcapture_size";

fn non_comment_lines(document: &str) -> impl Iterator<Item = &str> {
    document
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
}

fn requirement_ids() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/interaction_requirements.tsv");
    let document = fs::read_to_string(path).expect("interaction requirements must exist");
    let mut lines = non_comment_lines(&document);
    assert_eq!(
        lines.next(),
        Some("requirement_id"),
        "interaction requirements schema changed without updating the gate"
    );
    lines.map(str::to_string).collect()
}

fn capture_scenario_ids() -> BTreeSet<String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/capture_scenarios.tsv");
    let document = fs::read_to_string(path).expect("capture scenario matrix must exist");
    let mut lines = non_comment_lines(&document);
    assert_eq!(
        lines.next(),
        Some(CAPTURE_HEADER),
        "capture matrix schema changed without updating the acceptance gate"
    );
    lines
        .filter_map(|line| line.split('\t').next())
        .map(str::to_string)
        .collect()
}

#[test]
fn gpui_interaction_matrix_covers_every_parity_row_and_capture_token() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/gpui_interaction_matrix.tsv");
    let document = fs::read_to_string(path).expect("GPUI interaction matrix must exist");
    let mut lines = non_comment_lines(&document);
    assert_eq!(
        lines.next(),
        Some(MATRIX_HEADER),
        "GPUI interaction matrix schema changed without updating the gate"
    );

    let requirements = requirement_ids();
    let capture_scenarios = capture_scenario_ids();
    let mut cases = BTreeSet::new();
    let mut tests_by_target: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut covered = BTreeSet::new();
    let mut success = BTreeSet::new();

    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6, "malformed GPUI interaction row: {line}");
        let [case_id, p0_id, target, test_name, paths, captures] = fields.as_slice() else {
            unreachable!("field count checked above");
        };
        assert!(
            cases.insert(*case_id),
            "duplicate interaction case: {case_id}"
        );
        assert!(
            requirements.contains(*p0_id),
            "unknown requirement ID: {p0_id}"
        );
        assert!(
            matches!(*target, "gui" | "lib"),
            "invalid test target: {target}"
        );
        assert!(
            !test_name.is_empty(),
            "interaction test name must not be empty"
        );
        assert!(
            tests_by_target
                .entry(target)
                .or_default()
                .insert((*test_name).to_string()),
            "duplicate interaction test in target {target}: {test_name}"
        );

        let path_names: BTreeSet<_> = paths.split('|').filter(|path| !path.is_empty()).collect();
        assert!(
            !path_names.is_empty(),
            "interaction paths must not be empty: {line}"
        );
        assert!(
            path_names.iter().all(|path| {
                matches!(
                    *path,
                    "cancel"
                        | "evidence"
                        | "failure"
                        | "focus"
                        | "isolation"
                        | "keyboard"
                        | "lifecycle"
                        | "pointer"
                        | "provider-gap"
                        | "recovery"
                        | "responsive"
                        | "success"
                        | "toggle"
                )
            }),
            "unknown interaction path in row: {line}"
        );
        let scenario_names = captures
            .split('|')
            .filter(|scenario| !scenario.is_empty() && *scenario != "-");
        for scenario in scenario_names {
            assert!(
                capture_scenarios.contains(scenario),
                "unknown capture scenario {scenario} in row: {line}"
            );
        }
        covered.insert((*p0_id).to_string());
        if path_names.contains("success") {
            success.insert((*p0_id).to_string());
        }
    }

    assert_eq!(
        covered, requirements,
        "every interaction requirement needs a matrix case"
    );
    assert_eq!(
        success, requirements,
        "every interaction requirement needs a success path"
    );
    assert!(
        tests_by_target.values().all(|tests| !tests.is_empty()),
        "the matrix must contain at least one GUI or library test"
    );
}
