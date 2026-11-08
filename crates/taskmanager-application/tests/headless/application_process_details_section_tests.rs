//! test-intent: behavior
//! The shared process-properties section vocabulary: one variant set, one
//! order, one token per section and one catalog key per section, plus the
//! i18n owner's resolver over that key.

use super::*;
use crate::i18n::{Language, process_details_section_label, set_language};

#[test]
fn the_section_set_and_its_order_are_declared_once() {
    assert_eq!(
        ProcessDetailsSection::ALL,
        [
            ProcessDetailsSection::Overview,
            ProcessDetailsSection::Performance,
            ProcessDetailsSection::Command,
            ProcessDetailsSection::Insights,
        ]
    );
    assert_eq!(
        ProcessDetailsSection::default(),
        ProcessDetailsSection::Overview
    );
}

#[test]
fn every_section_carries_one_stable_token_and_one_catalog_key() {
    let tokens: Vec<&str> = ProcessDetailsSection::ALL
        .iter()
        .map(|section| section.token())
        .collect();
    assert_eq!(tokens, ["overview", "performance", "command", "insights"]);

    let keys: Vec<&str> = ProcessDetailsSection::ALL
        .iter()
        .map(|section| section.i18n_key())
        .collect();
    assert_eq!(
        keys,
        [
            "prop.overview",
            "prop.performance",
            "prop.command",
            "prop.insights"
        ]
    );
}

#[test]
fn the_cycle_walks_the_canonical_order_in_both_directions() {
    let forward: Vec<ProcessDetailsSection> = ProcessDetailsSection::ALL
        .iter()
        .map(|section| section.next())
        .collect();
    assert_eq!(
        forward,
        [
            ProcessDetailsSection::Performance,
            ProcessDetailsSection::Command,
            ProcessDetailsSection::Insights,
            ProcessDetailsSection::Overview,
        ]
    );

    let backward: Vec<ProcessDetailsSection> = ProcessDetailsSection::ALL
        .iter()
        .map(|section| section.prev())
        .collect();
    assert_eq!(
        backward,
        [
            ProcessDetailsSection::Insights,
            ProcessDetailsSection::Overview,
            ProcessDetailsSection::Performance,
            ProcessDetailsSection::Command,
        ]
    );
}

#[test]
fn the_i18n_owner_resolves_every_section_label() {
    set_language(Language::En);
    for section in ProcessDetailsSection::ALL {
        let label = process_details_section_label(section);
        assert!(
            !label.is_empty(),
            "{} must resolve a label",
            section.token()
        );
    }
    assert_eq!(
        process_details_section_label(ProcessDetailsSection::Overview),
        "Overview"
    );

    set_language(Language::Zh);
    assert_eq!(
        process_details_section_label(ProcessDetailsSection::Overview),
        "概览"
    );

    set_language(Language::En);
}
