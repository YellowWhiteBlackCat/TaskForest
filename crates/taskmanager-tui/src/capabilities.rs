//! The TUI shape's component/surface capability declaration (CORE-08).
//!
//! The terminal is a PORTING shape with real, terminal-driven limits: it
//! ports the reference semantics `taskmanager-ui` owns (GPUI-05) wherever
//! a terminal surface can carry them, and every capability it cannot offer
//! or must reshape carries its driver here — the registry replaces prose
//! parity claims with explained decisions.

use taskmanager_ui_contract::{
    CapabilityEntry, CapabilitySupport, ComponentCapability, FrontendCapabilityDeclaration,
    FrontendShape,
};

/// The TUI capability declaration: ports, terminal-native pieces, and the
/// registered divergences/absences with their drivers.
#[must_use]
pub fn capability_declaration() -> FrontendCapabilityDeclaration {
    use CapabilitySupport::{Divergent, Native, Ported, Unsupported};
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };
    let supports: [(ComponentCapability, CapabilitySupport); 19] = [
        (
            ModalOverlay,
            Divergent {
                reason: "keyboard focus is trapped and Escape cancels; terminal popups have no outside-click scrim dismissal",
            },
        ),
        // The keyboard action menus on targeted rows (process/service/
        // session/startup/batch) are this shape's context-menu surface.
        (ContextMenu, Ported),
        // The column-visibility menu over the Applications table.
        (DropdownMenu, Ported),
        (
            Tooltip,
            Unsupported {
                reason: "the terminal has no hover surface; key hints ride the footer/status \
                         line",
            },
        ),
        (
            Toast,
            Divergent {
                reason: "transient feedback renders through the shared footer activity line",
            },
        ),
        (
            TextInput,
            Divergent {
                reason: "search accepts end-appends and tail Backspace; there is no frontend-owned Left/Right/Home/End cursor, selection, mid-string insertion, or Delete",
            },
        ),
        (
            SearchInput,
            Divergent {
                reason: "typing filters the live process projection and Escape clears it; Left/Right/Home/End, selection, mid-string insertion, and Delete lack a frontend-owned cursor",
            },
        ),
        // Selection belongs to the terminal emulator, not the app.
        (
            TextSelection,
            Native {
                via: "terminal emulator selection",
            },
        ),
        (
            Switch,
            Divergent {
                reason: "settings switches toggle with Space or Left/Right; pointer activation and disabled switch states are not represented",
            },
        ),
        (
            Slider,
            Divergent {
                reason: "settings values use bounded discrete keyboard steps; there is no continuous pointer drag or interpolation",
            },
        ),
        // Column-visibility toggles carry the checkbox semantic.
        (Checkbox, Ported),
        // Skin/mode/font selections over enumerated token lists.
        (Select, Ported),
        (
            SegmentedControl,
            Divergent {
                reason: "no generic segmented group is rendered; page/resource selection uses navigation and TUI-003 bindings without a segment traversal contract",
            },
        ),
        (Table, Ported),
        (
            ColumnDragResize,
            Unsupported {
                reason: "the terminal has no pointer-driven column-edge drag surface",
            },
        ),
        // Ratatui clips only the bounded row window handed to it.
        (VirtualList, Ported),
        // The Applications expand/collapse tree.
        (Tree, Ported),
        (
            Scrollbar,
            Divergent {
                reason: "key-driven bounded row window; the terminal shape has no pointer \
                         scroll rail",
            },
        ),
        (
            FocusVisible,
            Divergent {
                reason: "the cursor row is the single focus indicator; there is no \
                         pointer/keyboard modality split",
            },
        ),
    ];
    FrontendCapabilityDeclaration {
        frontend: FrontendShape::Tui,
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
#[path = "../tests/gui/capabilities_tests.rs"]
mod tests;
