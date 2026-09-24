//! Bevy's explicit CORE-08 component/surface capability declaration.
//!
//! The declaration describes the DELIVERED Bevy surface, not a promise that
//! every reference component exists. A cell declares the shape's actual
//! delivered mechanism: `Unsupported` where nothing equivalent is offered,
//! `Divergent` where a different mounted mechanism carries the same user goal.
//! Each delivered-surface cell is pinned against the mounted page trees in
//! `tests/headless/capabilities.rs`, so mounting a new mechanism later fails
//! the test until the declaration is updated in the same change. Unsupported
//! and deliberate differences carry reasons so the four-frontend registry
//! stays fail-closed while Bevy grows.

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
        // No control-anchored popover is mounted anywhere: this shape ships
        // no column-visibility or preset-picker surface to anchor one to.
        (
            DropdownMenu,
            Unsupported {
                reason: "no control-anchored popover is offered; no column-visibility \
                         or preset-picker surface exists",
            },
        ),
        // No pointer-hover explanation surface is mounted; hint copy is
        // rendered as inline captions (e.g. the insights scroll hint).
        (
            Tooltip,
            Unsupported {
                reason: "no pointer-hover explanation surface is mounted; hint text \
                         renders as inline captions",
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
        // The shape offers no selection surface at all: read-out text nodes
        // are not selectable, no system-clipboard read/write is wired, and the
        // search editor binds no clipboard chords (Ctrl+C/X/V are not
        // intercepted).
        (
            TextSelection,
            Unsupported {
                reason: "read-out text is not selectable and no system-clipboard export is \
                         wired; the search editor offers character editing only and binds no \
                         Ctrl+C/X/V clipboard chords",
            },
        ),
        // Boolean rows use the official `bevy_ui_widgets::Checkbox`; no
        // switch-specific control is authored.
        (
            Switch,
            Divergent {
                reason: "boolean rows use the official bevy_ui_widgets Checkbox; no switch-specific control is authored",
            },
        ),
        // Bounded settings are discrete radio choices.
        (
            Slider,
            Divergent {
                reason: "bounded settings render as discrete radio choices",
            },
        ),
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
        // Scrolling rides the official `bevy_ui_widgets::ScrollArea`
        // (wheel/trackpad + ScrollIntoView).
        (
            Scrollbar,
            Divergent {
                reason: "page scrolling rides the official bevy_ui_widgets ScrollArea",
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
