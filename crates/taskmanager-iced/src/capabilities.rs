//! The Iced shape's component/surface capability declaration (CORE-08).
//!
//! Iced is a PORTING shape: it consumes the same reference semantics
//! `taskmanager-ui` owns (GPUI-05) and rebuilds them on iced, mostly as
//! renderer-local widgets routed through the crate focus shell. Every
//! deliberate difference carries its architecture driver here — this
//! registry REPLACES the hand-maintained divergence prose: a difference
//! without a registered reason is drift, not a choice.

use taskmanager_ui_contract::{
    CapabilityEntry, CapabilitySupport, ComponentCapability, FrontendCapabilityDeclaration,
    FrontendShape,
};

/// The Iced capability declaration: ports of the reference semantics, the
/// toolkit-native pieces the frontend trusts, and the registered
/// divergences with their drivers.
#[must_use]
pub fn capability_declaration() -> FrontendCapabilityDeclaration {
    use CapabilitySupport::{Divergent, Native, Ported};
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };
    let supports: [(ComponentCapability, CapabilitySupport); 19] = [
        // The modal surfaces (surface.rs + focus shell) port the reference
        // dialog semantics: scrim, focus containment, branch-matched close.
        (ModalOverlay, Ported),
        // The single Iced-local context-menu slot (app/surface.rs).
        (ContextMenu, Ported),
        // The column menu anchored to the table header.
        (DropdownMenu, Ported),
        (Tooltip, Ported),
        (
            Toast,
            Divergent {
                reason: "transient feedback renders through the shared footer activity line, \
                         not floating toasts",
            },
        ),
        // iced's native text_input widget; keyboard paths ride the focus
        // shell so activation/traversal match the reference.
        (TextInput, Ported),
        (SearchInput, Ported),
        (
            TextSelection,
            // `components::SelectableText`: pointer drag selection (working
            // past the widget bounds), double-click word, triple-click all,
            // one active selection per window, drag finish publishes the
            // primary clipboard, Ctrl/Cmd-C copies through the standard
            // clipboard. Registered difference: keyboard select-all (Ctrl-A)
            // is NOT ported — Ctrl-A belongs to the shared command vocabulary
            // (row-summary copy) in this frontend. Multi-line blocks (service
            // log lines) select ROW-WISE by design: the paragraph layer
            // exposes no line metrics, so a block-wide highlight could only
            // be approximate; whole-block export stays on the copy/export
            // actions.
            Ported,
        ),
        (Switch, Ported),
        (Slider, Ported),
        (Checkbox, Ported),
        (Select, Ported),
        (SegmentedControl, Ported),
        // The shared typed column contract (PROCESS_COLUMNS) over the
        // iced-local virtual list and sticky-header shell.
        (Table, Ported),
        // Iced's header-edge drag owns an equivalent live resize session;
        // its width domain is renderer-local geometry, not this capability.
        (ColumnDragResize, Ported),
        (VirtualList, Ported),
        (Tree, Ported),
        // Scrolling trusts iced's own scrollable: viewport tracking and
        // wheel semantics belong to the toolkit here.
        (
            Scrollbar,
            Native {
                via: "iced scrollable",
            },
        ),
        // The renderer-local input-modality tracker synthesizes the same
        // strict keyboard-visible policy the GPUI root tracker drives
        // (see the focus-ring behavior tests in tests/gui/theme_tests.rs).
        (FocusVisible, Ported),
    ];
    FrontendCapabilityDeclaration {
        frontend: FrontendShape::Iced,
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
