//! Contract-tag authority tests (P4 evidence closure): the Rust
//! [`ContractTag`] enum is the single source for the committed manifest's
//! `contract_tag` column.
//!
//! The manifest is a textual artifact whose contract is the text itself, so
//! reading it here is a mechanical consistency check, not a source-inspection
//! test: we never read production Rust source and never prove behavior from
//! source text.

use super::*;

/// The committed declaration manifest, embedded at compile time so the check
/// cannot silently pass by reading a stale or missing file.
const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/parity/cross_frontend_manifest.tsv"
));

/// Extract the `contract_tag` column from manifest text.
///
/// The column is located by header name, so column reordering cannot silently
/// change what is validated. Comment and blank lines are skipped, matching the
/// resolver's parsing contract.
fn manifest_contract_tags(manifest: &str) -> Vec<&str> {
    let mut lines = manifest.lines();
    let header = loop {
        match lines.next() {
            Some(line) if !line.trim().is_empty() && !line.trim_start().starts_with('#') => {
                break line;
            }
            Some(_) => continue,
            None => return Vec::new(),
        }
    };
    let tag_index = header
        .split('\t')
        .position(|field| field.trim() == "contract_tag")
        .expect("manifest header must declare a contract_tag column");
    lines
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split('\t').nth(tag_index))
        .map(str::trim)
        .collect()
}

/// `ALL` is total and duplicate-free: non-empty, every id unique and non-empty,
/// and the pinned count makes adding or dropping a tag a conscious change.
#[test]
fn all_contract_tags_are_unique_and_named() {
    assert_eq!(ContractTag::ALL.len(), 28);
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

/// The column is found by header name, not by position.
#[test]
fn parser_locates_the_tag_column_by_header() {
    let synthetic = "contract_tag\tsubject_id\nchart\tx\n";
    assert_eq!(manifest_contract_tags(synthetic), vec!["chart"]);
}
