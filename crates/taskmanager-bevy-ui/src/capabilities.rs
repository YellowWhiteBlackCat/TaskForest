//! Bevy's explicit CORE-08 component/surface capability declaration.
//!
//! The declaration describes the DELIVERED Bevy surface, not a promise that
//! every authored widget scene is mounted. Several widget families
//! (`dropdown_menu_scene`, `tooltip_scene`, `slider_scene`,
//! `scrollbar_scene`) are ports that await a host surface, so their cells
//! declare the shape's actual delivered mechanism instead of `Ported`:
//! `Unsupported` where nothing equivalent is offered, `Divergent` where a
//! different mounted mechanism carries the same user goal. Each cell is
//! pinned against the mounted page trees in
//! `tests/headless/capabilities.rs`, so mounting one of those scenes later
//! fails the test until the declaration is updated in the same change.
//! Unsupported and deliberate differences carry reasons so the four-frontend
//! registry stays fail-closed while Bevy grows.

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
        // No control-anchored popover is mounted anywhere: the authored
        // `dropdown_menu_scene` is a port awaiting a host surface (column
        // visibility / preset picker) and this shape ships no such surface.
        (
            DropdownMenu,
            Unsupported {
                reason: "no control-anchored popover is offered; the authored dropdown \
                         scene is not mounted and no column-visibility surface exists",
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
        // are not selectable, and the search editor's Ctrl+C/X/V edit only
        // the shell-owned editor buffer (no system-clipboard write).
        (
            TextSelection,
            Unsupported {
                reason: "read-out text is not selectable and no system-clipboard export is \
                         wired; the search editor's Ctrl+C/X/V edit only the shell-owned \
                         editor buffer",
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
        // Bounded settings are discrete radio choices; the authored slider
        // scene is not mounted on any page.
        (
            Slider,
            Divergent {
                reason: "bounded settings render as discrete radio choices; the authored slider scene is not mounted",
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
        // (wheel/trackpad + ScrollIntoView); the authored rail scene is not
        // mounted on any page.
        (
            Scrollbar,
            Divergent {
                reason: "page scrolling rides the official bevy_ui_widgets ScrollArea; the authored rail scene is not mounted",
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
