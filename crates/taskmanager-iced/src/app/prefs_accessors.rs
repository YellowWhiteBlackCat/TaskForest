//! Resolved preference/unit accessors for [`IcedApp`], extracted from
//! [`super`] so `app.rs` stays under the source-size budget. Pure read-only
//! projections of the immutable presentation preferences — never perform I/O.

use super::IcedApp;
use super::motion::viewport_compact;
use taskmanager_theme::tokens::UiSize;

impl IcedApp {
    /// Whether the current viewport is at the GPUI compact breakpoint
    /// ([`viewport_compact`]).
    #[must_use]
    pub(crate) fn compact_layout(&self) -> bool {
        viewport_compact(self.viewport.size())
    }

    /// Current viewport width for renderer-local responsive geometry. The
    /// view must not reach into the app's private event state directly.
    #[must_use]
    pub(crate) fn viewport_width(&self) -> f32 {
        self.viewport.size().width
    }

    /// The full tracked viewport size — the one input the Performance page's
    /// typed frame budget ([`crate::ui::responsive::PerformancePageBudget::for_perf_frame`])
    /// allocates its semantic slots from.
    #[must_use]
    pub(crate) fn viewport_size(&self) -> iced::Size {
        self.viewport.size()
    }

    /// Resolved row-density preference for the view layer.
    #[must_use]
    pub fn compact_density(&self) -> bool {
        self.preferences().density.eq_ignore_ascii_case("Compact")
    }

    #[must_use]
    pub fn ui_size(&self) -> UiSize {
        UiSize::from_config_token(&self.preferences().ui_size)
    }

    /// Resolved memory-unit preference for the view layer.
    #[must_use]
    pub fn memory_use_bytes(&self) -> bool {
        self.preferences().memory_use_bytes
    }

    /// Resolved memory ladder preference (base-2 MiB/GiB vs base-10 MB/GB).
    #[must_use]
    pub fn memory_use_base2(&self) -> bool {
        self.preferences().memory_use_base2
    }

    /// Resolved drive unit preference (bytes vs bits).
    #[must_use]
    pub fn drive_use_bytes(&self) -> bool {
        self.preferences().drive_use_bytes
    }

    /// Resolved drive ladder preference (base-2 vs base-10).
    #[must_use]
    pub fn drive_use_base2(&self) -> bool {
        self.preferences().drive_use_base2
    }

    /// Resolved network unit preference (bytes vs bits).
    #[must_use]
    pub fn network_use_bytes(&self) -> bool {
        self.preferences().network_use_bytes
    }

    /// Resolved network ladder preference (base-2 vs base-10).
    #[must_use]
    pub fn network_use_base2(&self) -> bool {
        self.preferences().network_use_base2
    }

    /// Resolved Performance graph window length (clamped by the buffer).
    #[must_use]
    pub fn graph_data_points(&self) -> usize {
        self.preferences().graph_data_points.clamp(10, 600)
    }

    /// Resolved network-graph scaling preference (observed peak vs link speed).
    #[must_use]
    pub fn network_dynamic_scaling(&self) -> bool {
        self.preferences().network_dynamic_scaling
    }

    /// The resolved drive unit pair for the device blocks.
    #[must_use]
    pub(crate) fn drive_units(&self) -> crate::ui::UnitPrefs {
        crate::ui::UnitPrefs {
            use_bytes: self.preferences().drive_use_bytes,
            use_base2: self.preferences().drive_use_base2,
        }
    }

    /// The resolved network unit pair for the device blocks.
    #[must_use]
    pub(crate) fn network_units(&self) -> crate::ui::UnitPrefs {
        crate::ui::UnitPrefs {
            use_bytes: self.preferences().network_use_bytes,
            use_base2: self.preferences().network_use_base2,
        }
    }

    /// The resolved graph presentation for the device mini-graphs: smoothing
    /// from the preference, everything else on the `GraphPrefs` defaults
    /// (hover off — only the main per-device graphs flip it on).
    #[must_use]
    pub(crate) fn graph_prefs(&self) -> crate::ui::GraphPrefs {
        crate::ui::GraphPrefs {
            smooth: true,
            ..crate::ui::GraphPrefs::default()
        }
    }
}
