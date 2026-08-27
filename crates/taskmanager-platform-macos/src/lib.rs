//! macOS composition of application platform ports.
//!
//! This is the second operating-system adapter behind the shared provider SPI
//! and runtime. It implements the complete platform-neutral provider surface
//! from `taskmanager-platform-provider` and composes the full standard product
//! capability set through `taskmanager-platform-runtime`.
//!
//! Most domains are implemented through safe wrapper crates (sysinfo for
//! CPU/memory/process/storage/network, plus `starship-battery`, `open`, `plist`) and
//! bounded `std::process::Command` shell-outs to macOS system tools
//! (`launchctl`/`smartctl`/`system_profiler`/`networksetup`/`renice`/`ps`) —
//! the same safe route-C pattern the Linux adapter uses for `systemctl`, with
//! `#![forbid(unsafe_code)]` and no hand-written FFI (ADR-019). Capabilities
//! that still have no safe macOS source complete with a typed unsupported
//! outcome attributed to a `macos.*` provider — never a fabricated
//! observation. The remaining typed-Unsupported capabilities are: GPU
//! telemetry, container rollup (cgroup-v2 is Linux-only), per-process network
//! accounting, per-process GPU memory, process isolation, the standalone
//! per-process threads insight, CPU affinity, resource-control limits, the
//! per-process network escalation chain, service dependencies, log streaming,
//! session control, the per-fd open-files insight, desktop notifications, and
//! first-run setup. The last three are registered-pending optional facets
//! (2026-08-14, G-05): their descriptors exist so catalog enumeration stays
//! honest, but submissions still complete with typed `Unsupported` outcomes.
//! The authoritative enumeration lives in
//! `tests/contract.rs::PENDING_CAPABILITIES`.

#![forbid(unsafe_code)]

#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!("release builds require `hardware-all`; hardware backends are runtime-selected");

use taskmanager_application::PlatformHandle;
use taskmanager_platform_runtime::{
    CompositionError, NativeProviderSet, RuntimeConfig, RuntimeExecutors, RuntimeProviderBindings,
    assemble_native_runtime,
};

mod bindings;
mod config;
mod provider;
// Neutral system-tray seam implementation (NSStatusItem via tray-icon,
// ADR-0NN); the frontends reach it through `taskmanager-platform-native::tray`.
pub mod tray;
// Single-instance adapter (per-user Unix socket, borrowed core from
// tauri-plugin-single-instance; ADR-032 follow-up).
pub mod instance;

pub use config::{user_config_path, user_history_dir};
pub use provider::MacOsProviderRegistry;

fn wall_clock_ms() -> u64 {
    taskmanager_core::core::time::unix_millis(std::time::SystemTime::now())
}

/// Composition entry point for the second-OS macOS runtime.
pub struct MacOsPlatformRuntime;

impl MacOsPlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        Self::spawn_with_providers(provider::macos_provider_registry())
    }

    pub fn spawn_with_providers(
        providers: MacOsProviderRegistry,
    ) -> Result<PlatformHandle, CompositionError> {
        spawn_runtime(providers)
    }
}

/// The target-neutral composition name exposed by
/// `taskmanager-platform-native`.
pub struct NativePlatformRuntime;

impl NativePlatformRuntime {
    pub fn spawn() -> Result<PlatformHandle, CompositionError> {
        MacOsPlatformRuntime::spawn()
    }
}

fn spawn_runtime(providers: MacOsProviderRegistry) -> Result<PlatformHandle, CompositionError> {
    assemble_native_runtime(providers, RuntimeConfig::new(wall_clock_ms))
}

impl NativeProviderSet for MacOsProviderRegistry {
    fn runtime_provider_bindings(&self) -> RuntimeProviderBindings {
        bindings::runtime_provider_bindings(self)
    }

    fn into_runtime_executors(self) -> RuntimeExecutors {
        let MacOsProviderRegistry {
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
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
