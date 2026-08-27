use super::*;

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
