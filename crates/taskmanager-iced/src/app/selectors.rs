//! Frontend-local selectors for [`super::IcedApp`]: Performance resources and
//! the shared Applications status filter vocabulary. The Applications row
//! hierarchy itself is canonical and therefore has no selector state.
//! Extracted from [`super`] so the state + update module stays the entry point.

// The shell owns the filtered row projection and this frontend owns the
// segmented control that selects it. Re-export the type at the app boundary so
// the Iced message/focus vocabulary stays toolkit-local without duplicating the
// classifier.
pub use taskmanager_shell::ProcessStatusFilter;

/// The Performance-page resource selector — the MC select-a-device detail
/// model. Frontend-local state: which resource's detail panel renders. `Cpu`/
/// `Memory` are singleton devices; every dynamic variant carries the selected
/// device index just like GPUI's `SelectedDevice::Disk(i)` / `Nic(i)` /
/// `Gpu(i)` / `Battery(i)` / `Fan(i)`. Default is `Cpu` (MC's default view).
/// This never crosses into the shared shell, so the selector stays
/// parallel-safe across frontends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PerfDevice {
    #[default]
    Cpu,
    Memory,
    Disk(usize),
    Network(usize),
    Gpu(usize),
    /// Stored-energy power supplies (batteries / UPS). Reads
    /// `SystemProjectionStore.power_supplies`; an honest "no battery" state renders when
    /// no power supply has been observed.
    Battery(usize),
    /// Fan channels from the shared sensor snapshot (`SystemProjectionStore.sensors`); an
    /// honest "no fan detected" state renders when no sensor batch has landed.
    Fan(usize),
}

impl PerfDevice {
    /// Every selectable Performance resource, in tab order. The selector row,
    /// the focus-target registry and the tests iterate this so no variant can
    /// be silently dropped (the anti-报菜名 enumeration rule).
    pub const ALL: [Self; 7] = [
        Self::Cpu,
        Self::Memory,
        Self::Disk(0),
        Self::Network(0),
        Self::Gpu(0),
        Self::Battery(0),
        Self::Fan(0),
    ];

    /// Stable non-localized identifier for focus-operation IDs. Never rendered.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk(_) => "disk",
            Self::Network(_) => "network",
            Self::Gpu(_) => "gpu",
            Self::Battery(_) => "battery",
            Self::Fan(_) => "fan",
        }
    }

    /// The selected device index for dynamic resources; singleton CPU/Memory
    /// pages intentionally return `None`.
    #[must_use]
    pub const fn index(self) -> Option<usize> {
        match self {
            Self::Cpu | Self::Memory => None,
            Self::Disk(index)
            | Self::Network(index)
            | Self::Gpu(index)
            | Self::Battery(index)
            | Self::Fan(index) => Some(index),
        }
    }
}
