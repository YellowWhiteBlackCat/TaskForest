use super::*;

#[test]
fn canonical_targets_round_trip_without_display_name_inference() {
    for (target, expected_init, expected_native) in [
        (
            systemd_service_id("demo.service"),
            InitSystem::Systemd,
            "demo.service",
        ),
        (openrc_service_id("demo"), InitSystem::Openrc, "demo"),
    ] {
        let resolved = resolve_service_target(&target).expect("canonical service target");
        assert_eq!(resolved.init(), expected_init);
        assert_eq!(resolved.native(), expected_native);
    }
}

#[test]
fn empty_unknown_path_option_and_repeated_prefix_targets_are_rejected() {
    for target in [
        ServiceId::default(),
        ServiceId::new("demo"),
        ServiceId::new("unknown:demo"),
        systemd_service_id("demo"),
        systemd_service_id(""),
        systemd_service_id("demo*.service"),
        systemd_service_id("demo?.service"),
        systemd_service_id("demo[0].service"),
        systemd_service_id("demo;stop.service"),
        systemd_service_id("demo\\x2fescape.service"),
        systemd_service_id("demo\n.service"),
        openrc_service_id("--help"),
        openrc_service_id("../demo"),
        openrc_service_id("demo*"),
        openrc_service_id("demo?"),
        openrc_service_id("demo;stop"),
        openrc_service_id("demo/name"),
        openrc_service_id("demo\n"),
        ServiceId::new(format!("{OPENRC_PREFIX}{SYSTEMD_PREFIX}demo.service")),
        ServiceId::new(format!("{SYSTEMD_PREFIX}{OPENRC_PREFIX}demo")),
    ] {
        assert_eq!(
            resolve_service_target(&target),
            Err(ProviderFailure::Rejected),
            "{target}"
        );
    }
}

#[test]
fn real_unit_and_init_script_name_shapes_remain_authorizable() {
    for unit in [
        "demo.service",
        "worker@42.service",
        "dbus-org.demo.service",
        "escaped\\x2dname.service",
        "utf8\\xc3\\xa9.service",
    ] {
        assert!(valid_systemd_service_name(unit), "{unit}");
    }
    for unit in ["network.target", "demo.socket", "home-user.mount"] {
        assert!(valid_systemd_unit_name(unit), "{unit}");
    }
    for service in ["networking", "net.eth0", "worker@42", "local-mount_2"] {
        assert!(valid_openrc_service_name(service), "{service}");
    }
}

#[test]
fn same_display_name_has_distinct_backend_bound_authority() {
    assert_ne!(
        systemd_service_id("demo.service"),
        openrc_service_id("demo")
    );
}

#[test]
fn stale_backend_bound_target_is_identity_changed_not_reinterpreted() {
    assert_eq!(
        resolve_service_target_for_detection(
            &systemd_service_id("demo.service"),
            Ok(InitSystem::Openrc),
        ),
        Err(ProviderFailure::IdentityChanged)
    );
    assert_eq!(
        resolve_service_target_for_detection(&openrc_service_id("demo"), Ok(InitSystem::Systemd),),
        Err(ProviderFailure::IdentityChanged)
    );
}
