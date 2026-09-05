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
    use CapabilitySupport::{Divergent, Ported};
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };

    let supports: [(ComponentCapability, CapabilitySupport); 19] = [
        (ModalOverlay, Ported),
        (ContextMenu, Ported),
        (DropdownMenu, Ported),
        (Tooltip, Ported),
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
            Divergent {
                reason: "the Bevy shape provides row and summary copying without inline text drag selection",
            },
        ),
        (Switch, Ported),
        (Slider, Ported),
        (Checkbox, Ported),
        (Select, Ported),
        (SegmentedControl, Ported),
        (Table, Ported),
        (
            ColumnDragResize,
            Divergent {
                reason: "the Bevy table uses responsive flex slot distribution rather than pointer-drag column resizing",
            },
        ),
        (VirtualList, Ported),
        (Tree, Ported),
        (Scrollbar, Ported),
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
