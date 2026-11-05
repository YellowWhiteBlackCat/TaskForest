//! The one layout vocabulary every frontend classifies with.
//!
//! These are the shared horizontal/vertical capacity facts: the buckets,
//! the bands that are common to every shape, and the pure classification
//! folds. A shape may spend its OWN measured thresholds (recorded and
//! reason-checked in [`crate::presentation::thresholds`]), but it must not
//! redefine what a bucket means, how the axes fold into a product bit, or
//! which rung drops which obligation. Prose that claims another shape is
//! "at parity" is a defect (ARCH § 8.3): parity lives in this module plus
//! the registry, and the cross-shape matrix in
//! [`crate::presentation::budget`] proves it.

// ─────────────────────────────────────────────────────────────────────────────
// Bands shared by every shape (identical by construction, so they are stated
// exactly once here). Per-shape measured thresholds are NOT here; they live in
// the threshold registry.

/// Smallest window the desktop shapes accept as a viable canvas.
pub const MIN_WINDOW_WIDTH: f32 = 720.0;
/// Smallest window height the desktop shapes accept.
pub const MIN_WINDOW_HEIGHT: f32 = 480.0;
/// Largest capture/launch width the window parsers clamp to.
pub const MAX_WINDOW_WIDTH: f32 = 3840.0;
/// Largest capture/launch height the window parsers clamp to.
pub const MAX_WINDOW_HEIGHT: f32 = 2160.0;

/// Navigation is a layout region, not a pile of independently sized controls.
/// These are the only rail-width decisions; the page body always owns the
/// remaining space.
pub const NAV_RAIL_COMPACT_WIDTH: f32 = 54.0;
/// The labelled navigation rail width.
pub const NAV_RAIL_WIDTH: f32 = 144.0;

/// Width below which the page slot has room for a single mandatory column
/// only: chrome collapses to icons, secondary groups drop whole.
pub const ULTRA_COMPACT_CONTENT_WIDTH: f32 = 840.0;
/// Width at or above which a page column admits multi-line prose detail
/// cards: below this the column wraps prose hard enough that such cards must
/// degrade to their compact (bar-only) form regardless of height.
pub const COMPACT_CONTENT_WIDTH: f32 = 1080.0;
/// Width at or above which a page earns the full desktop composition.
pub const WIDE_CONTENT_WIDTH: f32 = 1600.0;

/// The minimum band between the last page row and the bottom edge. The
/// elastic-layout playbook makes the bottom safety band a BUDGET fact, not a
/// rendering accident: it is never restored by clipping or by a late inset.
pub const BOTTOM_SAFETY_BAND: f32 = 8.0;

/// Main viewport floor for the Performance workspace once the page earns the
/// Compact profile.
pub const PERFORMANCE_MAIN_MIN_WIDTH: f32 = 360.0;
/// Main viewport floor while the page is still Ultra-Compact: the headline
/// graph survives at a narrower readable floor than the desktop default.
pub const PERFORMANCE_ULTRA_COMPACT_MAIN_MIN_WIDTH: f32 = 320.0;
/// The gap the Performance slot allocation charges between its columns.
pub const PERFORMANCE_SLOT_GAP: f32 = 12.0;
/// Ceiling for the persisted device-rail preference.
pub const PERFORMANCE_SIDEBAR_MAX_WIDTH: f32 = 460.0;
/// Content height the Performance core stack needs: title row + header band
/// (readouts / composition) + headline tier floor + summary row + shared gaps.
/// Below this the page degrades to the [`PerformanceVerticalRunway::Floor`]
/// rung instead of clipping the stack.
pub const PERFORMANCE_RUNWAY_CORE_FLOOR: f32 = 380.0;

/// Horizontal layout capacity: the width axis every page consumes.
///
/// Height is deliberately not folded into this enum. A very wide, short
/// window still has enough horizontal room to combine page chrome even when
/// its vertical budget requires secondary content to collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutProfile {
    UltraCompact,
    Compact,
    Standard,
    Wide,
}

impl LayoutProfile {
    /// Every horizontal bucket, in degradation order.
    pub const ALL: [Self; 4] = [
        Self::UltraCompact,
        Self::Compact,
        Self::Standard,
        Self::Wide,
    ];

    /// Bucket one measured content width into the shared horizontal profile.
    /// The input is the page slot AFTER the shape's own chrome deduction,
    /// never the raw window width.
    #[must_use]
    pub const fn for_content_width(width: f32) -> Self {
        if width < ULTRA_COMPACT_CONTENT_WIDTH {
            Self::UltraCompact
        } else if width < COMPACT_CONTENT_WIDTH {
            Self::Compact
        } else if width < WIDE_CONTENT_WIDTH {
            Self::Standard
        } else {
            Self::Wide
        }
    }

    /// The page padding this profile spends. One fold for every shape: the
    /// horizontal capacity decision and the inset it earns never drift apart.
    #[must_use]
    pub const fn page_padding(self) -> f32 {
        match self {
            Self::UltraCompact => 8.0,
            Self::Compact => 12.0,
            Self::Standard | Self::Wide => 16.0,
        }
    }

    /// Whether a page column may admit multi-line prose detail cards.
    #[must_use]
    pub const fn admits_prose_detail(self) -> bool {
        !matches!(self, Self::UltraCompact)
    }

    /// The stable transport token of this bucket.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::UltraCompact => "ultra-compact",
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::Wide => "wide",
        }
    }

    /// Parse a transport token. An unknown token is a HARD failure for the
    /// caller: a capture that carried a bucket the contract does not know is
    /// evidence of drift, not of a benign default.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "ultra-compact" => Some(Self::UltraCompact),
            "compact" => Some(Self::Compact),
            "standard" => Some(Self::Standard),
            "wide" => Some(Self::Wide),
            _ => None,
        }
    }
}

/// Shared naming for [`LayoutProfile::for_content_width`] at a call site that
/// reads a raw width rather than a toolkit size.
#[must_use]
pub const fn layout_profile_for_width(width: f32) -> LayoutProfile {
    LayoutProfile::for_content_width(width)
}

/// Independent vertical capacity. Page projections use this axis to collapse
/// secondary regions without discarding available horizontal space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerticalSpace {
    Constrained,
    Standard,
    Generous,
}

impl VerticalSpace {
    /// Every vertical bucket, in degradation order.
    pub const ALL: [Self; 3] = [Self::Constrained, Self::Standard, Self::Generous];

    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Constrained => "constrained",
            Self::Standard => "standard",
            Self::Generous => "generous",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "constrained" => Some(Self::Constrained),
            "standard" => Some(Self::Standard),
            "generous" => Some(Self::Generous),
            _ => None,
        }
    }
}

/// Root navigation rail presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationPresentation {
    IconOnly,
    Labeled,
}

impl NavigationPresentation {
    /// The one rail-width decision per navigation presentation.
    #[must_use]
    pub const fn rail_width(self) -> f32 {
        match self {
            Self::IconOnly => NAV_RAIL_COMPACT_WIDTH,
            Self::Labeled => NAV_RAIL_WIDTH,
        }
    }
}

impl DeviceNavigationPresentation {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Strip => "strip",
            Self::Sidebar => "sidebar",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "strip" => Some(Self::Strip),
            "sidebar" => Some(Self::Sidebar),
            _ => None,
        }
    }
}

impl PerformanceDetailsPresentation {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Pinned => "pinned",
            Self::Stacked => "stacked",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "hidden" => Some(Self::Hidden),
            "pinned" => Some(Self::Pinned),
            "stacked" => Some(Self::Stacked),
            _ => None,
        }
    }
}

impl PerformanceChartInventory {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::AggregateOnly => "aggregate-only",
            Self::Full => "full",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "aggregate-only" => Some(Self::AggregateOnly),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

impl PerformanceVerticalRunway {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Core => "core",
            Self::Charts => "charts",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "floor" => Some(Self::Floor),
            "core" => Some(Self::Core),
            "charts" => Some(Self::Charts),
            _ => None,
        }
    }
}

/// The root chrome's row ownership for one frame.
///
/// A shape that composes its chrome from real measured regions (GPUI's
/// titlebar/alert deduction) declares this axis `Absent`; a shape that wraps
/// its route strip and action toolbar onto bounded rows classifies with
/// [`ChromePresentation::for_width`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromePresentation {
    /// Routes get their own bounded strip and actions get wrapped full-width
    /// rows.
    Wrapped,
    /// One desktop row: routes left, actions pinned to the trailing edge.
    SingleRow,
}

impl ChromePresentation {
    /// Map one frame width onto the chrome presentation using the shape's
    /// registered single-row threshold. `None` means the shape does not spend
    /// this axis and keeps its declared chrome composition.
    #[must_use]
    pub const fn for_width(width: f32, single_row_min_width: Option<f32>) -> Option<Self> {
        match single_row_min_width {
            Some(threshold) => {
                if width < threshold {
                    Some(Self::Wrapped)
                } else {
                    Some(Self::SingleRow)
                }
            }
            None => None,
        }
    }

    /// Whether this frame wraps the chrome onto bounded rows.
    #[must_use]
    pub const fn is_wrapped(self) -> bool {
        matches!(self, Self::Wrapped)
    }
}

/// Performance device-navigation presentation: the same devices stay
/// reachable in either form, so this is a geometry decision, not a data one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceNavigationPresentation {
    /// Horizontally windowed pill strip stacked above the detail.
    Strip,
    /// Vertical device-card rail beside the detail.
    Sidebar,
}

/// Performance detail/statistics rail presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceDetailsPresentation {
    /// No room for the rail at all: the facts it carries stay available
    /// through the headline readouts, never silently clipped.
    Hidden,
    /// Beside the main viewport in its own bounded column.
    Pinned,
    /// Below the main viewport when horizontal capacity cannot support two
    /// readable columns. Keeps the information available without starving
    /// the primary graph.
    Stacked,
}

/// Performance chart inventory: one product bit folded from BOTH axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceChartInventory {
    /// Aggregate series only: the secondary charts and per-core matrix drop.
    AggregateOnly,
    /// The full inventory composes.
    Full,
}

/// The Performance page's typed vertical degradation ladder — the height axis
/// of the minimum-space doctrine (ADR-039). Each rung names exactly which
/// fixed obligations of the page may still render; renderers consume the rung,
/// never the raw pixel height.
///
/// Ordering is the degradation order: content drops from Charts down to Floor,
/// and the headline surface's tier floor is never touched (that is the window
/// layer's guarantee, not this ladder's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerformanceVerticalRunway {
    /// Minimum viable page: title row + headline surface only. The header band
    /// and per-chart summary rows drop first — explicitly, not by silent
    /// clipping.
    Floor,
    /// The core stack composes: title row + header band (readouts /
    /// composition) + headline surface with its summary row. Secondary charts
    /// and below content drop so the fixed viewport cannot clip them.
    Core,
    /// The full chart inventory fits: the core stack plus secondary charts and
    /// the per-core matrix. This is the rung [`PerformanceChartInventory::Full`]
    /// requires from the height axis.
    Charts,
}

impl PerformanceVerticalRunway {
    /// Every rung, in degradation order.
    pub const ALL: [Self; 3] = [Self::Floor, Self::Core, Self::Charts];

    /// Classify from the page-slot content height against the shape's
    /// registered Charts-rung threshold. The Core threshold is the composed
    /// core-stack floor and is shared.
    #[must_use]
    pub const fn for_content_height(height: f32, charts_rung_content_height: f32) -> Self {
        if height >= charts_rung_content_height {
            Self::Charts
        } else if height >= PERFORMANCE_RUNWAY_CORE_FLOOR {
            Self::Core
        } else {
            Self::Floor
        }
    }

    /// Whether the core stack (header band + summaries) may render.
    #[must_use]
    pub const fn carries_core_stack(self) -> bool {
        matches!(self, Self::Core | Self::Charts)
    }

    /// Whether a page may admit content below its headline surface. Only the
    /// full Charts runway has enough named capacity for that optional band;
    /// Core is intentionally headline-only, which makes the no-scroll
    /// Performance contract explicit at the shared boundary.
    #[must_use]
    pub const fn carries_below(self) -> bool {
        matches!(self, Self::Charts)
    }
}

/// Fold the two typed axes into the one chart-inventory product bit. The width
/// axis admits the full inventory only from the Compact profile up; the height
/// axis needs the Charts runway. Two named reasons in, one bit out — no axis
/// conflation at the derivation site.
#[must_use]
pub const fn chart_inventory_from_axes(
    profile: LayoutProfile,
    vertical: PerformanceVerticalRunway,
) -> PerformanceChartInventory {
    match (profile, vertical) {
        (
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
            PerformanceVerticalRunway::Charts,
        ) => PerformanceChartInventory::Full,
        (LayoutProfile::UltraCompact, _)
        | (_, PerformanceVerticalRunway::Core | PerformanceVerticalRunway::Floor) => {
            PerformanceChartInventory::AggregateOnly
        }
    }
}

/// System-page surface column count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemSurfacePresentation {
    SingleColumn,
    MultiColumn,
}

/// The system page's surface fold: one column while Ultra-Compact, the card
/// grid from Compact up.
#[must_use]
pub const fn system_surfaces_from_profile(profile: LayoutProfile) -> SystemSurfacePresentation {
    match profile {
        LayoutProfile::UltraCompact => SystemSurfacePresentation::SingleColumn,
        LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
            SystemSurfacePresentation::MultiColumn
        }
    }
}

/// The device-navigation fold for the coarse, profile-only constructor: no
/// pixel width is known there, so the rail form follows the bucket alone.
#[must_use]
pub const fn device_navigation_from_profile(
    profile: LayoutProfile,
) -> DeviceNavigationPresentation {
    match profile {
        LayoutProfile::UltraCompact => DeviceNavigationPresentation::Strip,
        LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
            DeviceNavigationPresentation::Sidebar
        }
    }
}

/// The details-rail fold for the coarse, profile-only constructor.
#[must_use]
pub const fn details_from_profile(profile: LayoutProfile) -> PerformanceDetailsPresentation {
    match profile {
        LayoutProfile::UltraCompact => PerformanceDetailsPresentation::Hidden,
        LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
            PerformanceDetailsPresentation::Pinned
        }
    }
}

/// The vertical-rung fold for the coarse, profile-only constructor: the legacy
/// path has no pixel height, so the coarse classification maps Constrained to
/// the Core runway and every other band to Charts.
#[must_use]
pub const fn runway_from_vertical_space(vertical: VerticalSpace) -> PerformanceVerticalRunway {
    match vertical {
        VerticalSpace::Constrained => PerformanceVerticalRunway::Core,
        VerticalSpace::Standard | VerticalSpace::Generous => PerformanceVerticalRunway::Charts,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_vocabulary.rs"]
mod tests;
