//! Cross-frontend conformance vocabulary (P4 evidence closure).
//!
//! [`ContractTag`] is the single authoritative vocabulary for the "contract
//! property" that a cross-frontend evidence anchor proves. The committed
//! declaration manifest (`scripts/parity/cross_frontend_manifest.tsv`) carries
//! these stable ids in its `contract_tag` column; the Python resolver only
//! discovers and counts anchors, and this crate mechanically validates that
//! every declared tag names a known variant. The manifest is a textual artifact
//! whose contract is its text; the tag set lives here and is never copied into
//! TSV, Python, or bash.
//!
//! The vocabulary folds the three sets that were drifting independently:
//!
//! - the shared interaction paths previously copied as `ALLOWED_PATHS` in the
//!   GPUI/Iced matrices (`keyboard`, `pointer`, `focus`, ...);
//! - presentation conformance properties (`accessibility`, `theme`, `chart`);
//! - process-insight honesty domains (`network`, `gpu`, `resources`, ...).
//!
//! ## Honesty boundary
//!
//! This enum proves that a declared anchor names a known contract property. It
//! does NOT prove that the anchor's subject is allowed to carry that property
//! (the `subject -> contract_tags` dimension is future P4 work) and it does NOT
//! prove behavior. A tag is a vocabulary fact, never evidence by itself.

/// A stable contract property proven by a cross-frontend evidence anchor.
///
/// Variants are grouped by family; [`ContractTag::ALL`] is the canonical order
/// used by reports and the manifest validator. The machine ids returned by
/// [`ContractTag::id`] are the contract: renaming one is a hard cutover of every
/// declaration that references it, not a cosmetic refactor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractTag {
    // Shared interaction paths.
    Keyboard,
    Pointer,
    Focus,
    Cancel,
    Failure,
    ProviderGap,
    Recovery,
    Responsive,
    Isolation,
    Lifecycle,
    Toggle,
    Evidence,
    Success,
    // Presentation conformance.
    Accessibility,
    Theme,
    Chart,
    // Process-insight honesty domains.
    Network,
    Gpu,
    Resources,
    IsolationDomain,
    Threads,
    OpenFiles,
    Environment,
    Loading,
    Unavailable,
    Partial,
    Honesty,
    CaptureVisual,
}

impl ContractTag {
    /// Canonical order. Declarations, reports, and the manifest validator fold
    /// against this list, never against a hand-maintained copy.
    pub const ALL: [Self; 28] = [
        Self::Keyboard,
        Self::Pointer,
        Self::Focus,
        Self::Cancel,
        Self::Failure,
        Self::ProviderGap,
        Self::Recovery,
        Self::Responsive,
        Self::Isolation,
        Self::Lifecycle,
        Self::Toggle,
        Self::Evidence,
        Self::Success,
        Self::Accessibility,
        Self::Theme,
        Self::Chart,
        Self::Network,
        Self::Gpu,
        Self::Resources,
        Self::IsolationDomain,
        Self::Threads,
        Self::OpenFiles,
        Self::Environment,
        Self::Loading,
        Self::Unavailable,
        Self::Partial,
        Self::Honesty,
        Self::CaptureVisual,
    ];

    /// Stable machine id used by the evidence manifest and matrix reports.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Pointer => "pointer",
            Self::Focus => "focus",
            Self::Cancel => "cancel",
            Self::Failure => "failure",
            Self::ProviderGap => "provider-gap",
            Self::Recovery => "recovery",
            Self::Responsive => "responsive",
            Self::Isolation => "isolation",
            Self::Lifecycle => "lifecycle",
            Self::Toggle => "toggle",
            Self::Evidence => "evidence",
            Self::Success => "success",
            Self::Accessibility => "accessibility",
            Self::Theme => "theme",
            Self::Chart => "chart",
            Self::Network => "network",
            Self::Gpu => "gpu",
            Self::Resources => "resources",
            Self::IsolationDomain => "isolation-domain",
            Self::Threads => "threads",
            Self::OpenFiles => "open-files",
            Self::Environment => "environment",
            Self::Loading => "loading",
            Self::Unavailable => "unavailable",
            Self::Partial => "partial",
            Self::Honesty => "honesty",
            Self::CaptureVisual => "capture-visual",
        }
    }

    /// Resolve a stable machine id back to its tag.
    ///
    /// This is the membership primitive the manifest validator uses: an id that
    /// does not resolve is an unknown contract tag and must fail the gate.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tag| tag.id() == id)
    }
}

impl core::fmt::Display for ContractTag {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.id())
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_conformance.rs"]
mod tests;
