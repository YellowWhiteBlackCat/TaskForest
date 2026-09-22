use super::*;
use taskmanager_core::CapabilityRiskLevel;

#[test]
fn recognises_container_cgroups_and_ids() {
    let docker = "/docker/0123456789abcdef0123456789abcdef";
    assert_eq!(
        detect_isolation(docker, b"", false),
        (
            Some(IsolationKind::Docker),
            Some("0123456789abcdef0123456789abcdef".into())
        )
    );
    assert_eq!(
        detect_isolation("/kubepods.slice/pod123", b"", false).0,
        Some(IsolationKind::Kubernetes)
    );
    assert_eq!(
        detect_isolation("/libpod-abcdef.scope", b"", false).0,
        Some(IsolationKind::Podman)
    );
}

#[test]
fn desktop_sandboxes_use_nul_separated_environment() {
    assert_eq!(
        detect_isolation("", b"USER=a\0FLATPAK_ID=org.example.App\0", false).0,
        Some(IsolationKind::Flatpak)
    );
    assert_eq!(
        detect_isolation("", b"SNAP=/snap/example/1\0", false).0,
        Some(IsolationKind::Snap)
    );
    assert_eq!(detect_isolation("", b"USER=a\0", false), (None, None));
}

#[test]
fn security_status_parser_keeps_only_known_hardening_values() {
    assert_eq!(
        parse_security_status("Name:\tworker\nSeccomp:\t2\nNoNewPrivs:\t1\n"),
        (Some(2), Some(true))
    );
    assert_eq!(
        parse_security_status("Seccomp:\t9\nNoNewPrivs:\t2\n"),
        (None, None)
    );
    assert_eq!(parse_security_status("Name:\tworker\n"), (None, None));
}

#[test]
fn capability_status_parser_keeps_each_mask_independent_and_hex_typed() {
    let parsed = parse_capability_status(
        "CapInh:\t0000000000000001\nCapPrm:\t0000000000200000\nCapEff:\t0000000000080000\nCapBnd:\t0000000000000000\nCapAmb:\tnot-hex\n",
        4_000,
    );
    assert_eq!(parsed.inheritable, Some(1));
    assert_eq!(parsed.permitted, Some(1 << 21));
    assert_eq!(parsed.effective, Some(1 << 19));
    assert_eq!(parsed.bounding, Some(0));
    assert_eq!(parsed.ambient, None);
    assert_eq!(parsed.risk_level(), CapabilityRiskLevel::Critical);
}

#[cfg(target_os = "linux")]
#[test]
fn namespace_collection_compares_fixture_links_to_pid_one() {
    use std::os::unix::fs::symlink;

    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-namespaces-{}", std::process::id()));
    for pid in ["1", "42"] {
        for kind in LinuxNamespaceKind::ALL {
            std::fs::create_dir_all(root.join(pid).join("ns")).expect("create ns directory");
            let inode = if pid == "1" {
                1000
            } else if kind == LinuxNamespaceKind::Network {
                2000
            } else {
                1000
            };
            symlink(
                format!("{}:[{inode}]", kind.proc_name()),
                root.join(pid).join("ns").join(kind.proc_name()),
            )
            .expect("create namespace link");
        }
    }
    let audit = collect_namespace_audit(&root.join("42"), &root, 5_000);
    assert_eq!(audit.entries.len(), LinuxNamespaceKind::ALL.len());
    assert_eq!(audit.isolated_count(), 1);
    assert_eq!(
        audit.entry(LinuxNamespaceKind::Network).unwrap().status,
        NamespaceAuditStatus::Isolated {
            inode: 2000,
            host_inode: 1000,
        }
    );
    assert_eq!(
        audit.entry(LinuxNamespaceKind::Pid).unwrap().status,
        NamespaceAuditStatus::Host { inode: 1000 }
    );
    std::fs::remove_dir_all(root).expect("remove namespace fixture");
}

#[test]
fn sandbox_metadata_parsers_keep_uid_rootfs_and_confinement_typed() {
    assert_eq!(parse_root_host_uid("0 100000 65536\n"), Some(100_000));
    assert_eq!(parse_root_host_uid("1000 101000 64536\n"), None);
    assert_eq!(
        parse_rootfs_read_only("36 25 0:32 / / ro,relatime - tmpfs tmpfs ro\n"),
        Some(true)
    );
    assert_eq!(
        parse_rootfs_read_only("36 25 0:32 / / rw,relatime - ext4 /dev/vda rw\n"),
        Some(false)
    );
    assert_eq!(
        parse_snap_confinement(b"SNAP_NAME=demo\0SNAP_CONFINEMENT=strict\0"),
        Some("strict".to_owned())
    );
    assert_eq!(parse_snap_confinement(b"container=flatpak\0"), None);
    assert_eq!(
        parse_sandbox_permissions("shared=network;ipc\nsockets=wayland;x11\nfilesystems=home,ro\n"),
        vec![
            "network".to_owned(),
            "ipc".to_owned(),
            "wayland".to_owned(),
            "x11".to_owned(),
            "home".to_owned(),
            "ro".to_owned(),
        ]
    );
}
