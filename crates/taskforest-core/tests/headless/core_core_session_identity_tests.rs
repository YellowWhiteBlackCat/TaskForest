use taskforest_core::core::session::{SessionIdentity, SessionIncarnation, SessionItem};
use taskforest_core::core::target::SessionId;

fn item(incarnation: Option<SessionIncarnation>) -> SessionItem {
    SessionItem {
        id: SessionId::new("seat0"),
        incarnation,
        uid: 501,
        user: "alice".into(),
        seat: Some("seat0".into()),
        tty: None,
        remote: false,
        timestamp: None,
    }
}

#[test]
fn only_valid_os_lifetime_facts_form_a_session_identity() {
    assert_eq!(item(None).identity(), None);
    assert_eq!(
        item(Some(SessionIncarnation::LinuxLogind {
            boot_id: String::new(),
            created_monotonic_usec: 1,
        }))
        .identity(),
        None
    );
    assert_eq!(
        item(Some(SessionIncarnation::WindowsWts {
            logon_time_100ns: 0,
        }))
        .identity(),
        None
    );

    let identity = item(Some(SessionIncarnation::WindowsWts {
        logon_time_100ns: 42,
    }))
    .identity();
    assert_eq!(
        identity,
        Some(SessionIdentity {
            session_id: SessionId::new("seat0"),
            incarnation: SessionIncarnation::WindowsWts {
                logon_time_100ns: 42,
            },
        })
    );
}

#[test]
fn structured_session_identity_serialization_keeps_delimiter_data_unambiguous() {
    let identity = SessionIdentity::new(
        SessionId::new("linux:session:7|incarnation"),
        SessionIncarnation::LinuxLogind {
            boot_id: "boot|id:part".into(),
            created_monotonic_usec: 123,
        },
    )
    .expect("valid source identity");

    let encoded = serde_json::to_string(&identity).expect("serialize identity");
    let decoded: SessionIdentity = serde_json::from_str(&encoded).expect("deserialize identity");
    assert_eq!(decoded, identity);

    let alternate = SessionIdentity::new(
        SessionId::new("linux:session:7"),
        SessionIncarnation::LinuxLogind {
            boot_id: "incarnation|boot|id:part".into(),
            created_monotonic_usec: 123,
        },
    )
    .expect("valid alternate identity");
    let alternate = serde_json::to_string(&alternate).expect("serialize alternate identity");
    assert_ne!(encoded, alternate);
}
