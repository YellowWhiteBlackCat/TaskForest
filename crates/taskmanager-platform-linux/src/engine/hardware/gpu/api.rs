//! Bounded Linux runtime graphics-API probes.
//!
//! `glxinfo` and `vulkaninfo` are optional capability tools, not required
//! product dependencies. Their fixed argv and bounded output are owned here;
//! only the canonical version tokens cross into the core GPU model.

#![cfg_attr(any(test, feature = "test-support"), allow(dead_code))]

use taskmanager_core::GpuGraphicsApi;

#[cfg(not(any(test, feature = "test-support")))]
use std::process::Command;
#[cfg(not(any(test, feature = "test-support")))]
use std::time::Duration;
#[cfg(not(any(test, feature = "test-support")))]
use taskmanager_platform_portable::run_with_timeout;

#[cfg(not(any(test, feature = "test-support")))]
const GRAPHICS_API_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Probe the current Linux graphics runtime once.
///
/// The caller binds the result to a GPU only when the DRM inventory proves
/// there is exactly one visible adapter. That prevents a display-server
/// renderer or first Vulkan device from being copied onto an unrelated GPU.
pub(super) fn probe_graphics_api() -> Option<GpuGraphicsApi> {
    #[cfg(any(test, feature = "test-support"))]
    {
        None
    }

    #[cfg(not(any(test, feature = "test-support")))]
    {
        let opengl_version = probe_command("glxinfo", &["-B"], parse_opengl_version);
        let vulkan_version = probe_command("vulkaninfo", &["--summary"], parse_vulkan_version);
        (opengl_version.is_some() || vulkan_version.is_some()).then_some(GpuGraphicsApi {
            opengl_version,
            vulkan_version,
        })
    }
}

#[cfg(not(any(test, feature = "test-support")))]
fn probe_command(
    program: &str,
    args: &[&str],
    parse: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    let output = run_with_timeout(&mut command, GRAPHICS_API_PROBE_TIMEOUT).ok()?;
    output.status.success().then(|| {
        parse(&String::from_utf8_lossy(&output.stdout))
            .or_else(|| parse(&String::from_utf8_lossy(&output.stderr)))
    })?
}

/// Parse the canonical OpenGL version line from `glxinfo -B`.
fn parse_opengl_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let value = line
            .strip_prefix("OpenGL version string:")
            .or_else(|| line.strip_prefix("OpenGL core profile version string:"))?;
        parse_version_token(value)
    })
}

/// Parse the physical-device API version from `vulkaninfo --summary`.
fn parse_vulkan_version(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case("apiVersion")
                .then_some(value)
        })
        .and_then(parse_version_token)
}

fn parse_version_token(value: &str) -> Option<String> {
    let token: String = value
        .trim()
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    let mut components = token.split('.');
    let major = components.next()?.parse::<u32>().ok()?;
    let minor = components.next()?.parse::<u32>().ok()?;
    if components
        .next()
        .is_some_and(|part| part.parse::<u32>().is_err())
    {
        return None;
    }
    (major > 0 || minor > 0).then_some(token)
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_hardware_gpu_api_tests.rs"]
mod tests;
