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
    use CapabilitySupport::{Divergent, Native, Ported};
    use ComponentCapability::{
        Checkbox, ColumnDragResize, ContextMenu, DropdownMenu, FocusVisible, ModalOverlay,
        Scrollbar, SearchInput, SegmentedControl, Select, Slider, Switch, Table, TextInput,
        TextSelection, Toast, Tooltip, Tree, VirtualList,
    };

    let supports: [(ComponentCapability, CapabilitySupport); 19] = [
        (ModalOverlay, Ported),
        (ContextMenu, Ported),
        // The column chooser is Bevy's official anchored Popover: toggles stay
        // open, outside press/Escape/focus loss close it, and focus returns to
        // the trigger.
        (
            DropdownMenu,
            Native {
                via: "bevy_ui_widgets Popover",
            },
        ),
        // The tooltip state machine opens on pointer hover or keyboard focus
        // and dismisses on pointer exit, pointer press, Escape, or focus loss
        // (tests/headless/tooltip.rs drives the mounted overlay).
        (Tooltip, Ported),
        (
            Toast,
            Divergent {
                reason: "transient feedback renders through the shared feedback line",
            },
        ),
        // Applications/Services search fields are native text inputs; the
        // shell stays the single query authority through the editor bridge.
        (
            TextInput,
            Native {
                via: "bevy_ui_widgets TextInput",
            },
        ),
        (SearchInput, Ported),
        // System fact readouts are read-only native text inputs: pointer
        // range/word/triple-click selection, Ctrl/Cmd+A, the clipboard chords,
        // and the highlight are the official widget's behavior; the process
        // table adds its own double-click cell selection and row copy.
        (
            TextSelection,
            Native {
                via: "bevy_ui_widgets TextInput (read-only EditableText)",
            },
        ),
        // The settings rows own a real sliding switch: the official
        // `bevy_ui_widgets::Checkbox` behavior primitive drives the toggle
        // and the platform tree publishes the `Switch` role.
        (Switch, Ported),
        // The alert rule editor mounts the official `bevy_ui_widgets` Slider
        // for its threshold values; the bounded settings rows still render as
        // discrete radio choices.
        (
            Slider,
            Divergent {
                reason: "the alert rule editor mounts the official bevy_ui_widgets Slider for threshold values; bounded settings rows still render as discrete radio choices",
            },
        ),
        // These are semantic-contract capabilities. Bevy composes the
        // checkbox contract with its official Checkbox behavior primitive.
        (Checkbox, Ported),
        // Select is composed from the official RadioGroup/RadioButton pair;
        // Bevy does not mount a generic Select or enable FeathersSelect.
        (Select, Ported),
        (SegmentedControl, Ported),
        (Table, Ported),
        // The process table mounts a header-edge drag gutter with the shared
        // anchored measurement, plus the column chooser's `-`/`=` keyboard
        // stepper (both write the one `ColumnWidthOverrides` authority).
        (ColumnDragResize, Ported),
        (VirtualList, Ported),
        (Tree, Ported),
        // Scrolling rides the official `bevy_ui_widgets::ScrollArea`
        // (wheel/trackpad + ScrollIntoView); the visible rail is the
        // official `bevy_ui_widgets::Scrollbar` wearing the product's
        // translucent track and capsule thumb.
        (
            Scrollbar,
            Divergent {
                reason: "the scroll areas mount the official bevy_ui_widgets Scrollbar: a translucent floating rail with a capsule thumb, press-and-drag included",
            },
        ),
        // Modality-aware high-contrast keyboard focus indication (CORE-08 / FocusVisible).
        (FocusVisible, Ported),
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
