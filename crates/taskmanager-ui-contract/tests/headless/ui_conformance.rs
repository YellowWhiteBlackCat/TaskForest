//! Contract-tag authority tests (P4 evidence closure): the Rust
//! [`ContractTag`] enum is the single source for the committed declaration
//! files' contract vocabulary — the facet manifest's `contract_tag` column and
//! the unified interaction matrix's `contract_tag` and `paths` columns.
//!
//! These files are textual artifacts whose contract is the text itself, so
//! reading them here is a mechanical consistency check, not a source-inspection
//! test: we never read production Rust source and never prove behavior from
//! source text.

use super::*;

/// The committed declaration manifest, embedded at compile time so the check
/// cannot silently pass by reading a stale or missing file.
const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/parity/cross_frontend_manifest.tsv"
));

/// The committed unified interaction matrix (S4), embedded for the same reason
/// as [`MANIFEST`].  It is the second declaration source that references the
/// Rust vocabulary; the resolver owns no tag set of its own.
const MATRIX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/parity/cross_frontend_matrix.tsv"
));

/// Extract one column from committed TSV text.
///
/// The column is located by header name, so column reordering cannot silently
/// change what is validated. Comment and blank lines are skipped, matching the
/// resolver's parsing contract.
fn column_values<'a>(table: &'a str, column: &str) -> Vec<&'a str> {
    let mut lines = table.lines();
    let header = loop {
        match lines.next() {
            Some(line) if !line.trim().is_empty() && !line.trim_start().starts_with('#') => {
                break line;
            }
            Some(_) => continue,
            None => return Vec::new(),
        }
    };
    let column_index = header
        .split('\t')
        .position(|field| field.trim() == column)
        .unwrap_or_else(|| panic!("committed table header must declare a {column} column"));
    lines
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split('\t').nth(column_index))
        .map(str::trim)
        .collect()
}

/// Extract the `contract_tag` column from manifest text.
fn manifest_contract_tags(manifest: &str) -> Vec<&str> {
    column_values(manifest, "contract_tag")
}

/// Every token of the unified matrix's `paths` column, in declaration order.
fn matrix_path_tokens(matrix: &str) -> Vec<&str> {
    column_values(matrix, "paths")
        .into_iter()
        .flat_map(|paths| paths.split('|'))
        .filter(|token| !token.is_empty())
        .collect()
}

/// `ALL` is total and duplicate-free: non-empty, every id unique and non-empty,
/// and the pinned count makes adding or dropping a tag a conscious change.
#[test]
fn all_contract_tags_are_unique_and_named() {
    assert_eq!(ContractTag::ALL.len(), 37);
    let mut ids: Vec<_> = ContractTag::ALL.iter().map(|tag| tag.id()).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "contract tag ids must be unique");
    assert!(
        ContractTag::ALL.iter().all(|tag| !tag.id().is_empty()),
        "every contract tag names a stable machine id"
    );
}

/// The id is the contract: it round-trips through `from_id`, and an unrelated
/// string does not resolve.
#[test]
fn id_round_trips_through_from_id() {
    for tag in ContractTag::ALL {
        assert_eq!(ContractTag::from_id(tag.id()), Some(tag));
    }
    assert_eq!(ContractTag::from_id("not-a-contract-tag"), None);
}

/// Every `contract_tag` in the committed manifest is a known [`ContractTag`].
/// This is the mechanical validator that lets the Python resolver drop its
/// local vocabulary: an unknown tag fails here, in Rust.
#[test]
fn every_manifest_contract_tag_is_known() {
    let tags = manifest_contract_tags(MANIFEST);
    assert!(
        !tags.is_empty(),
        "the committed manifest must declare at least one row"
    );
    for tag in tags {
        assert!(
            ContractTag::from_id(tag).is_some(),
            "manifest declares unknown contract_tag {tag:?}"
        );
    }
}

/// The validator is not a rubber stamp: a synthetic manifest carrying an
/// unknown tag is reported, so deleting this check would be a visible
/// regression rather than a silent one.
#[test]
fn unknown_contract_tag_is_rejected() {
    let synthetic = concat!(
        "subject_kind\tsubject_id\tfrontend\tstatus\treason\tcontract_tag\t",
        "evidence_kind\ttest_id_or_scenario\ttarget_or_validator\n",
        "facet\tx\tgpui\tready\t\tnot-a-contract-tag\tnone\t-\t-\n",
    );
    let unknown: Vec<_> = manifest_contract_tags(synthetic)
        .into_iter()
        .filter(|tag| ContractTag::from_id(tag).is_none())
        .collect();
    assert_eq!(
        unknown,
        vec!["not-a-contract-tag"],
        "the validator must reject an unknown contract tag"
    );
}

/// Every `contract_tag` and every `paths` token in the committed unified
/// interaction matrix (S4) is a known [`ContractTag`].  The matrix is the
/// second declaration source referencing the vocabulary; validating both
/// columns here keeps the Rust enum the single authority and covers the path
/// tokens that never lead a row (so they cannot drift as an unowned copy).
#[test]
fn every_matrix_contract_tag_is_known() {
    let tags = column_values(MATRIX, "contract_tag");
    assert!(
        !tags.is_empty(),
        "the committed unified matrix must declare at least one row"
    );
    for tag in tags {
        assert!(
            ContractTag::from_id(tag).is_some(),
            "unified matrix declares unknown contract_tag {tag:?}"
        );
    }
    let tokens = matrix_path_tokens(MATRIX);
    assert!(
        !tokens.is_empty(),
        "the committed unified matrix must declare at least one path token"
    );
    for token in tokens {
        assert!(
            ContractTag::from_id(token).is_some(),
            "unified matrix declares unknown paths token {token:?}"
        );
    }
}

/// The matrix validator is not a rubber stamp either: a synthetic matrix row
/// carrying an unknown `paths` token is reported, and the row's primary
/// `contract_tag` cannot mask it.
#[test]
fn unknown_matrix_path_token_is_rejected() {
    let synthetic = concat!(
        "subject_kind\tcase_id\tfrontend\tp0_id\ttarget\ttest_name\tpaths\t",
        "capture_scenarios\tcontract_tag\tplatform\n",
        "interaction\tcase-x\tgpui\t-\tlib\t-\tchart|not-a-contract-tag\t-\tchart\t\n",
    );
    let unknown: Vec<_> = matrix_path_tokens(synthetic)
        .into_iter()
        .filter(|token| ContractTag::from_id(token).is_none())
        .collect();
    assert_eq!(
        unknown,
        vec!["not-a-contract-tag"],
        "the matrix validator must reject an unknown path token"
    );
}

/// The column is found by header name, not by position.
#[test]
fn parser_locates_the_tag_column_by_header() {
    let synthetic = "contract_tag\tsubject_id\nchart\tx\n";
    assert_eq!(manifest_contract_tags(synthetic), vec!["chart"]);
}
