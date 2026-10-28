use super::*;
use std::collections::HashSet;

#[test]
fn the_inventory_covers_exactly_sixteen_columns() {
    assert_eq!(PROCESS_COLUMNS.len(), 16);
    assert_eq!(
        PROCESS_COLUMNS.first().map(|spec| spec.id),
        Some("Name"),
        "Name stays the leading identity column"
    );
    assert_eq!(
        PROCESS_COLUMNS.last().map(|spec| spec.id),
        Some("Nice"),
        "Nice stays the trailing column"
    );
}

#[test]
fn ids_are_distinct_and_lookup_is_exact() {
    let mut seen = HashSet::new();
    for spec in PROCESS_COLUMNS {
        assert!(seen.insert(spec.id), "duplicate column id {}", spec.id);
        assert_eq!(
            find(spec.id),
            Some(spec),
            "lookup must be exact for {}",
            spec.id
        );
    }
    assert_eq!(seen.len(), PROCESS_COLUMNS.len());
    assert_eq!(
        find("not-a-process-column"),
        None,
        "unknown tokens must not resolve"
    );
}

#[test]
fn name_is_not_hideable_and_every_width_is_positive() {
    let name = find("Name").expect("Name column exists");
    assert!(!name.hideable, "the identity column must never be hideable");
    for spec in PROCESS_COLUMNS {
        assert!(
            spec.default_width > 0.0,
            "{} must have a positive default width",
            spec.id
        );
        if spec.id != "Name" {
            assert!(spec.hideable, "{} must stay hideable", spec.id);
        }
    }
}

/// The column-width policy is ONE authority: the values are pinned here so
/// a frontend cannot drift away from the contract, and the policy stays
/// coherent (a minimum below the maximum, a positive step).
#[test]
fn the_column_width_policy_is_pinned_and_coherent() {
    assert_eq!(COLUMN_WIDTH_MIN, 10.0, "the policy's minimum is pinned");
    assert_eq!(COLUMN_WIDTH_MAX, 1200.0, "the policy's maximum is pinned");
    assert_eq!(
        COLUMN_WIDTH_KEYBOARD_STEP, 10.0,
        "the policy's keyboard step is pinned"
    );
    // The policy's coherence is a COMPILE-TIME fact: these are constants, so
    // a contradictory policy fails the build instead of a test run.
    const {
        assert!(COLUMN_WIDTH_MIN < COLUMN_WIDTH_MAX);
        assert!(COLUMN_WIDTH_KEYBOARD_STEP > 0.0);
        assert!(COLUMN_WIDTH_MIN + COLUMN_WIDTH_KEYBOARD_STEP <= COLUMN_WIDTH_MAX);
    }
}

/// One transition authority: drags and keyboard steps clamp identically, and
/// a terminal can supply its cell bounds through the same policy.
#[test]
fn the_resize_policy_clamps_and_steps_in_the_callers_unit() {
    let process = ColumnResizePolicy::PROCESS;
    assert_eq!(process.step(120.0, 1), 130.0);
    assert_eq!(process.step(120.0, -2), 100.0);
    assert_eq!(
        process.step(COLUMN_WIDTH_MAX, 1),
        COLUMN_WIDTH_MAX,
        "a keyboard step never passes the maximum"
    );
    assert_eq!(process.drag(50.0, 5_000.0), COLUMN_WIDTH_MAX);
    assert_eq!(process.drag(COLUMN_WIDTH_MIN, -5_000.0), COLUMN_WIDTH_MIN);

    let cells = ColumnResizePolicy {
        min: 4.0,
        max: 40.0,
        step: 1.0,
    };
    assert_eq!(cells.step(4.0, -1), 4.0);
    assert_eq!(cells.drag(39.5, 1.0), 40.0);
    assert_eq!(cells.step(10.0, 3), 13.0);

    let inverted = ColumnResizePolicy {
        min: 10.0,
        max: 2.0,
        step: 1.0,
    };
    assert_eq!(
        inverted.clamp(20.0),
        10.0,
        "an inverted bound pair still yields a legal width"
    );
}
