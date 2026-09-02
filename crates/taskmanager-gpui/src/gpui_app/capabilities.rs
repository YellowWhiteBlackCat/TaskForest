//! The GPUI shape's component/surface capability declaration (CORE-08).
//!
//! GPUI is the REFERENCE shape (GPUI-05): `taskmanager-ui` builds every
//! capability in the registry directly on gpui — gpui 0.2.2 ships none of
//! them — so this declaration marks each capability
//! [`CapabilitySupport::Reference`]. The reference shape may not defer a
//! capability (`Ported`/`Divergent`/`Unsupported` are gate findings): a new
//! capability enters the shared vocabulary only when the reference layer
//! grows it. Parallel frontends (Iced, TUI, Bevy) port these semantics; gaps
//! they find while porting are written back here, never forked silently.

use taskmanager_ui_contract::{
    CapabilityEntry, CapabilitySupport, ComponentCapability, FrontendCapabilityDeclaration,
    FrontendShape,
};

/// The GPUI capability declaration: every registry capability is owned by
/// the `taskmanager-ui` reference layer.
#[must_use]
pub fn capability_declaration() -> FrontendCapabilityDeclaration {
    FrontendCapabilityDeclaration {
        frontend: FrontendShape::Gpui,
        entries: ComponentCapability::ALL
            .iter()
            .map(|capability| CapabilityEntry {
                capability: *capability,
                support: CapabilitySupport::Reference,
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_capabilities_tests.rs"]
mod tests;
