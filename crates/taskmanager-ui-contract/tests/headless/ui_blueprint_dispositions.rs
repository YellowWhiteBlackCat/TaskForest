use super::*;

fn findings_for(text: &'static str) -> Vec<BlueprintDispositionFinding> {
    BlueprintDispositionTable::parse(text).findings().to_vec()
}

#[test]
fn committed_ledger_is_complete_and_structurally_clean() {
    let table = BlueprintDispositionTable::committed();
    assert!(table.findings().is_empty(), "{:?}", table.findings());
    assert_eq!(table.rows().len(), SLOT_COUNT);
    assert_eq!(table.adopted_count(), 225);
    assert_eq!(table.unsupported_count(), 0);
    for area in FeatureArea::ALL {
        let rows: Vec<_> = table
            .rows()
            .iter()
            .filter(|row| row.area == *area)
            .collect();
        assert_eq!(rows.len(), ITEMS_PER_AREA, "{}", area.id());
        let expected = 15;
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row.disposition, BlueprintDisposition::Adopted(_)))
                .count(),
            expected,
            "{}: adopted slot census",
            area.id()
        );
    }
}

#[test]
fn adopted_slots_and_typed_features_agree_bidirectionally() {
    let table = BlueprintDispositionTable::committed();
    for feature in FeatureId::ALL {
        match feature.blueprint_slot() {
            Some(slot) => {
                let row = table
                    .slot(slot)
                    .unwrap_or_else(|| panic!("{} must disposition slot {slot}", feature.id()));
                assert_eq!(
                    row.disposition,
                    BlueprintDisposition::Adopted(*feature),
                    "{}",
                    feature.id()
                );
                assert_eq!(row.area, feature.area(), "{}", feature.id());
            }
            None => assert_eq!(
                *feature,
                FeatureId::MemoryProcessSwapCharge,
                "only the deliberate extra feature may sit outside the 225 slots"
            ),
        }
    }
    for row in table.rows() {
        if let BlueprintDisposition::Adopted(feature) = row.disposition {
            assert!(
                feature.blueprint_slot() == Some(row.slot)
                    || feature.blueprint_alias_slots().contains(&row.slot),
                "{} must own slot {} as its primary slot or a declared alias",
                feature.id(),
                row.slot
            );
        }
    }
}

#[test]
fn unsupported_rows_state_a_public_reason_and_claim_no_feature() {
    let table = BlueprintDispositionTable::committed();
    for row in table
        .rows()
        .iter()
        .filter(|row| row.disposition == BlueprintDisposition::Unsupported)
    {
        assert!(
            !row.reason.trim().is_empty() && row.reason != DASH,
            "slot {} must state why it is not delivered",
            row.slot
        );
    }
    for row in table
        .rows()
        .iter()
        .filter(|row| matches!(row.disposition, BlueprintDisposition::Adopted(_)))
    {
        assert_eq!(row.reason, DASH, "slot {}", row.slot);
    }
}

#[test]
fn parser_rejects_structural_damage() {
    let duplicate_slot = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tunsupported\t-\tnot delivered\n",
        "1\tprocess-lifecycle\tunsupported\t-\tnot delivered\n",
    );
    assert!(findings_for(duplicate_slot).iter().any(|finding| matches!(
        finding,
        BlueprintDispositionFinding::DuplicateSlot { slot: 1, .. }
    )));

    let malformed_area = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tmemory-forensics\tunsupported\t-\tnot delivered\n",
    );
    assert!(findings_for(malformed_area).iter().any(|finding| matches!(
        finding,
        BlueprintDispositionFinding::AreaMismatch { slot: 1, .. }
    )));

    let unknown_feature = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tadopted\tno.such.feature\t-\n",
    );
    assert!(
        findings_for(unknown_feature)
            .iter()
            .any(|finding| matches!(finding, BlueprintDispositionFinding::UnknownFeature { .. }))
    );

    let adopted_with_reason = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tadopted\tprocess.scheduler-policy\tstill shipping\n",
    );
    assert!(
        findings_for(adopted_with_reason)
            .iter()
            .any(|finding| matches!(
                finding,
                BlueprintDispositionFinding::AdoptedWithReason { slot: 1, .. }
            ))
    );

    let unsupported_with_feature = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tunsupported\tprocess.scheduler-policy\tnot delivered\n",
    );
    assert!(
        findings_for(unsupported_with_feature)
            .iter()
            .any(|finding| matches!(
                finding,
                BlueprintDispositionFinding::UnsupportedWithFeature { slot: 1, .. }
            ))
    );

    let missing_reason = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tunsupported\t-\t-\n",
    );
    assert!(findings_for(missing_reason).iter().any(|finding| matches!(
        finding,
        BlueprintDispositionFinding::MissingReason { slot: 1, .. }
    )));

    let unknown_disposition = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tdeferred\t-\tnot delivered\n",
    );
    assert!(
        findings_for(unknown_disposition)
            .iter()
            .any(|finding| matches!(
                finding,
                BlueprintDispositionFinding::UnknownDisposition { .. }
            ))
    );

    let duplicate_feature = concat!(
        "slot\tarea\tdisposition\tfeature\treason\n",
        "1\tprocess-lifecycle\tadopted\tprocess.scheduler-policy\t-\n",
        "2\tprocess-lifecycle\tadopted\tprocess.scheduler-policy\t-\n",
    );
    assert!(
        findings_for(duplicate_feature)
            .iter()
            .any(|finding| matches!(
                finding,
                BlueprintDispositionFinding::DuplicateAdoptedFeature { .. }
            ))
    );

    let empty = "slot\tarea\tdisposition\tfeature\treason\n";
    let empty_findings = findings_for(empty);
    assert!(
        empty_findings
            .iter()
            .any(|finding| matches!(finding, BlueprintDispositionFinding::EmptyTable))
    );
    assert!(empty_findings.iter().any(|finding| matches!(
        finding,
        BlueprintDispositionFinding::MissingSlot { slot: 1 }
    )));
}

#[test]
fn the_committed_table_has_no_missing_or_duplicate_slots() {
    let table = BlueprintDispositionTable::committed();
    for slot in 1..=SLOT_COUNT as u8 {
        let row = table.slot(slot).expect("every slot is dispositioned");
        assert_eq!(row.slot, slot);
        assert_eq!(
            row.area,
            FeatureArea::ALL[(usize::from(slot) - 1) / ITEMS_PER_AREA]
        );
    }
    assert!(blueprint_disposition_findings().is_empty());
}
