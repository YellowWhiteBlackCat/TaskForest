use super::spec::COMMAND_SPECS;
use super::*;

#[test]
fn canonical_key_names_parse_without_driving_routing_by_strings() {
    assert_eq!(KeyCode::parse("PageUp"), Ok(KeyCode::PageUp));
    assert_eq!(KeyCode::parse("F9"), Ok(KeyCode::F9));
    assert_eq!(
        KeyChord::from_key_name("f", Modifiers::CONTROL),
        Ok(KeyChord::new(KeyCode::F, Modifiers::CONTROL))
    );
    assert_eq!(KeyCode::parse("control-f"), Err(KeyParseError::UnknownKey));
}

/// The spec table is the single source `CommandId::ALL`, the default
/// bindings, and the enable rules derive from, so a duplicated id would
/// silently drop one command from every derived surface (the router's
/// conflict check only rejects duplicated chords, not duplicated ids).
#[test]
fn command_spec_rows_carry_unique_command_ids() {
    for (index, row) in COMMAND_SPECS.iter().enumerate() {
        assert!(
            !COMMAND_SPECS[index + 1..]
                .iter()
                .any(|later| later.id == row.id),
            "duplicate spec row for {:?}",
            row.id
        );
    }
}
