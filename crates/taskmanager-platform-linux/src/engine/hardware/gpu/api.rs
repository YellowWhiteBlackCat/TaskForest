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
        let glxinfo = probe_command_text("glxinfo", &["-B"]);
        let opengl_version = glxinfo.as_deref().and_then(parse_opengl_version);
        let mesa_version = glxinfo.as_deref().and_then(parse_mesa_version);
        let vulkan_version = probe_command("vulkaninfo", &["--summary"], parse_vulkan_version);
        (opengl_version.is_some() || vulkan_version.is_some() || mesa_version.is_some()).then_some(
            GpuGraphicsApi {
                opengl_version,
                vulkan_version,
                mesa_version,
            },
        )
    }
}

#[cfg(not(any(test, feature = "test-support")))]
fn probe_command(
    program: &str,
    args: &[&str],
    parse: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    probe_command_text(program, args).and_then(|text| parse(&text))
}

#[cfg(not(any(test, feature = "test-support")))]
fn probe_command_text(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    let output = run_with_timeout(&mut command, GRAPHICS_API_PROBE_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Some(text)
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

/// Parse Mesa's userspace release from the same bounded `glxinfo -B` output
/// as the OpenGL version. The token is kept verbatim after validation so a
/// distro build suffix remains visible instead of being silently discarded.
fn parse_mesa_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, rest) = line.split_once(" Mesa ")?;
        let token = rest.split_whitespace().next()?.trim();
        let mut parts = token.split(['.', '-']);
        let major = parts.next()?.parse::<u32>().ok()?;
        let minor = parts.next()?.parse::<u32>().ok()?;
        (major > 0 || minor > 0).then(|| token.to_owned())
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
