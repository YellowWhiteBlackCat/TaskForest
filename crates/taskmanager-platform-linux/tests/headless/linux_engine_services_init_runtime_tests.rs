use super::*;

#[derive(serde::Deserialize)]
struct FixtureCase {
    pid_one_comm: Option<String>,
    openrc_runtime_active: bool,
    openrc_binary_installed: bool,
    expected: InitSystem,
}

#[test]
fn installed_openrc_binary_never_selects_an_inactive_backend() {
    let cases: Vec<FixtureCase> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/init_runtime_selection.json"
    )))
    .expect("runtime selection fixture");
    for case in cases {
        assert_eq!(
            classify_init(
                case.pid_one_comm.as_deref(),
                case.openrc_runtime_active,
                case.openrc_binary_installed,
            ),
            case.expected
        );
    }
}

#[test]
fn runtime_selection_can_change_between_requests() {
    let observations = [
        classify_init(Some("systemd\n"), false, true),
        classify_init(Some("openrc-init\n"), true, true),
        classify_init(Some("runit\n"), false, true),
        classify_init(Some("systemd\n"), false, false),
    ];
    assert_eq!(
        observations,
        [
            InitSystem::Systemd,
            InitSystem::Openrc,
            InitSystem::Unsupported,
            InitSystem::Systemd,
        ]
    );
}

#[test]
fn probe_failures_remain_typed() {
    assert_eq!(
        classify_probe_error(io::ErrorKind::NotFound),
        FailureKind::MissingDependency
    );
    assert_eq!(
        classify_probe_error(io::ErrorKind::PermissionDenied),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        classify_probe_error(io::ErrorKind::TimedOut),
        FailureKind::TimedOut
    );
    assert_eq!(
        classify_probe_error(io::ErrorKind::Other),
        FailureKind::ProviderFault
    );
}
