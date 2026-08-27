//! Shared, Linux-internal NVML failure vocabulary.
//!
//! The dynamically loaded library and vendor errors do not cross the platform
//! boundary. Device and per-process GPU providers map them into this small
//! capability vocabulary before producing shared domain facts.

use nvml_wrapper::error::NvmlError;
use taskmanager_core::{DeviceStatus, FailureKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NvmlFailureKind {
    Unsupported,
    NotSupported,
    MissingLibrary,
    PermissionDenied,
    Transient,
}

impl NvmlFailureKind {
    pub(crate) const fn device_status(self) -> DeviceStatus {
        match self {
            Self::Unsupported | Self::NotSupported => DeviceStatus::Unsupported,
            Self::MissingLibrary => DeviceStatus::MissingTool,
            Self::PermissionDenied => DeviceStatus::PermissionDenied,
            Self::Transient => DeviceStatus::Stale,
        }
    }

    pub(crate) const fn failure_kind(self) -> FailureKind {
        match self {
            Self::Unsupported | Self::NotSupported => FailureKind::Unsupported,
            Self::MissingLibrary => FailureKind::MissingDependency,
            Self::PermissionDenied => FailureKind::PermissionDenied,
            Self::Transient => FailureKind::TemporarilyUnavailable,
        }
    }
}

pub(crate) fn classify_error(error: &NvmlError) -> NvmlFailureKind {
    match error {
        NvmlError::NotSupported => NvmlFailureKind::NotSupported,
        NvmlError::LibraryNotFound
        | NvmlError::LibloadingError(_)
        | NvmlError::FailedToLoadSymbol(_)
        | NvmlError::FunctionNotFound
        | NvmlError::DriverNotLoaded => NvmlFailureKind::MissingLibrary,
        NvmlError::NoPermission | NvmlError::OperatingSystem => NvmlFailureKind::PermissionDenied,
        _ => NvmlFailureKind::Transient,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_nvml_tests.rs"]
mod tests;
