//! Component/surface capability coverage contract (the parity registry,
//! CORE-08).
//!
//! The keybindings matrix proves every command is explicitly bound or
//! deliberately unbound in every frontend shape; this module extends the
//! same anti-silence fold to component and surface CAPABILITIES. For every
//! capability in [`ComponentCapability::ALL`], every frontend must declare
//! one explicit [`CapabilitySupport`] decision — a silent omission is
//! drift, not a choice. Deliberate differences must carry a reason
//! ([`CapabilitySupport::Divergent`] / [`CapabilitySupport::Unsupported`]);
//! comments claiming parity do not count.
//!
//! ## The reference shape (GPUI-05)
//!
//! `taskmanager-ui` — the GPUI component layer built directly on gpui
//! (gpui 0.2.2 ships none of these components; they are hand-built there) —
//! is the SEMANTIC REFERENCE SOURCE for the parallel component layers
//! (Iced, TUI, Bevy). Each capability names its reference component through
//! [`ComponentCapability::reference_path`], and only the GPUI shape may
//! declare [`CapabilitySupport::Reference`]. A parallel frontend that
//! ports the semantics declares [`CapabilitySupport::Ported`]; a frontend
//! that must diverge declares [`CapabilitySupport::Divergent`] with the
//! driver. When porting work uncovers a gap or a bug in the reference, the
//! resolution is a write-back: fix `taskmanager-ui` (and this contract if
//! the vocabulary itself is wrong) FIRST, then port the fix — a silent
//! per-frontend fork of a reference semantic is exactly what this registry
//! exists to make impossible. Because the GPUI declaration must cover every
//! capability with `Reference`/`Native`, the vocabulary can only grow when
//! the reference layer grows.
//!
//! ## Honesty boundary
//!
//! This registry proves DECLARATION discipline — no silence, no
//! unexplained divergence, no reference-less capability. It does NOT prove
//! behavioral equivalence: each cell is backed by the shape's own behavior
//! tests and evidence route (CORE-06), and a `Ported` cell claims the
//! intent to match, never the match itself.

use crate::keybindings::FrontendShape;

impl FrontendShape {
    /// The shape whose component layer (`taskmanager-ui`) is the semantic
    /// reference for every capability (GPUI-05): only this shape may
    /// declare [`CapabilitySupport::Reference`].
    #[must_use]
    pub const fn is_capability_reference_shape(self) -> bool {
        matches!(self, Self::Gpui)
    }
}

/// One component/surface capability the product offers through its
/// frontends. The set starts from what at least one shape really ships —
/// the gate forbids silence, not absence, so capabilities join this list
/// only when a shape actually implements (or needs to refuse) them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentCapability {
    /// A modal layer-stacked dialog with scrim and focus containment.
    ModalOverlay,
    /// A menu of actions for a targeted object (pointer context menu or its
    /// keyboard equivalent).
    ContextMenu,
    /// A menu surface anchored to a control (e.g. a table-column menu).
    DropdownMenu,
    /// Hover-anchored transient explanation surface.
    Tooltip,
    /// Floating transient notification.
    Toast,
    /// Single-line editable text field.
    TextInput,
    /// Type-to-filter query field.
    SearchInput,
    /// Read-only selectable/copyable text.
    TextSelection,
    /// Two-state toggle control.
    Switch,
    /// Analog/bounded-step range control.
    Slider,
    /// Multi-state check control.
    Checkbox,
    /// Single-choice control over an enumerated set.
    Select,
    /// Compact exclusive choice group (tabs-like).
    SegmentedControl,
    /// Typed-column table with sticky header, row identity and selection.
    Table,
    /// Pointer-driven resize of a resizable table column through its trailing
    /// edge. Width persistence and page-specific column identity remain
    /// frontend-owned concerns.
    ColumnDragResize,
    /// Bounded window rendering over a large list.
    VirtualList,
    /// Recursive expand/collapse structure.
    Tree,
    /// Scroll affordance bound to a tracked viewport.
    Scrollbar,
    /// Input-modality-aware focus indication (keyboard-visible ring).
    FocusVisible,
}

impl ComponentCapability {
    /// Every capability in canonical order; declarations fold against this
    /// set the way binding declarations fold against the shared command
    /// set (`CommandId::ALL`).
    pub const ALL: &'static [Self] = &[
        Self::ModalOverlay,
        Self::ContextMenu,
        Self::DropdownMenu,
        Self::Tooltip,
        Self::Toast,
        Self::TextInput,
        Self::SearchInput,
        Self::TextSelection,
        Self::Switch,
        Self::Slider,
        Self::Checkbox,
        Self::Select,
        Self::SegmentedControl,
        Self::Table,
        Self::ColumnDragResize,
        Self::VirtualList,
        Self::Tree,
        Self::Scrollbar,
        Self::FocusVisible,
    ];

    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ModalOverlay => "modal-overlay",
            Self::ContextMenu => "context-menu",
            Self::DropdownMenu => "dropdown-menu",
            Self::Tooltip => "tooltip",
            Self::Toast => "toast",
            Self::TextInput => "text-input",
            Self::SearchInput => "search-input",
            Self::TextSelection => "text-selection",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
            Self::SegmentedControl => "segmented-control",
            Self::Table => "table",
            Self::ColumnDragResize => "column-drag-resize",
            Self::VirtualList => "virtual-list",
            Self::Tree => "tree",
            Self::Scrollbar => "scrollbar",
            Self::FocusVisible => "focus-visible",
        }
    }

    /// The `taskmanager-ui` source path (relative to `crates/taskmanager-ui/
    /// src/`) of the component that owns this capability's reference
    /// semantics (GPUI-05). A root-level existence gate keeps the pairing
    /// real: renaming or removing the reference component breaks the gate
    /// instead of silently orphaning the parallel frontends' declarations.
    #[must_use]
    pub const fn reference_path(self) -> &'static str {
        match self {
            Self::ModalOverlay => "overlays/dialog.rs",
            Self::ContextMenu => "overlays/context_menu.rs",
            Self::DropdownMenu => "overlays/dropdown_menu.rs",
            Self::Tooltip => "primitives/tooltip.rs",
            Self::Toast => "overlays/toast.rs",
            Self::TextInput => "inputs/text_input.rs",
            Self::SearchInput => "inputs/search_input.rs",
            Self::TextSelection => "primitives/selectable_text.rs",
            Self::Switch => "inputs/switch.rs",
            Self::Slider => "inputs/slider.rs",
            Self::Checkbox => "inputs/checkbox.rs",
            Self::Select => "inputs/select.rs",
            Self::SegmentedControl => "primitives/segmented.rs",
            Self::Table => "data/table.rs",
            Self::ColumnDragResize => "data/table/resize.rs",
            Self::VirtualList => "data/virtual_list.rs",
            Self::Tree => "data/tree.rs",
            Self::Scrollbar => "primitives/scrollbar.rs",
            Self::FocusVisible => "focus.rs",
        }
    }
}

/// One frontend's support decision for a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilitySupport {
    /// The reference semantics themselves, owned by `taskmanager-ui`; only
    /// the GPUI shape may declare this (GPUI-05).
    Reference,
    /// The toolkit/platform supplies the mechanism and its semantics;
    /// `via` names the supplier (e.g. "iced scrollable", "terminal
    /// emulator selection").
    Native { via: &'static str },
    /// A frontend-local port of the reference semantics, with the intent to
    /// match them (behavior proof stays with the shape's tests).
    Ported,
    /// Deliberate divergence from the reference semantics, with the
    /// architecture driver stated.
    Divergent { reason: &'static str },
    /// Typed not-offered in this shape, with the driver stated.
    Unsupported { reason: &'static str },
}

impl CapabilitySupport {
    /// The explanatory text this decision must carry — `None` for
    /// `Reference`/`Ported`, the supplier or reason otherwise.
    #[must_use]
    pub const fn explanation(self) -> Option<&'static str> {
        match self {
            Self::Reference | Self::Ported => None,
            Self::Native { via } => Some(via),
            Self::Divergent { reason } | Self::Unsupported { reason } => Some(reason),
        }
    }
}

/// One declared capability-to-support pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityEntry {
    pub capability: ComponentCapability,
    pub support: CapabilitySupport,
}

/// A frontend's explicit declaration of its component/surface capability
/// surface: one entry per contract-known capability.
#[derive(Clone, Debug)]
pub struct FrontendCapabilityDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<CapabilityEntry>,
}

/// The coverage outcome for one capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    /// An explicit support decision.
    Declared(CapabilitySupport),
    /// Contract-known but absent from the declaration — a silent omission.
    Missing,
    /// Declared more than once by the same frontend.
    Duplicated,
    /// Declared but outside the known capability set.
    Unknown,
}

impl CapabilityStatus {
    /// Whether the capability carries an explicit decision — the only
    /// status a no-drift declaration may show.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Declared(_))
    }

    /// Whether this status is a drift finding a gate must reject.
    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// The coverage matrix for one declaration against the full capability set,
/// in canonical [`ComponentCapability::ALL`] order.
#[must_use]
pub fn capability_report(
    declaration: &FrontendCapabilityDeclaration,
) -> Vec<(ComponentCapability, CapabilityStatus)> {
    capability_report_over(declaration, ComponentCapability::ALL)
}

/// The drift findings alone.
#[must_use]
pub fn capability_drift(
    report: &[(ComponentCapability, CapabilityStatus)],
) -> Vec<(ComponentCapability, CapabilityStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// What a capability gate rejects, beyond the plain drift statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityFindingKind {
    /// Contract-known capability absent from the declaration.
    Missing,
    /// Capability declared more than once.
    Duplicated,
    /// Declared capability outside the known set.
    Unknown,
    /// A non-reference shape declared `Reference` (GPUI-05: only the GPUI
    /// shape owns reference semantics).
    ReferenceOutsideReferenceShape,
    /// The reference shape declared `Ported`/`Divergent`/`Unsupported` —
    /// the shape that owns the semantics cannot port, diverge from, or
    /// defer itself; grow (or shrink) the shared vocabulary instead.
    ReferenceShapeCannotDefer,
    /// A `Native`/`Divergent`/`Unsupported` decision without its required
    /// supplier/reason text.
    EmptyExplanation,
}

/// One gate finding: the frontend, the capability, and what is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityFinding {
    pub frontend: FrontendShape,
    pub capability: ComponentCapability,
    pub kind: CapabilityFindingKind,
}

/// The one-call gate payload: fold drift plus the shape-discipline rules
/// (reference role, reference-shape totality, non-empty explanations). An
/// empty result is the only passing state.
#[must_use]
pub fn capability_findings(declaration: &FrontendCapabilityDeclaration) -> Vec<CapabilityFinding> {
    let drift_kind = |status: CapabilityStatus| match status {
        CapabilityStatus::Missing => Some(CapabilityFindingKind::Missing),
        CapabilityStatus::Duplicated => Some(CapabilityFindingKind::Duplicated),
        CapabilityStatus::Unknown => Some(CapabilityFindingKind::Unknown),
        CapabilityStatus::Declared(_) => None,
    };
    let mut findings: Vec<CapabilityFinding> = capability_report(declaration)
        .into_iter()
        .filter_map(|(capability, status)| {
            drift_kind(status).map(|kind| CapabilityFinding {
                frontend: declaration.frontend,
                capability,
                kind,
            })
        })
        .collect();
    for (capability, status) in capability_report(declaration) {
        let CapabilityStatus::Declared(support) = status else {
            continue;
        };
        let kind = if support == CapabilitySupport::Reference
            && !declaration.frontend.is_capability_reference_shape()
        {
            Some(CapabilityFindingKind::ReferenceOutsideReferenceShape)
        } else if declaration.frontend.is_capability_reference_shape()
            && matches!(
                support,
                CapabilitySupport::Ported
                    | CapabilitySupport::Divergent { .. }
                    | CapabilitySupport::Unsupported { .. }
            )
        {
            Some(CapabilityFindingKind::ReferenceShapeCannotDefer)
        } else if support.explanation().is_some_and(str::is_empty) {
            Some(CapabilityFindingKind::EmptyExplanation)
        } else {
            None
        };
        if let Some(kind) = kind {
            findings.push(CapabilityFinding {
                frontend: declaration.frontend,
                capability,
                kind,
            });
        }
    }
    findings.sort_by_key(|finding| finding.capability);
    findings.dedup();
    findings
}

/// The fold against a restricted known set — the seam that keeps the
/// `Unknown` path real (a declared capability outside `known` is reported,
/// never silently dropped).
fn capability_report_over(
    declaration: &FrontendCapabilityDeclaration,
    known: &[ComponentCapability],
) -> Vec<(ComponentCapability, CapabilityStatus)> {
    let mut report: Vec<(ComponentCapability, CapabilityStatus)> = known
        .iter()
        .map(|capability| (*capability, CapabilityStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(capability, _)| *capability == entry.capability)
        {
            Some((_, status)) => {
                if status.is_explicit() || matches!(status, CapabilityStatus::Duplicated) {
                    *status = CapabilityStatus::Duplicated;
                } else {
                    *status = CapabilityStatus::Declared(entry.support);
                }
            }
            None => report.push((entry.capability, CapabilityStatus::Unknown)),
        }
    }
    report
}

#[cfg(test)]
#[path = "../tests/headless/ui_capabilities.rs"]
mod tests;
