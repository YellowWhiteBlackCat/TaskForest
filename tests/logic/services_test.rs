use std::path::Path;
use taskmanager::core::services::{ServiceItem, ServiceStatus};
use taskmanager_platform_linux::{
    InitSystem, ServiceManager, parse_openrc_description, parse_unit_description,
};

#[test]
fn test_systemd_services_scanning() {
    let snapshot = ServiceManager::scan_snapshot();
    let services = snapshot.items;
    println!("Scanned {} services", services.len());
    if !matches!(
        ServiceManager::detect_init(),
        Ok(InitSystem::Systemd | InitSystem::Openrc)
    ) {
        assert!(
            services.is_empty(),
            "unsupported init must fail closed instead of assuming systemd"
        );
        return;
    }
    assert!(!services.is_empty(), "Should scan systemd services");

    for service in &services {
        assert!(!service.name.is_empty(), "Service name should not be empty");
        assert!(
            !service.load_state.is_empty(),
            "Service load_state should not be empty"
        );
    }
}

#[test]
fn test_service_status_mapping() {
    assert_eq!(ServiceStatus::from("active"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("running"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("reloading"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("activating"), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from("inactive"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("dead"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("deactivating"), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from("failed"), ServiceStatus::Failed);
    assert_eq!(ServiceStatus::from("unknown_state"), ServiceStatus::Unknown);

    assert_eq!(ServiceStatus::Active.as_str(), "Active");
    assert_eq!(ServiceStatus::Inactive.as_str(), "Inactive");
    assert_eq!(ServiceStatus::Failed.as_str(), "Failed");
    assert_eq!(ServiceStatus::Unknown.as_str(), "Unknown");
    assert_eq!(format!("{}", ServiceStatus::Active), "Active");
}

#[test]
fn test_scan_unit_files_fallback() {
    let unit_services = ServiceManager::scan_unit_files();
    if Path::new("/usr/lib/systemd/system").exists() || Path::new("/etc/systemd/system").exists() {
        assert!(
            !unit_services.is_empty(),
            "Unit files directory exists so scan_unit_files should return services"
        );
    }
}

#[test]
fn test_service_item_struct() {
    let item = ServiceItem::from_inventory(
        "",
        "dbus",
        ServiceStatus::Active,
        "D-Bus System Message Bus",
        "loaded",
        "active",
        "running",
    );

    assert_eq!(item.name, "dbus");
    assert_eq!(item.status, ServiceStatus::Active);
    assert_eq!(item.description, "D-Bus System Message Bus");
    assert_eq!(item.load_state, "loaded");
}

// ---------------------------------------------------------------------------
// parse_unit_description — pure systemd `Description=` parser
// (mock unit-file bodies; no filesystem access).
// ---------------------------------------------------------------------------

#[test]
fn parse_unit_description_finds_first_value() {
    // Realistic [Unit] section: the parser ignores section headers and returns
    // the first `Description=` value it encounters.
    let unit = "[Unit]\nDescription=Network Manager service\nAfter=network.target\n";
    assert_eq!(
        parse_unit_description(unit),
        Some("Network Manager service".to_string())
    );
}

#[test]
fn parse_unit_description_rejects_description_foo() {
    // `Description_foo=` must NOT match — `=` is required immediately after
    // `Description`. The real `Description=` on a later line wins.
    let unit = "Description_foo=Decoy\nDescription=Real\n";
    assert_eq!(parse_unit_description(unit), Some("Real".to_string()));
}

#[test]
fn parse_unit_description_rejects_all_suffixed_keys() {
    // Any suffix before `=` is rejected (Description_/, Description., ...).
    let unit = "Description_x=y\nDescription.bar=baz\nDescription=Only\n";
    assert_eq!(parse_unit_description(unit), Some("Only".to_string()));
}

#[test]
fn parse_unit_description_strips_double_quotes() {
    let unit = "[Unit]\nDescription=\"D-Bus System Message Bus\"\n";
    assert_eq!(
        parse_unit_description(unit),
        Some("D-Bus System Message Bus".to_string())
    );
}

#[test]
fn parse_unit_description_strips_single_quotes() {
    let unit = "Description='cups printing service'\n";
    assert_eq!(
        parse_unit_description(unit),
        Some("cups printing service".to_string())
    );
}

#[test]
fn parse_unit_description_leaves_mismatched_quotes() {
    // Mismatched quote pair is not a wrapping pair → left untouched.
    let unit = "Description=\"foo'\n";
    assert_eq!(parse_unit_description(unit), Some("\"foo'".to_string()));
}

#[test]
fn parse_unit_description_filters_empty_value() {
    // Empty `Description=` is skipped; a later non-empty one is returned.
    let unit = "Description=\nDescription=Real Value\n";
    assert_eq!(parse_unit_description(unit), Some("Real Value".to_string()));
}

#[test]
fn parse_unit_description_filters_empty_after_quote_strip() {
    // `Description=""` becomes empty after stripping → None (no value to use).
    assert_eq!(parse_unit_description("Description=\"\"\n"), None);
}

#[test]
fn parse_unit_description_returns_none_when_absent() {
    let unit = "[Unit]\nAfter=network.target\n[Service]\nExecStart=/usr/bin/sleep 1\n";
    assert_eq!(parse_unit_description(unit), None);
    assert_eq!(parse_unit_description(""), None);
}

#[test]
fn parse_unit_description_is_case_sensitive() {
    // systemd unit keys are case-sensitive; lowercase `description=` must not
    // match `Description=` (this is the systemd parser, not the OpenRC one).
    assert_eq!(parse_unit_description("description=lowercase\n"), None);
}

// ---------------------------------------------------------------------------
// parse_openrc_description — pure OpenRC init.d `description=` parser
// (mock init.d script bodies; no filesystem access).
// ---------------------------------------------------------------------------

#[test]
fn parse_openrc_description_finds_value_in_realistic_script() {
    // Shape of a real /etc/init.d/<name> shell script: shebang, description
    // among other `command=`/`pidfile=` style vars.
    let script = "#!/sbin/openrc-run\n\ncommand=\"/usr/sbin/sshd\"\ndescription=\"Secure Shell server\"\npidfile=\"/run/sshd.pid\"\n";
    assert_eq!(
        parse_openrc_description(script),
        Some("Secure Shell server".to_string())
    );
}

#[test]
fn parse_openrc_description_rejects_description_foo() {
    // `description_foo=` (a different var) must be rejected; the real
    // `description=` wins. This is the headline guard for the openrc parser.
    let script = "description_foo=Decoy\ndescription=\"Real\"\n";
    assert_eq!(parse_openrc_description(script), Some("Real".to_string()));
}

#[test]
fn parse_openrc_description_rejects_suffixed_and_aligned_keys() {
    // description_info=, description_extra=, etc. — all rejected.
    let script = "description_info=skip\ndescription_long=skip\ndescription=Keep\n";
    assert_eq!(parse_openrc_description(script), Some("Keep".to_string()));
}

#[test]
fn parse_openrc_description_allows_whitespace_around_equals() {
    // init.d scripts commonly write `description = "..."` with spaces.
    let script = "description = \"Chrony NTP daemon\"\n";
    assert_eq!(
        parse_openrc_description(script),
        Some("Chrony NTP daemon".to_string())
    );
}

#[test]
fn parse_openrc_description_strips_double_quotes() {
    assert_eq!(
        parse_openrc_description("description=\"OpenRC Service\"\n"),
        Some("OpenRC Service".to_string())
    );
}

#[test]
fn parse_openrc_description_strips_single_quotes() {
    assert_eq!(
        parse_openrc_description("description='single-quoted'\n"),
        Some("single-quoted".to_string())
    );
}

#[test]
fn parse_openrc_description_unquoted_value() {
    // Unquoted values are accepted as-is.
    assert_eq!(
        parse_openrc_description("description=Plain text\n"),
        Some("Plain text".to_string())
    );
}

#[test]
fn parse_openrc_description_filters_empty_value() {
    // Empty `description=` skipped; later non-empty value returned.
    let script = "description=\ndescription=Later\n";
    assert_eq!(parse_openrc_description(script), Some("Later".to_string()));
}

#[test]
fn parse_openrc_description_filters_empty_after_quote_strip() {
    // `description=""` → empty after strip → None.
    assert_eq!(parse_openrc_description("description=\"\"\n"), None);
    assert_eq!(parse_openrc_description("description=''\n"), None);
}

#[test]
fn parse_openrc_description_returns_none_when_absent() {
    let script = "#!/sbin/openrc-run\ncommand=/usr/bin/foo\n";
    assert_eq!(parse_openrc_description(script), None);
    assert_eq!(parse_openrc_description(""), None);
}

#[test]
fn parse_openrc_description_is_case_sensitive_lowercase() {
    // The OpenRC parser keys on lowercase `description` (init.d convention);
    // a capitalised `Description=` must not match it.
    assert_eq!(parse_openrc_description("Description=Capital\n"), None);
}

#[test]
fn test_service_description_after_scan() {
    let snapshot = ServiceManager::scan_snapshot();
    let services = snapshot.items;
    if !matches!(
        ServiceManager::detect_init(),
        Ok(InitSystem::Systemd | InitSystem::Openrc)
    ) {
        assert!(
            services.is_empty(),
            "unsupported init must not fabricate service descriptions"
        );
        return;
    }
    assert!(!services.is_empty(), "Should have scanned services");
    let with_desc: Vec<_> = services
        .iter()
        .filter(|s| !s.description.is_empty())
        .collect();
    assert!(
        !with_desc.is_empty(),
        "At least some scanned services should have descriptions"
    );
}

#[test]
fn test_service_status_display_for_dialog() {
    // The Service Details dialog renders item.status.to_string() / .as_str().
    // Each variant must produce a human-readable label.
    assert_eq!(ServiceStatus::Active.as_str(), "Active");
    assert_eq!(ServiceStatus::Inactive.as_str(), "Inactive");
    assert_eq!(ServiceStatus::Failed.as_str(), "Failed");
    assert_eq!(ServiceStatus::Unknown.as_str(), "Unknown");
    assert_eq!(format!("{}", ServiceStatus::Active), "Active");
    assert_eq!(format!("{}", ServiceStatus::Unknown), "Unknown");
}
