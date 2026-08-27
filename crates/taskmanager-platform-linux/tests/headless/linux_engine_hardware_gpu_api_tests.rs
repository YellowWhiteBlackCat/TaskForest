use super::{parse_opengl_version, parse_vulkan_version};

#[test]
fn parses_the_canonical_opengl_version_without_driver_suffix() {
    let output = "OpenGL renderer string: Intel Arc B390\nOpenGL version string: 4.6 (Compatibility Profile) Mesa 26.2.1";

    assert_eq!(parse_opengl_version(output).as_deref(), Some("4.6"));
}

#[test]
fn accepts_core_profile_when_the_compatibility_line_is_missing() {
    let output = "OpenGL core profile version string: 4.5 (Core Profile) Mesa";

    assert_eq!(parse_opengl_version(output).as_deref(), Some("4.5"));
}

#[test]
fn parses_a_physical_device_vulkan_api_version_not_the_instance_banner() {
    let output = "Vulkan Instance Version: 1.4.357\nGPU0:\n    apiVersion = 1.4.354\n    deviceName = Intel Arc B390";

    assert_eq!(parse_vulkan_version(output).as_deref(), Some("1.4.354"));
}

#[test]
fn malformed_graphics_api_versions_are_omitted() {
    assert_eq!(
        parse_opengl_version("OpenGL version string: unavailable"),
        None
    );
    assert_eq!(parse_vulkan_version("apiVersion = 0.0"), None);
}
