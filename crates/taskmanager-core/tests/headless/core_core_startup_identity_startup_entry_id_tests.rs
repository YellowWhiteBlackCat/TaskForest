use super::StartupEntryId;

#[test]
fn empty_and_non_empty_ids_are_distinguished() {
    assert!(StartupEntryId::from(String::new()).is_empty());
    assert!(StartupEntryId::from("").is_empty());
    assert!(!StartupEntryId::from("systemd-user@session.service").is_empty());
    assert!(!StartupEntryId::from(String::from("x")).is_empty());
}

#[test]
fn ids_round_trip_through_every_constructor() {
    let via_string = StartupEntryId::from(String::from("alice.desktop"));
    let via_str = StartupEntryId::from("alice.desktop");
    assert_eq!(via_string, via_str);
    assert_eq!(via_string.as_str(), "alice.desktop");
}
