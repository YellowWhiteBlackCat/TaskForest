//! Platform text-rendering compensation policy (CORE-07).
//!
//! The single neutral place where platform rendering facts — rasterizer
//! behavior, and later DPI-class axes — are decided. Every toolkit binding
//! in this crate (`gpui`, `iced`) PROJECTS these decisions; a binding may
//! never embed its own `#[cfg(target_os = …)]` compensation again, because
//! that is how the stem-darkening rule drifted into the gpui side alone.
//!
//! The functions take the platform axis EXPLICITLY so the full decision
//! table runs on every host. The historical test gap: the Windows
//! stem-darkening branch used to live inline behind
//! `#[cfg(target_os = "windows")]`, so Linux hosts only ever executed the
//! uncompensated branch and the compensated one shipped untested on every
//! non-Windows developer machine and CI run.

use crate::color::Weight;

/// The text-rendering axis of a platform: which rasterizer semantics a
/// toolkit's text pipeline inherits there.
///
/// This classifies the PLATFORM, not any toolkit: both toolkit bindings
/// read [`WeightCompensationAxis::target`] and apply the same policy, so a
/// reclassification (if a toolkit's Windows text stack is verified to
/// behave differently) is a one-place change with paired tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightCompensationAxis {
    /// FreeType (Linux and friends): automatic stem darkening on dark
    /// surfaces; fractional variable-font weights render as authored.
    FreeType,
    /// DirectWrite (Windows): no automatic stem darkening, and fractional
    /// variable-font weights without explicit axis interpolation snap
    /// toward Regular (400).
    DirectWrite,
}

impl WeightCompensationAxis {
    /// All axes in stable order; tests iterate this to run the whole
    /// decision table regardless of host.
    pub const ALL: [Self; 2] = [Self::FreeType, Self::DirectWrite];

    /// The axis of the platform being compiled for. The cfg seam lives HERE
    /// and nowhere else, so the toolkit bindings stay pure projections and
    /// tests can still drive both axes explicitly.
    #[cfg(target_os = "windows")]
    pub const fn target() -> Self {
        Self::DirectWrite
    }

    /// The axis of the platform being compiled for (non-Windows variant).
    #[cfg(not(target_os = "windows"))]
    pub const fn target() -> Self {
        Self::FreeType
    }
}

/// Font-weight compensation for one axis — the single source every toolkit
/// binding projects (CORE-07: platform compensation must appear in PAIRS,
/// decided once here, never per-frontend).
///
/// DirectWrite snaps the fractional body weight (450, within ±1) to Medium
/// (500) so visual density matches the FreeType platforms, where FreeType
/// keeps authored fractional weights unchanged.
#[must_use]
pub fn effective_weight(weight: Weight, axis: WeightCompensationAxis) -> Weight {
    match axis {
        WeightCompensationAxis::FreeType => weight,
        WeightCompensationAxis::DirectWrite => {
            if (weight.0 - 450.0).abs() < 1.0 {
                Weight(500.0)
            } else {
                weight
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/theme_platform.rs"]
mod tests;
