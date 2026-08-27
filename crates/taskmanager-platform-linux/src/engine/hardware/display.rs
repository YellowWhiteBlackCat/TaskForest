//! Safe Linux display inventory from DRM connector status and EDID.
//!
//! The provider reads `/sys/class/drm` for connector/EDID identity and, when
//! an active Wayland session exists, opens a separate bounded read-only client
//! for compositor current-mode state. It never invokes `xrandr` or submits a
//! display configuration request. A connected connector remains visible even
//! when its EDID or compositor state is absent; only proven fields are filled.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use taskmanager_core::DisplayInfo;
use taskmanager_platform_contract::{FailureKind, SourceOutcome};
use taskmanager_platform_portable::EdidFacts;

mod wayland;

pub(super) use wayland::probe_wayland;
const MAX_EDID_BYTES: usize = 128 * 33;

pub(super) fn collect_displays(root: &Path) -> (Vec<DisplayInfo>, SourceOutcome) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => return (Vec::new(), SourceOutcome::Unavailable(io_failure(&error))),
    };

    let mut displays = Vec::new();
    let mut connected = 0_usize;
    let mut failure = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_connector_name(&name) {
            continue;
        }
        let connector_root = entry.path();
        let status = match fs::read_to_string(connector_root.join("status")) {
            Ok(status) => status,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                failure = Some(select_failure(failure, io_failure(&error)));
                continue;
            }
        };
        if !status.trim().eq_ignore_ascii_case("connected") {
            continue;
        }
        connected += 1;

        let connector = connector_name(&name);
        let mode = read_first_mode(&connector_root.join("modes"));
        let mut display = DisplayInfo {
            connector,
            width_px: mode.map(|(width, _)| width),
            height_px: mode.map(|(_, height)| height),
            ..DisplayInfo::default()
        };
        match read_bounded_bytes(&connector_root.join("edid"), MAX_EDID_BYTES) {
            Ok(edid) if !edid.is_empty() => match parse_edid(&display.connector, &edid) {
                Some(parsed) => display = merge_mode_fallback(parsed, &display),
                None => {
                    failure = Some(select_failure(failure, FailureKind::ProviderFault));
                }
            },
            Ok(_) => {
                failure = Some(select_failure(failure, FailureKind::Unsupported));
            }
            Err(error) => {
                failure = Some(select_failure(failure, io_failure(&error)));
            }
        }
        displays.push(display);
    }

    let outcome = match (displays.is_empty(), connected, failure) {
        (false, _, Some(failure)) => SourceOutcome::Partial(failure),
        (false, _, None) => SourceOutcome::Available,
        (true, 0, None) => SourceOutcome::Empty,
        (true, 0, Some(failure)) => SourceOutcome::Unavailable(failure),
        (true, _, Some(failure)) => SourceOutcome::Partial(failure),
        (true, _, None) => SourceOutcome::Partial(FailureKind::Unsupported),
    };
    (displays, outcome)
}

fn merge_mode_fallback(mut parsed: DisplayInfo, fallback: &DisplayInfo) -> DisplayInfo {
    if parsed.width_px.is_none() {
        parsed.width_px = fallback.width_px;
    }
    if parsed.height_px.is_none() {
        parsed.height_px = fallback.height_px;
    }
    parsed
}

fn is_connector_name(name: &str) -> bool {
    // DRM card nodes (`card0`) and render nodes (`renderD128`) have no
    // connector separator. Connector names always contain one, including
    // `card0-DP-1`, `card1-HDMI-A-1`, and virtual `card0-Virtual-1`.
    name.contains('-')
}

fn connector_name(name: &str) -> String {
    name.split_once('-')
        .map_or_else(|| name.to_owned(), |(_, connector)| connector.to_owned())
}

fn read_first_mode(path: &Path) -> Option<(u32, u32)> {
    fs::read_to_string(path).ok()?.lines().find_map(parse_mode)
}

fn read_bounded_bytes(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "display EDID exceeds the bounded reader limit",
        ));
    }
    Ok(bytes)
}

fn parse_mode(line: &str) -> Option<(u32, u32)> {
    let (width, height) = line.trim().split_once('x')?;
    let width = width.parse::<u32>().ok().filter(|value| *value > 0)?;
    let height = height
        .split_whitespace()
        .next()?
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)?;
    Some((width, height))
}

/// Parse the base EDID block and its detailed timing descriptors through the
/// shared portable parser, mapping the proven facts onto this adapter's
/// connector identity. Byte-level EDID interpretation is adapter-neutral; the
/// connector name and the mode fallback stay Linux-owned.
pub(super) fn parse_edid(connector: &str, edid: &[u8]) -> Option<DisplayInfo> {
    let facts = taskmanager_platform_portable::parse_edid(edid)?;
    let EdidFacts {
        manufacturer,
        model,
        serial,
        width_mm,
        height_mm,
        width_px,
        height_px,
        refresh_hz,
        hdr_supported,
    } = facts;
    Some(DisplayInfo {
        connector: connector.to_owned(),
        manufacturer,
        model,
        serial,
        width_mm,
        height_mm,
        width_px,
        height_px,
        refresh_hz,
        hdr_supported,
    })
}

fn select_failure(current: Option<FailureKind>, candidate: FailureKind) -> FailureKind {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => current,
        _ => candidate,
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 8,
        FailureKind::PermissionDenied => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::TimedOut => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn io_failure(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        io::ErrorKind::InvalidData => FailureKind::ProviderFault,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_display_tests.rs"]
mod tests;
