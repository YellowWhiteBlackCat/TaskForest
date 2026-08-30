//! SMBIOS memory-inventory subsection card for the System page.
//!
//! Paints the pure projection from
//! [`super::sections::memory_inventory`] in the same card geometry as the
//! page's other section cards, plus the one interactive affordance: the
//! "Authorize memory inventory" button, the ONLY trigger for the lane's
//! OS-native prompt (escalation discipline: never auto-polled). `Hidden`
//! renders no card at all.

use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window, div,
};
use taskmanager_application::{SmbiosMemoryRequest, i18n, request_submission_failure};
use taskmanager_platform_contract::{CapabilityId, SubmissionErrorKind};

use super::sections::memory_inventory::{
    MemoryInventoryInputs, MemoryInventoryModel, memory_inventory_model,
};
use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::platform_submission_time_ms;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::primitives::card_surface::CardSurface;

// ─────────────────────────────────────────────────────────────────────────────
// RootView glue — the ONLY submission entry, gated on the projection.
// ─────────────────────────────────────────────────────────────────────────────

impl RootView {
    /// The user clicked the authorize affordance. This is the single explicit
    /// trigger for the SMBIOS lane's OS-native prompt; never auto-invoked.
    pub(crate) fn authorize_memory_inventory(&mut self, cx: &mut Context<Self>) {
        let inputs = MemoryInventoryInputs {
            state: self.shell.smbios_memory_state(),
            capability: self
                .projection()
                .capability_status(&CapabilityId::TELEMETRY_MEMORY_SMBIOS),
        };
        if !matches!(
            memory_inventory_model(&inputs, self.display_units()),
            MemoryInventoryModel::AuthorizationRequired
        ) {
            return;
        }
        self.submit_memory_inventory_request();
        cx.notify();
    }

    /// Submit one memory-inventory read. Beginning the attempt before touching
    /// the platform makes replacement and synchronous rejection obey the same
    /// identity rules as asynchronous terminals
    /// (`submit_gpu_engine_rows_refresh`).
    pub(crate) fn submit_memory_inventory_request(&mut self) -> bool {
        let attempt = self.shell.begin_smbios_memory_request();
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_smbios_memory(
                        SmbiosMemoryRequest::Refresh,
                        platform_submission_time_ms(),
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
/// card. `Hidden` produces no element.
pub(super) fn render_memory_inventory(
    theme: &Theme,
    inputs: &MemoryInventoryInputs<'_>,
    units: UnitPreferences,
    entity: Entity<RootView>,
) -> AnyElement {
    let model = memory_inventory_model(inputs, units);
    if matches!(model, MemoryInventoryModel::Hidden) {
        return div().into_any_element();
    }
    let mut content = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(theme.fg)
                .child(i18n::t("system.memory_inventory")),
        )
        .debug_selector(|| "tm-memory-inventory-card".to_string());
    match &model {
        MemoryInventoryModel::Inventory(rows) => {
            for (label, value) in rows {
                content = content.child(
                    KeyValueRow::new(label, value, theme.palette())
                        .selectable_value(gpui::ElementId::Name(
                            format!("memory-inventory-value:{label}").into(),
                        ))
                        .render(),
                );
            }
        }
        MemoryInventoryModel::Reading => {
            content = content.child(dim_text(theme, i18n::t("system.memory_inventory_reading")));
        }
        MemoryInventoryModel::AuthorizationRequired => {
            content = content
                .child(dim_text(theme, i18n::t("system.memory_requires_auth")))
                .child(authorize_button(theme, entity));
        }
        MemoryInventoryModel::Unavailable(key) => {
            content = content.child(dim_text(theme, i18n::t(key)));
        }
        MemoryInventoryModel::Hidden => {}
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

fn dim_text(theme: &Theme, text: &str) -> Div {
    div()
        .text_size(tokens::FONT_12)
        .text_color(theme.fg_dim)
        .child(text.to_owned())
}

/// Keyboard-focusable, clickable text button in the shared affordance style
/// (accent label, focus ring, pointer cursor — the GPU engines panel's
/// `action_button` idiom). The click publishes through the RootView entity;
/// `render_system` is a stateless free function.
fn authorize_button(theme: &Theme, entity: Entity<RootView>) -> AnyElement {
    let button = div()
        .id("memory-inventory-authorize")
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(move |_ev: &ClickEvent, _win: &mut Window, cx: &mut App| {
            entity.update(cx, |view, cx| {
                view.authorize_memory_inventory(cx);
            });
        })
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.accent)
                .child(i18n::t("system.memory_authorize").to_owned()),
        );
    button
        .debug_selector(|| "tm-memory-inventory-authorize".to_string())
        .into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_system_view_memory_inventory_tests.rs"]
mod tests;
