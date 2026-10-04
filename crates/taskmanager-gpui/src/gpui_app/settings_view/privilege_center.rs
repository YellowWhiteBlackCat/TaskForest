//! Central authorization surface for optional hardware facts.
//!
//! Performance and System pages only render accepted observations. This
//! module is the single GPUI entry point for the four user-initiated helper
//! lanes that can make those observations available. Each row still submits
//! one typed capability request; the center groups the controls without
//! turning them into a blanket privileged process.

use gpui::{Div, Entity, InteractiveElement, ParentElement, Styled, div};
use taskmanager_application::i18n;
use taskmanager_shell::presentation::privilege_center::{
    PrivilegeAction, PrivilegeCenterInputs, PrivilegeRow, PrivilegeRowState,
};
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui::theme_binding::absolute;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;

pub(crate) fn render_privilege_center(
    theme: &Theme,
    inputs: &PrivilegeCenterInputs<'_>,
    entity: Entity<RootView>,
) -> Option<Div> {
    let rows = inputs.rows();
    if rows.is_empty() {
        return None;
    }

    let mut panel = div()
        .debug_selector(|| "tm-settings-privilege-center".to_string())
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_8))
        .px(definite_length(tokens::SPACE_10))
        .py(definite_length(tokens::SPACE_8))
        .rounded(absolute(tokens::control_radius(theme)))
        .border_1()
        .border_color(hsla(theme.border))
        .bg(fill(theme.sidebar_card_bg))
        .child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t("settings.privileges_hint")),
        );
    for row in rows {
        panel = panel.child(render_row(theme, row, entity.clone()));
    }
    Some(panel)
}

fn render_row(theme: &Theme, row: PrivilegeRow, entity: Entity<RootView>) -> Div {
    let state = row.state;
    let mut line = div()
        .debug_selector(move || format!("tm-settings-privilege-row:{}", row.id))
        .flex()
        .flex_row()
        .items_center()
        .gap(definite_length(tokens::SPACE_8))
        .w_full()
        .min_w(gpui::px(0.0))
        .child(
            div()
                .flex_1()
                .min_w(gpui::px(0.0))
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg))
                .child(i18n::t(row.label_key)),
        )
        .child(
            div()
                .flex_none()
                .text_size(font_size(tokens::FONT_11))
                .text_color(hsla(
                    if matches!(state, PrivilegeRowState::Denied | PrivilegeRowState::Failed) {
                        theme.warning
                    } else {
                        theme.fg_dim
                    },
                ))
                .debug_selector({
                    let id = row.id;
                    move || format!("tm-settings-privilege-state:{id}")
                })
                .child(i18n::t(state.label_key())),
        );
    if let Some(action) = row.action {
        line = line.child(authorization_button(theme, row.id, entity, action));
    }
    line
}

fn authorization_button(
    theme: &Theme,
    id: &'static str,
    entity: Entity<RootView>,
    action: PrivilegeAction,
) -> Div {
    div()
        .debug_selector(move || format!("tm-settings-privilege-action:{id}"))
        .flex_none()
        .child(elements::tool_btn(
            theme,
            id,
            i18n::t("settings.privileges_authorize"),
            true,
            false,
            move |_window, cx| {
                entity.update(cx, |view, cx| match &action {
                    PrivilegeAction::GpuEngines(id) => {
                        if let Some(index) = (0..view.system_snapshot().gpu.len())
                            .find(|index| view.gpu_engine_rows_device_id(*index) == *id)
                        {
                            view.enable_gpu_engines(index, cx);
                        }
                    }
                    PrivilegeAction::SmbiosMemory => view.authorize_memory_inventory(cx),
                    PrivilegeAction::RaplPower => view.authorize_package_power(cx),
                    PrivilegeAction::MsrReadouts => view.authorize_msr_readouts(cx),
                });
            },
            |_hovered, _window, _cx| {},
        ))
}
