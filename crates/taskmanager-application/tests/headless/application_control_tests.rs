use super::*;

#[test]
fn superseded_control_result_cannot_replace_latest_intent() {
    let mut requests = LatestControlRequest::default();
    let old = requests.begin();
    let latest = requests.begin();

    assert!(!requests.accept(old));
    assert_eq!(requests.pending(), Some(latest));
    assert!(requests.accept(latest));
    assert_eq!(requests.pending(), None);
}

#[test]
fn request_ids_remain_non_zero_after_wrap() {
    let mut requests = LatestControlRequest {
        next: u64::MAX,
        pending: None,
    };

    assert_eq!(requests.begin().get(), 1);
}

#[test]
fn service_control_rejects_out_of_order_wrong_target_and_wrong_action() {
    let systemd = ServiceId::new("linux.service.systemd:demo.service");
    let openrc = ServiceId::new("linux.service.openrc:demo");
    let mut requests = LatestServiceControlRequest::default();
    let old = requests.begin(systemd.clone(), ServiceAction::Start);
    let latest = requests.begin(openrc.clone(), ServiceAction::Restart);

    assert!(!requests.accept(old, &systemd, ServiceAction::Start));
    assert!(!requests.accept(latest, &systemd, ServiceAction::Restart));
    assert!(!requests.accept(latest, &openrc, ServiceAction::Stop));
    assert_eq!(
        requests.pending(),
        Some((latest, &openrc, ServiceAction::Restart))
    );
    assert!(requests.accept(latest, &openrc, ServiceAction::Restart));
    assert_eq!(requests.pending(), None);
}

#[test]
fn session_control_rejects_out_of_order_wrong_incarnation_and_wrong_action() {
    use taskforest_core::core::session::{SessionIdentity, SessionIncarnation};
    use taskforest_core::core::target::SessionId;

    let identity = |created_monotonic_usec| {
        SessionIdentity::new(
            SessionId::new("session:stable"),
            SessionIncarnation::LinuxLogind {
                boot_id: "boot:1".into(),
                created_monotonic_usec,
            },
        )
        .expect("fixture identity")
    };
    let first = identity(10);
    let replacement = identity(20);
    let mut requests = LatestSessionControlRequest::default();
    let old = requests.begin(first.clone(), SessionControlAction::Lock);
    let latest = requests.begin(replacement.clone(), SessionControlAction::Disconnect);

    assert!(!requests.accept(old, &first, SessionControlAction::Lock));
    assert!(!requests.accept(latest, &first, SessionControlAction::Disconnect));
    assert!(!requests.accept(latest, &replacement, SessionControlAction::Lock));
    assert_eq!(
        requests.pending(),
        Some((latest, &replacement, SessionControlAction::Disconnect))
    );
    assert!(requests.accept(latest, &replacement, SessionControlAction::Disconnect));
    assert_eq!(requests.pending(), None);
}
