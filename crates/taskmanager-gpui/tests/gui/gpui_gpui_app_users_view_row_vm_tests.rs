use super::*;

fn session(
    seat: Option<&str>,
    tty: Option<&str>,
    timestamp: Option<&str>,
    remote: bool,
) -> SessionItem {
    SessionItem {
        id: "session-7".to_owned(),
        uid: 1000,
        user: "alice".to_owned(),
        seat: seat.map(str::to_owned),
        tty: tty.map(str::to_owned),
        remote,
        timestamp: timestamp.map(str::to_owned),
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
fn remote_folds_to_the_localized_yes_no_labels() {
    assert_eq!(
        user_row_vm(&session(None, None, None, true)).remote_label,
        i18n::t("common.yes")
    );
    assert_eq!(
        user_row_vm(&session(None, None, None, false)).remote_label,
        i18n::t("common.no")
    );
    assert_ne!(
        i18n::t("common.yes"),
        i18n::t("common.no"),
        "the yes/no keys must resolve to distinct labels"
    );
}
