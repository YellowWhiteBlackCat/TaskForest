use super::*;

#[test]
fn test_parse_cgroup_v2_docker() {
    let cgroup = "0::/system.slice/docker-a1b2c3d4e5f678901234567890abcdef.scope";
    let meta = parse_cgroup_container_metadata(cgroup).expect("should parse docker");
    assert_eq!(meta.runtime, ContainerRuntime::Docker);
    assert_eq!(meta.container_id, "a1b2c3d4e5f6");
    assert_eq!(meta.display_badge(), "[Docker: a1b2c3d4e5f6]");
}

#[test]
fn test_parse_cgroup_v2_podman() {
    let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/libpod-fedcba0987654321fedcba0987654321.scope";
    let meta = parse_cgroup_container_metadata(cgroup).expect("should parse podman");
    assert_eq!(meta.runtime, ContainerRuntime::Podman);
    assert_eq!(meta.container_id, "fedcba098765");
    assert_eq!(meta.display_badge(), "[Podman: fedcba098765]");
}

#[test]
fn test_parse_cgroup_v2_crio() {
    let cgroup = "0::/kubepods.slice/kubepods-burstable.slice/crio-1122334455667788.scope";
    let meta = parse_cgroup_container_metadata(cgroup).expect("should parse crio");
    assert_eq!(meta.runtime, ContainerRuntime::Crio);
    assert_eq!(meta.container_id, "112233445566");
}

#[test]
fn test_parse_cgroup_v2_flatpak() {
    let cgroup = "0::/app.slice/app-flatpak-org.mozilla.firefox-12345.scope";
    let meta = parse_cgroup_container_metadata(cgroup).expect("should parse flatpak");
    assert_eq!(meta.runtime, ContainerRuntime::Flatpak);
    assert_eq!(meta.name.as_deref(), Some("org.mozilla.firefox"));
    assert_eq!(meta.display_badge(), "[Flatpak: org.mozilla.firefox]");
}

#[test]
fn test_parse_cgroup_v2_snap() {
    let cgroup = "0::/user.slice/user-1000.slice/snap.discord.discord-1234.scope";
    let meta = parse_cgroup_container_metadata(cgroup).expect("should parse snap");
    assert_eq!(meta.runtime, ContainerRuntime::Snap);
    assert_eq!(meta.name.as_deref(), Some("discord"));
}

#[test]
fn test_parse_cgroup_host_process_returns_none() {
    let cgroup = "0::/user.slice/user-1000.slice/user@1000.service/session.slice/session-2.scope";
    assert_eq!(parse_cgroup_container_metadata(cgroup), None);
}
