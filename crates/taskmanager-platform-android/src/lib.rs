#![forbid(unsafe_code)]

//! Standalone Android platform-provider seam.
//!
//! The `android-provider` feature is intentionally opt-in. The current
//! milestone proves only that Android has a dedicated composition boundary and
//! that the shared core/application/runtime contracts can be named without
//! selecting Linux or desktop facilities. Until Android sources and their
//! permission/lifecycle semantics are verified on a target device, this
//! runtime exposes no capabilities and returns typed unsupported outcomes.

use taskmanager_application::PlatformHandle;
use taskmanager_platform_runtime::{CompositionError, capability_absent_handle};

/// Cargo feature name for the experimental Android provider seam.
pub const ANDROID_PROVIDER_FEATURE: &str = "android-provider";

/// Whether the explicit Android provider feature is enabled for this build.
#[must_use]
pub const fn provider_feature_enabled() -> bool {
    cfg!(feature = "android-provider")
}

/// Android runtime composition root.
pub struct AndroidPlatformRuntime;

impl AndroidPlatformRuntime {
    /// Construct the current Android runtime.
    ///
    /// No capability is claimed until an Android source has a verified
    /// permission, identity, lifecycle, and failure contract.
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        Ok(capability_absent_handle())
    }
}

/// Target-neutral name reserved for the future native selector.
pub struct NativePlatformRuntime;

impl NativePlatformRuntime {
    /// Construct the current Android runtime.
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        AndroidPlatformRuntime::spawn()
    }
}
