use super::*;
use taskforest_core::core::session::SessionFamilyFact;

fn session(
    seat: Option<&str>,
    tty: Option<&str>,
    timestamp: Option<&str>,
    remote: bool,
) -> SessionItem {
    let seat = seat.map(str::to_owned);
    let tty = tty.map(str::to_owned);
    let family = SessionFamilyFact::classify(seat.as_deref(), tty.as_deref(), remote);
    SessionItem {
        id: "session-7".to_owned().into(),
        incarnation: None,
        uid: 1000,
        user: "alice".to_owned(),
        seat,
        tty,
        timestamp: timestamp.map(str::to_owned),
        family,
    }
}

#[test]
fn missing_seat_tty_and_logon_fold_to_the_shared_dash() {
    let vm = user_row_vm(&session(None, None, None, false));
    assert_eq!(vm.seat, missing_value());
    assert_eq!(vm.tty, missing_value());
    assert_eq!(vm.logon, missing_value());
}

#[test]
fn present_cells_pass_through_verbatim() {
    let vm = user_row_vm(&session(
        Some("seat0"),
        Some("tty2"),
        Some("2026-08-19 09:00"),
        false,
    ));
    assert_eq!(vm.session, "session-7");
    assert_eq!(vm.user, "alice");
    assert_eq!(vm.seat, "seat0");
    assert_eq!(vm.tty, "tty2");
    assert_eq!(vm.logon, "2026-08-19 09:00");
}

#[test]
fn family_folds_through_the_shared_localized_fact_labels() {
    assert_eq!(
        user_row_vm(&session(None, None, None, true)).family_label,
        i18n::t("users.family_remote")
    );
    assert_eq!(
        user_row_vm(&session(None, None, None, false)).family_label,
        i18n::t("users.family_unobserved")
    );
    assert!(
        user_row_vm(&session(None, None, None, true)).family_is_remote,
        "the renderer accent key follows the typed remote family"
    );
    assert!(
        !user_row_vm(&session(None, None, None, false)).family_is_remote,
        "absence does not borrow the remote accent"
    );
}
