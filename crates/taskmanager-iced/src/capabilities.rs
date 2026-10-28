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
        // The column menu is anchored to its table-header trigger. The shared
        // overlay policy forwards the outside press to the background target,
        // matching the GPUI reference.
        (DropdownMenu, Ported),
        // Behavior anchors: ui::perf_rail::tests::tooltip_behavior_tests::
        // mounted_rail_tooltip_opens_on_hover_and_dismisses_on_click_or_escape
        // and focused_rail_tooltip_opens_on_focus_and_escape_dismisses drive
        // the focused rail readout through Iced widget events.
        (
            Tooltip,
            Divergent {
                reason: "the focused Performance rail readout opens on keyboard focus and \
                         dismisses on pointer click/Escape; footer and row hint badges remain \
                         hover-only because their triggers are not focusable",
            },
        ),
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
            Divergent {
                // `components::SelectableText`: pointer drag selection
                // (working past widget bounds), double-click word,
                // triple-click all, one active selection per window,
                // drag-finish clipboard, Ctrl/Cmd-C, multi-line spans, and
                // Ctrl/Cmd-A on the readout that owns the window selection
                // are ported. Readouts are not keyboard-focusable, so an
                // unowned readout leaves Ctrl-A on the shared row-summary
                // command instead of selecting text without a prior pointer
                // ownership gesture.
                reason: "Iced ports pointer/word/block selection, multi-line spans, and Ctrl/Cmd-A once a readout owns the window selection; an unowned readout keeps Ctrl-A on the shared row-summary command because readouts are not keyboard-focusable",
            },
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
        // Scrolling rides iced's own scrollable with the product's capsule thumb
        // and translucent floating rail (styled_scrollable + theme::scrollbar_style).
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
