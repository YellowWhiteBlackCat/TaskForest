//! test-intent: behavior
//! One optional session slot: absent stays absent, install binds the real
//! session, and a second install replaces rather than forks the owner.

use super::*;

#[test]
fn absent_slot_never_invents_an_active_session() {
    let mut slot = SessionSlot::<u32>::default();
    assert!(!slot.is_active());
    assert_eq!(slot.active_mut(), None);
}

#[test]
fn install_binds_one_session_and_replaces_the_previous_owner() {
    let mut slot = SessionSlot::default();
    slot.install_with(|| 7_u32);
    assert!(slot.is_active());
    assert_eq!(slot.active_mut(), Some(&mut 7));

    if let Some(session) = slot.active_mut() {
        *session += 1;
    }
    assert_eq!(slot.active_mut(), Some(&mut 8));

    slot.install_with(|| 41_u32);
    assert_eq!(slot.active_mut(), Some(&mut 41));
}
