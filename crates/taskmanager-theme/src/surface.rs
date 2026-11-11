//! Toolkit-neutral roles for glass surfaces and their shared elevation.

use crate::theme::GlassTier;

/// The semantic role of a glass surface.
///
/// Each role resolves to one shared glass tier and one elevation mode so
/// frontends do not independently choose material depth or floating shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassSurfaceRole {
    /// Persistent side navigation or titlebar chrome.
    Chrome,
    /// A page header already inside the page viewport.
    PageHeader,
    /// An anchored popup, context, or dropdown menu panel.
    Menu,
    /// A transient status receipt card.
    Toast,
    /// A hover or keyboard hint card.
    Tooltip,
    /// A true modal dialog panel.
    Dialog,
    /// An inline timeline or plot surface inside an elevated card.
    InlinePlot,
}

impl GlassSurfaceRole {
    /// Every semantic role, for exhaustive adapters and behavior checks.
    pub const ALL: [Self; 7] = [
        Self::Chrome,
        Self::PageHeader,
        Self::Menu,
        Self::Toast,
        Self::Tooltip,
        Self::Dialog,
        Self::InlinePlot,
    ];

    /// The shared glass material tier for this role.
    #[must_use]
    pub const fn tier(self) -> GlassTier {
        match self {
            Self::Chrome => GlassTier::Sidebar,
            Self::Menu | Self::Toast | Self::Tooltip | Self::PageHeader | Self::InlinePlot => {
                GlassTier::Card
            }
            Self::Dialog => GlassTier::Modal,
        }
    }

    /// Whether this surface floats above its backdrop or sits inside another
    /// elevated surface.
    #[must_use]
    pub const fn elevation(self) -> SurfaceElevation {
        match self {
            Self::Menu | Self::Toast | Self::Tooltip | Self::Dialog => SurfaceElevation::Floating,
            Self::Chrome | Self::PageHeader | Self::InlinePlot => SurfaceElevation::Inset,
        }
    }
}

/// How a glass surface carries elevation.
///
/// Floating surfaces cast their tier's shared shadow. Inset surfaces retain
/// the same fill and edge without stacking another shadow inside the parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceElevation {
    /// The surface floats above its backdrop and casts the tier's shadow.
    Floating,
    /// The surface is inset in another surface and casts no shadow.
    Inset,
}

impl SurfaceElevation {
    /// Whether the renderer should project a shadow for this elevation.
    #[must_use]
    pub const fn casts_shadow(self) -> bool {
        matches!(self, Self::Floating)
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_surface_roles.rs"]
mod tests;
