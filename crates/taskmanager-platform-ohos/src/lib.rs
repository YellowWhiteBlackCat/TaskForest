#![forbid(unsafe_code)]

//! Standalone OpenHarmony platform-adapter seam.
//!
//! The first milestone intentionally contains no OS provider or UI code. It
//! proves that the shared application/runtime contract can be named by an
//! OHOS-specific crate without selecting the Linux adapter by accident. CPU
//! and memory telemetry are currently deferred until a maintained safe wrapper
//! or a separately audited OHOS native boundary exists. Until an OHOS provider
//! has a verified native source, the runtime exposes no capabilities and the
//! application reports typed `UnsupportedCapability` outcomes through the
//! existing absent-handle contract.

use taskmanager_application::PlatformHandle;
use taskmanager_platform_runtime::{CompositionError, capability_absent_handle};

/// OpenHarmony runtime composition root.
///
/// Provider registration is intentionally empty in this milestone. Keeping
/// the return type identical to the other native adapters lets a later
/// provider registry be introduced without changing the application port
/// boundary.
pub struct OhosPlatformRuntime;

impl OhosPlatformRuntime {
    /// Construct the current OHOS runtime.
    ///
    /// No provider is claimed to exist yet, so the returned handle has an
    /// empty capability catalog, no request ports, and an idle event port.
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        Ok(capability_absent_handle())
    }
}

/// Target-neutral name reserved for the future native selector.
pub struct NativePlatformRuntime;

impl NativePlatformRuntime {
    /// Construct the current OHOS runtime.
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        OhosPlatformRuntime::spawn()
    }
}
