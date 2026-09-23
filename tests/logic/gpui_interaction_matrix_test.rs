//! Structural gate for the GPUI projection of the cross-frontend interaction
//! contract.
//!
//! The actual behavior is proved by GPUI tests that advertise each stable case
//! ID as a function-name prefix. This gate keeps the GPUI rows of the unified
//! matrix synchronized with the public interaction requirements and the
//! canonical screenshot scenarios; the acceptance script then auto-discovers the
//! executable tests through nextest and verifies every discovered case received
//! an `ok` event.
//!
//! Since the S5 retirement the structural authority is the unified
//! `scripts/parity/cross_frontend_matrix.tsv`, filtered to `frontend == "gpui"`.
//! The per-frontend `scripts/gpui_interaction_matrix.tsv` is gone: the accept
//! chain reads the unified matrix too (D6 phases 3-4).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Unified interaction matrix header (`scripts/parity/cross_frontend_matrix.tsv`).
const MATRIX_HEADER: &str = "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\tcapture_scenarios\tcontract_tag\tplatform";
const CAPTURE_HEADER: &str =
    "name\tskin\tpage\tdevice\tsettings\tscenario\twindow_size\tcapture_size";

/// Column positions in the unified matrix, kept in header order.
const CASE_ID: usize = 1;
const FRONTEND: usize = 2;
const P0_ID: usize = 3;
const TARGET: usize = 4;
const PATHS: usize = 6;
const CAPTURES: usize = 7;

/// The GPUI projection is pinned: adding or dropping a row is a conscious
/// change, so a silently dropped case cannot hide behind a redundant sibling.
const GPUI_ROW_COUNT: usize = 39;

/// Interaction path tokens accepted on a GPUI row. The vocabulary authority is
/// the Rust `ContractTag` enum (its conformance test validates every unified
/// matrix token); this list is the stricter interaction channel the GPUI rows
/// must stay inside.
const GPUI_PATH_TOKENS: &[&str] = &[
    "cancel",
    "evidence",
    "failure",
    "focus",
    "isolation",
    "keyboard",
    "lifecycle",
    "pointer",
    "provider-gap",
    "recovery",
    "responsive",
    "success",
    "toggle",
];

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

fn unified_matrix_document() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/parity/cross_frontend_matrix.tsv");
    fs::read_to_string(path).expect("unified interaction matrix must exist")
}

/// The stable case-id convention the retired GPUI validator enforced:
/// `mc` + exactly two digits + one or more `-`-separated lowercase-alphanumeric
/// segments. The Rust gate now owns this discipline.
fn is_stable_case_id(case_id: &str) -> bool {
    let Some(body) = case_id.strip_prefix("mc") else {
        return false;
    };
    let mut characters = body.chars();
    let (Some(first), Some(second), Some('-')) =
        (characters.next(), characters.next(), characters.next())
    else {
        return false;
    };
    if !first.is_ascii_digit() || !second.is_ascii_digit() {
        return false;
    }
    let remainder = characters.as_str();
    !remainder.is_empty()
        && remainder.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

/// Validate the GPUI projection of a unified-matrix document.
///
/// Kept as a function so the negative tests below can exercise it directly
/// against synthetic documents: the gate is not a rubber stamp.
fn check_gpui_projection(document: &str) {
    let mut lines = non_comment_lines(document);
    assert_eq!(
        lines.next(),
        Some(MATRIX_HEADER),
        "unified interaction matrix schema changed without updating the gate"
    );

    let requirements = requirement_ids();
    let capture_scenarios = capture_scenario_ids();
    let mut cases = BTreeSet::new();
    let mut cases_by_target: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut covered = BTreeSet::new();
    let mut success = BTreeSet::new();

    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            10,
            "malformed unified interaction row: {line}"
        );
        if fields[FRONTEND] != "gpui" {
            continue;
        }
        let case_id = fields[CASE_ID];
        let p0_id = fields[P0_ID];
        let target = fields[TARGET];
        let paths = fields[PATHS];
        let captures = fields[CAPTURES];
        assert!(
            cases.insert(case_id),
            "duplicate interaction case: {case_id}"
        );
        assert!(
            requirements.contains(p0_id),
            "unknown requirement ID: {p0_id}"
        );
        assert!(
            matches!(target, "gui" | "lib"),
            "invalid test target: {target}"
        );
        assert!(
            is_stable_case_id(case_id),
            "interaction case IDs must be stable lowercase kebab-case: {case_id}"
        );
        assert!(
            cases_by_target
                .entry(target)
                .or_default()
                .insert(case_id.to_string()),
            "duplicate interaction case in target {target}: {case_id}"
        );

        let path_names: BTreeSet<_> = paths.split('|').filter(|path| !path.is_empty()).collect();
        assert!(
            !path_names.is_empty(),
            "interaction paths must not be empty: {line}"
        );
        assert!(
            path_names
                .iter()
                .all(|path| GPUI_PATH_TOKENS.contains(path)),
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
        covered.insert(p0_id.to_string());
        if path_names.contains("success") {
            success.insert(p0_id.to_string());
        }
    }

    assert_eq!(
        cases.len(),
        GPUI_ROW_COUNT,
        "GPUI projection row count changed; adding or dropping a case is a conscious change"
    );
    assert_eq!(
        covered, requirements,
        "every interaction requirement needs a matrix case"
    );
    assert_eq!(
        success, requirements,
        "every interaction requirement needs a success path"
    );
    assert!(
        cases_by_target.values().all(|cases| !cases.is_empty()),
        "the matrix must contain at least one GUI or library case"
    );
}

#[test]
fn gpui_interaction_matrix_covers_every_parity_row_and_capture_token() {
    check_gpui_projection(&unified_matrix_document());
}

/// A GPUI row whose `paths` token is not in the interaction vocabulary is
/// rejected (the Rust `ContractTag` conformance owns the wider tag set).
#[test]
#[should_panic(expected = "unknown interaction path in row")]
fn unknown_gpui_path_token_is_rejected() {
    let synthetic = concat!(
        "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\t",
        "capture_scenarios\tcontract_tag\tplatform\n",
        "interaction\tmc00-page-sweep\tgpui\tP0-MC-00\tgui\t-\t",
        "success|not-a-contract-tag\t-\tsuccess\t\n",
    );
    check_gpui_projection(synthetic);
}

/// A GPUI row whose case id breaks the stable kebab-case prefix convention is
/// rejected.
#[test]
#[should_panic(expected = "stable lowercase kebab-case")]
fn invalid_gpui_case_prefix_is_rejected() {
    let synthetic = concat!(
        "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\t",
        "capture_scenarios\tcontract_tag\tplatform\n",
        "interaction\tNotStable\tgpui\tP0-MC-00\tgui\t-\tsuccess\t-\tsuccess\t\n",
    );
    check_gpui_projection(synthetic);
}

/// A case id that starts with `mc` but skips the two-digit sequence is still
/// rejected: the discipline the retired validator enforced is not weakened.
#[test]
#[should_panic(expected = "stable lowercase kebab-case")]
fn non_two_digit_gpui_case_id_is_rejected() {
    let synthetic = concat!(
        "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\t",
        "capture_scenarios\tcontract_tag\tplatform\n",
        "interaction\tmc0-page-sweep\tgpui\tP0-MC-00\tgui\t-\tsuccess\t-\tsuccess\t\n",
    );
    check_gpui_projection(synthetic);
}

/// Dropping a GPUI row fails even when a sibling still covers the requirement:
/// the pinned count is the guarantee the coverage equality cannot give.
#[test]
#[should_panic(expected = "GPUI projection row count changed")]
fn dropped_gpui_row_is_rejected() {
    let document = unified_matrix_document();
    let trimmed: String = document
        .lines()
        .filter(|line| !line.contains("\tmc00-nav-pointer\t"))
        .map(|line| format!("{line}\n"))
        .collect();
    check_gpui_projection(&trimmed);
}
