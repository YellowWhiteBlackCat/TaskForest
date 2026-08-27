//! Canonical Linux service targets carried opaquely through shared layers.

use taskmanager_core::ServiceId;
use taskmanager_platform_contract::ProviderFailure;

use super::init_runtime::detection_provider_failure;
use super::{InitSystem, ServiceManager};

const SYSTEMD_PREFIX: &str = "linux.service.systemd:";
const OPENRC_PREFIX: &str = "linux.service.openrc:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResolvedServiceTarget {
    init: InitSystem,
    native: String,
}

impl ResolvedServiceTarget {
    pub(super) const fn init(&self) -> InitSystem {
        self.init
    }

    pub(super) fn native(&self) -> &str {
        self.native.as_str()
    }
}

pub(super) fn systemd_unit_id(unit: &str) -> ServiceId {
    ServiceId::new(format!("{SYSTEMD_PREFIX}{unit}"))
}

pub(super) fn systemd_service_id(service: &str) -> ServiceId {
    systemd_unit_id(service)
}

pub(super) fn openrc_service_id(service: &str) -> ServiceId {
    ServiceId::new(format!("{OPENRC_PREFIX}{service}"))
}

pub(super) fn resolve_service_target(
    target: &ServiceId,
) -> Result<ResolvedServiceTarget, ProviderFailure> {
    let encoded = target.as_str();
    let (init, native) = if let Some(native) = encoded.strip_prefix(SYSTEMD_PREFIX) {
        (InitSystem::Systemd, native)
    } else if let Some(native) = encoded.strip_prefix(OPENRC_PREFIX) {
        (InitSystem::Openrc, native)
    } else {
        return Err(ProviderFailure::Rejected);
    };
    let valid = match init {
        InitSystem::Systemd => valid_systemd_service_name(native),
        InitSystem::Openrc => valid_openrc_service_name(native),
        InitSystem::Unsupported => false,
    };
    if !valid || native.contains(SYSTEMD_PREFIX) || native.contains(OPENRC_PREFIX) {
        return Err(ProviderFailure::Rejected);
    }
    Ok(ResolvedServiceTarget {
        init,
        native: native.to_owned(),
    })
}

pub(super) fn resolve_active_service_target(
    target: &ServiceId,
) -> Result<ResolvedServiceTarget, ProviderFailure> {
    resolve_service_target_for_detection(target, ServiceManager::detect_init())
}

pub(super) fn resolve_service_target_for_detection(
    target: &ServiceId,
    detection: Result<InitSystem, taskmanager_core::FailureKind>,
) -> Result<ResolvedServiceTarget, ProviderFailure> {
    let resolved = resolve_service_target(target)?;
    verify_detected_init(detection, resolved.init())?;
    Ok(resolved)
}

fn verify_detected_init(
    detection: Result<InitSystem, taskmanager_core::FailureKind>,
    expected: InitSystem,
) -> Result<(), ProviderFailure> {
    match detection {
        Ok(actual) if actual == expected => Ok(()),
        Ok(InitSystem::Unsupported) => Err(ProviderFailure::Unsupported),
        Ok(_) => Err(ProviderFailure::IdentityChanged),
        Err(failure) => Err(detection_provider_failure(failure)),
    }
}

pub(crate) fn valid_systemd_service_name(native: &str) -> bool {
    native.ends_with(".service") && valid_systemd_unit_name(native)
}

pub(super) fn valid_systemd_unit_name(native: &str) -> bool {
    let Some((stem, unit_type)) = native.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && matches!(
            unit_type,
            "service"
                | "socket"
                | "target"
                | "device"
                | "mount"
                | "automount"
                | "swap"
                | "timer"
                | "path"
                | "slice"
                | "scope"
        )
        && valid_systemd_name_bytes(native.as_bytes())
}

fn valid_systemd_name_bytes(native: &[u8]) -> bool {
    if native.is_empty() || native.len() > 255 || native[0] == b'-' {
        return false;
    }
    let mut index = 0;
    while index < native.len() {
        let byte = native[index];
        if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'@' | b'-') {
            index += 1;
            continue;
        }
        if byte != b'\\'
            || native.get(index + 1) != Some(&b'x')
            || !native.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
            || !native.get(index + 3).is_some_and(u8::is_ascii_hexdigit)
        {
            return false;
        }
        let Some(decoded) = decode_hex(native[index + 2], native[index + 3]) else {
            return false;
        };
        if decoded < 0x20
            || decoded == 0x7f
            || matches!(decoded, b'/' | b'\\' | b'*' | b'?' | b'[' | b']' | b';')
        {
            return false;
        }
        index += 4;
    }
    true
}

fn decode_hex(high: u8, low: u8) -> Option<u8> {
    hex_value(high)?
        .checked_mul(16)?
        .checked_add(hex_value(low)?)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn valid_openrc_service_name(native: &str) -> bool {
    !native.is_empty()
        && native.len() <= 255
        && !native.starts_with('-')
        && native != "."
        && native != ".."
        && native.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        })
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_services_target_tests.rs"]
mod tests;
