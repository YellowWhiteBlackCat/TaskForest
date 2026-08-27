//! Windows composition of application platform ports.
//!
//! This adapter implements the platform-neutral provider surface from
//! `taskmanager-platform-provider` using published safe wrapper crates plus
//! the narrow audited `taskmanager-windows-api` boundary (performance/locale,
//! Known Folder, exact-process control, WTS, processor topology/cache and NIC
//! metadata). `sysinfo` supplies CPU/memory/process/network/disk,
//! `raw-cpuid` supplies advertised CPU frequency facts, `nvml-wrapper` supplies NVIDIA GPU,
//! `windows-registry` supplies startup/theme facts, `battery` supplies power,
//! and `open` supplies URL opening.
//!
//! Windows APIs without an accepted safe wrapper are NOT called. The
//! corresponding providers register with honest typed unsupported outcomes and the full
//! gap inventory lives in `adr/018-windows-telemetry-safety.md` —
//! nothing is fabricated, and no hand-written FFI module exists in this crate.
//! The remaining registered-pending optional facets are the per-fd
//! open-files insight and first-run setup; desktop notifications and
//! directory usage are real capabilities. Their
//! descriptors exist so catalog enumeration stays honest, but pending
//! submissions still complete with typed `Unsupported` outcomes. The
//! authoritative enumeration lives in
//! `tests/contract.rs::PENDING_CAPABILITIES`.

#![forbid(unsafe_code)]
// Like the macOS adapter, this crate compiles on EVERY target so its
// second-OS contract proof (tests/contract.rs) runs on the binding Linux CI
// gate. Windows-only safe wrappers are `[target.'cfg(windows)'.dependencies]`;
// their provider impls are `#[cfg(windows)]` with honest `MissingDependency`
// fallbacks elsewhere. sysinfo / battery / open / nvml-wrapper are
// cross-platform and compile everywhere (NVML degrades to `None` without a
// driver).

#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!("release builds require `hardware-all`; hardware backends are runtime-selected");

use taskmanager_application::PlatformHandle;
use taskmanager_platform_runtime::{
    CompositionError, NativeProviderSet, RuntimeConfig, RuntimeExecutors, RuntimeProviderBindings,
    assemble_native_runtime,
};

mod bindings;
mod command;
mod config;
mod local_time;
mod provider;
pub mod tray;
// Single-instance adapter (named mutex + named-event handoff, borrowed core
// from tauri-plugin-single-instance; ADR-032 follow-up).
pub mod instance;

pub use config::{user_config_path, user_history_dir, user_locale_name};
pub use local_time::local_time_rules;
pub use provider::WindowsProviderRegistry;

fn wall_clock_ms() -> u64 {
    taskmanager_core::core::time::unix_millis(std::time::SystemTime::now())
}

/// Composition entry point for the Windows runtime.
pub struct WindowsPlatformRuntime;

impl WindowsPlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        Self::spawn_with_providers(provider::windows_provider_registry())
    }

    pub fn spawn_with_providers(
        providers: WindowsProviderRegistry,
    ) -> Result<PlatformHandle, CompositionError> {
        spawn_runtime(providers)
    }
}

/// The target-neutral composition name exposed by
/// `taskmanager-platform-native`.
pub struct NativePlatformRuntime;

impl NativePlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        WindowsPlatformRuntime::spawn()
    }
}

fn spawn_runtime(providers: WindowsProviderRegistry) -> Result<PlatformHandle, CompositionError> {
    assemble_native_runtime(providers, RuntimeConfig::new(wall_clock_ms))
}

impl NativeProviderSet for WindowsProviderRegistry {
    fn runtime_provider_bindings(&self) -> RuntimeProviderBindings {
        bindings::runtime_provider_bindings(self)
    }

    fn into_runtime_executors(self) -> RuntimeExecutors {
        let WindowsProviderRegistry {
            system,
            processes,
            services,
            environment,
            integrations,
            storage,
            sensors,
            power,
        } = self;
        RuntimeExecutors {
            system: system.into_runtime(),
            process: processes.into_runtime(),
            service: services.into_runtime(),
            environment: environment.into_runtime(),
            integration: integrations.into_runtime(),
            storage: storage.into_runtime(),
            sensor: sensors.into_runtime(),
            power: power.into_runtime(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/platform_windows_lib.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
