/// Check whether a network interface is virtual (docker, veth, br-, virbr, vnet, tun, tap, lo).
pub fn is_virtual_interface(name: &str) -> bool {
    name.starts_with("docker")
        || name.starts_with("veth")
        || name.starts_with("br-")
        || name.starts_with("virbr")
        || name.starts_with("vnet")
        || name.starts_with("tun")
        || name.starts_with("tap")
        || name == "lo"
}
