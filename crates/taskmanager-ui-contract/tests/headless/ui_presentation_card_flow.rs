use super::*;

#[test]
fn a_group_below_the_readable_floor_is_not_admitted() {
    let spec = CardFlowSpec::new(112.0, 172.0).with_gap(8.0);
    assert_eq!(admit_card_groups(0.0, 2, spec).groups, 0);
    assert_eq!(admit_card_groups(111.0, 2, spec).groups, 0);
    assert_eq!(admit_card_groups(111.0, 2, spec).card_height, 0.0);
}

#[test]
fn the_floor_and_ceiling_clamp_the_card_height() {
    let spec = CardFlowSpec::new(112.0, 172.0).with_gap(8.0);
    let at_floor = admit_card_groups(112.0, 2, spec);
    assert_eq!(at_floor.groups, 1);
    assert_eq!(at_floor.card_height, 112.0);
    assert_eq!(at_floor.cards(), 2);

    let elastic = admit_card_groups(152.0, 2, spec);
    assert_eq!(elastic.groups, 1);
    assert_eq!(elastic.card_height, 152.0);

    let at_ceiling = admit_card_groups(240.0, 2, spec);
    assert_eq!(at_ceiling.groups, 2);
    assert_eq!(at_ceiling.card_height, 116.0);

    let capped = admit_card_groups_capped(1000.0, 2, spec, 2);
    assert_eq!(capped.groups, 2);
    assert_eq!(capped.card_height, 172.0);
}

#[test]
fn gaps_are_charged_before_any_card_grows() {
    let spec = CardFlowSpec::new(112.0, 172.0).with_gap(8.0);
    let two = admit_card_groups(112.0 * 2.0 + 8.0, 1, spec);
    assert_eq!(two.groups, 2);
    assert_eq!(two.card_height, 112.0);
    let with_slack = admit_card_groups(112.0 * 2.0 + 8.0 + 20.0, 1, spec);
    assert_eq!(with_slack.groups, 2);
    assert_eq!(with_slack.card_height, 122.0);
}

#[test]
fn degenerate_and_inverted_footprints_fail_closed() {
    for (floor, ceiling) in [(0.0, 10.0), (-4.0, 10.0), (120.0, 80.0)] {
        let spec = CardFlowSpec::new(floor, ceiling).with_gap(8.0);
        let admitted = admit_card_groups(1000.0, 2, spec);
        assert!(admitted.card_height >= 0.0);
        assert!(admitted.card_height <= spec.ceiling);
    }
    let infinite = CardFlowSpec::new(f32::NAN, f32::INFINITY).with_gap(f32::NAN);
    let admitted = admit_card_groups(f32::NAN, 0, infinite);
    assert_eq!(admitted.columns, 1);
    assert_eq!(admitted.groups, 0);
    assert_eq!(admitted.card_height, 0.0);
}
