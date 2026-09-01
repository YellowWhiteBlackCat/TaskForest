//! SMBIOS memory-inventory subsection card for the System page.
//!
//! Paints the pure projection from
//! [`super::sections::memory_inventory`] in the same card geometry as the
//! page's other section cards. Only accepted inventory is painted here;
//! authorization and failure states are collected in Settings' central
//! permission center. `Hidden` renders no card at all.

use gpui::{AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Styled, div};
use taskmanager_application::{SmbiosMemoryRequest, i18n, request_submission_failure};
use taskmanager_platform_contract::{CapabilityId, SubmissionErrorKind};

use super::sections::memory_inventory::{
    MemoryInventoryInputs, MemoryInventoryModel, memory_inventory_model,
};
use crate::gpui_app::root::RootView;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::primitives::card_surface::CardSurface;

impl RootView {
    /// The legacy page-local entry remains an internal request seam for
    /// callers/tests; the rendered affordance now lives in Settings' central
    /// permission center.
    pub(crate) fn authorize_memory_inventory(&mut self, cx: &mut Context<Self>) {
        let inputs = MemoryInventoryInputs {
            state: self.shell.smbios_memory_state(),
            capability: self
                .projection()
                .capability_status(&CapabilityId::TELEMETRY_MEMORY_SMBIOS),
        };
        let model = memory_inventory_model(&inputs, self.display_units());
        if !matches!(
            model,
            MemoryInventoryModel::AuthorizationRequired
                | MemoryInventoryModel::Unavailable("system.memory_inventory_denied")
        ) {
            return;
        }
        self.submit_memory_inventory_request();
        cx.notify();
    }

    pub(crate) fn submit_memory_inventory_request(&mut self) -> bool {
        let attempt = self.shell.begin_smbios_memory_request();
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_smbios_memory(
                        SmbiosMemoryRequest::Refresh,
                        crate::gpui_app::root::platform_submission_time_ms(),
                    )
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => self.shell.accept_smbios_memory_request(attempt, request_id),
            Err(kind) => {
                self.shell
                    .reject_smbios_memory_request(attempt, request_submission_failure(kind));
                false
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendering
// ─────────────────────────────────────────────────────────────────────────────

/// The memory-inventory card, rendered directly beneath the memory section
/// card. Only an accepted inventory produces an element; every other state is
/// handled by the central permission center.
pub(super) fn render_memory_inventory(
    theme: &Theme,
    inputs: &MemoryInventoryInputs<'_>,
    units: UnitPreferences,
) -> AnyElement {
    let model = memory_inventory_model(inputs, units);
    let MemoryInventoryModel::Inventory(rows) = model else {
        // Permission, loading and failure states are owned by the Settings
        // permission center. The System page only paints accepted inventory;
        // no unavailable slot is left as a dashed placeholder or inline
        // authorization button.
        return div().into_any_element();
    };
    let mut content = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(i18n::t("system.memory_inventory")),
        )
        .debug_selector(|| "tm-memory-inventory-card".to_string());
    for (label, value) in rows {
        let value_id = format!("memory-inventory-value:{label}");
        content = content.child(
            KeyValueRow::new(label, value, theme.palette())
                .selectable_value(gpui::ElementId::Name(value_id.into()))
                .render(),
        );
    }
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_12)
        .radius(tokens::control_radius(theme))
        .bordered(false)
        .child(content)
        .render()
        .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_system_view_memory_inventory_tests.rs"]
mod tests;
