use taskmanager_core::core::hardware::CpuType;

pub mod cpu;
pub mod disk;
#[cfg(target_os = "linux")]
mod display;
#[cfg(not(target_os = "linux"))]
mod display {
    use std::path::Path;

    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::hardware::DisplayInfo;
    use taskmanager_core::core::source::SourceOutcome;

    #[derive(Debug, Default)]
    pub(super) struct WaylandSessionFacts {
        pub compositor: Option<String>,
        pub compositor_backend: Option<String>,
    }

    pub(super) fn probe_wayland() -> Option<WaylandSessionFacts> {
        None
    }

    pub(super) fn collect_displays(_root: &Path) -> (Vec<DisplayInfo>, SourceOutcome) {
        (
            Vec::new(),
            SourceOutcome::Unavailable(FailureKind::Unsupported),
        )
    }

    pub(super) fn merge_wayland_facts(
        _displays: &mut Vec<DisplayInfo>,
        _session: &WaylandSessionFacts,
    ) {
    }
}
mod firmware;
pub mod gpu;
mod inventory;
pub mod network;
mod pci_ids;
pub mod platform;

pub use cpu::*;
pub use disk::*;
#[cfg(not(feature = "test-support"))]
pub(crate) use gpu::*;
#[cfg(feature = "test-support")]
pub use gpu::*;
pub use inventory::HardwareInventoryCollector;
pub use network::*;
pub use platform::*;
