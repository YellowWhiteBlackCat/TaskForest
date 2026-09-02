//! Bevy's explicit CORE-08 component/surface capability declaration.
//!
//! The declaration describes the current Bevy scene adapters, not a promise
//! that every GPUI reference component already exists. Unsupported and
//! deliberate differences carry reasons so the four-frontend registry stays
//! fail-closed while Bevy grows.

use taskmanager_ui_contract::{
    CapabilityEntry, CapabilitySupport, ComponentCapability, FrontendCapabilityDeclaration,
    FrontendShape,
};

/// Declare the Bevy shape's complete component capability surface.
#[must_use]
pub fn capability_declaration() -> FrontendCapabilityDeclaration {
    use CapabilitySupport::{Divergent, Ported, Unsupported};
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };

    let supports: [(ComponentCapability, CapabilitySupport); 19] = [
        (ModalOverlay, Ported),
        (ContextMenu, Ported),
        (
            DropdownMenu,
            Unsupported {
                reason: "the Bevy shape has no anchored dropdown menu surface",
            },
        ),
        (
            Tooltip,
            Unsupported {
                reason: "the Bevy shape has no hover tooltip surface",
            },
        ),
        (
            Toast,
            Divergent {
                reason: "transient feedback renders through the shared feedback line",
            },
        ),
        (
            TextInput,
            Divergent {
                reason: "search editing is shell-owned character input rendered as a readout; no native text widget is wired",
            },
        ),
        (SearchInput, Ported),
        (
            TextSelection,
            Unsupported {
                reason: "the Bevy shape has no read-only text selection or clipboard surface",
            },
        ),
        (Switch, Ported),
        (
            Slider,
            Unsupported {
                reason: "the current Bevy settings surface uses bounded choices, not a slider",
            },
        ),
        (Checkbox, Ported),
        (Select, Ported),
        (SegmentedControl, Ported),
        (Table, Ported),
        (
            ColumnDragResize,
            Unsupported {
                reason: "pointer-driven column resizing is not wired in the Bevy table",
            },
        ),
        (VirtualList, Ported),
        (Tree, Ported),
        (
            Scrollbar,
            Unsupported {
                reason: "Bevy details and long pages use wheel-only ScrollArea surfaces; no visible scrollbar rail is wired",
            },
        ),
        (
            FocusVisible,
            Divergent {
                reason: "Bevy uses its current control visuals; a dedicated modality-aware focus ring is not wired",
            },
        ),
    ];

    FrontendCapabilityDeclaration {
        frontend: FrontendShape::Bevy,
        entries: supports
            .into_iter()
            .map(|(capability, support)| CapabilityEntry {
                capability,
                support,
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "../tests/headless/capabilities.rs"]
mod tests;
