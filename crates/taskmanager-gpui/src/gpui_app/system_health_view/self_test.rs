//! SMART self-test confirmation surface: the pill actions and the typed
//! request payload, split from the parent health page to keep each file under
//! the line ratchet.

use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, ParentElement, Stateful, Styled, div,
};
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskmanager_core::core::{
    DeviceId, DeviceStatus, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use crate::gpui_app::elements;
use crate::gpui_app::system_health_view::{
    SmartSelfTestConfirmationRequest, SystemHealthCallbacks, SystemHealthText, badge, metric,
    state_color,
};

fn disabled_action(theme: &Theme, label: String, id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px(tokens::SPACE_10)
        .py(tokens::SPACE_6)
        .rounded(tokens::control_radius(theme))
        .border_1()
        .border_color(theme.border)
        .text_size(tokens::FONT_12)
        .text_color(theme.fg_dim)
        .child(label)
}

pub(crate) fn self_test_action(
    theme: &Theme,
    label: String,
    id: &'static str,
    kind: SmartSelfTestKind,
    disk: Option<&DiskMetrics>,
    enabled: bool,
    callbacks: &SystemHealthCallbacks,
) -> AnyElement {
    let Some(disk) = disk.filter(|_| enabled) else {
        return disabled_action(theme, label, id).into_any_element();
    };
    let request = SmartSelfTestConfirmationRequest {
        device_id: DeviceId::new(disk.device_id.clone()),
        device_generation: disk.device_generation,
        disk_name: disk.name.clone(),
        disk_label: if disk.model.is_empty() {
            disk.name.clone()
        } else {
            disk.model.clone()
        },
        kind,
    };
    let callback = callbacks.request_confirmation.clone();
    elements::pill(
        theme,
        id,
        &label,
        false,
        false,
        move |window, cx| callback(request.clone(), window, cx),
        |_, _, _| {},
    )
    .into_any_element()
}

pub(crate) fn self_test_card(
    theme: &Theme,
    disk: Option<&DiskMetrics>,
    report: Option<&SmartSelfTestReport>,
    copy: &dyn Fn(SystemHealthText) -> String,
    callbacks: &SystemHealthCallbacks,
) -> Div {
    let phase = report
        .map(|report| copy(SystemHealthText::SmartPhase(report.phase)))
        .unwrap_or_else(|| copy(SystemHealthText::Unavailable));
    let state = report.map_or(DeviceStatus::Unsupported, |report| report.state.status);
    let can_request = disk
        .is_some_and(|disk| disk.smart_availability == SmartAvailability::Available)
        && report.is_none_or(|report| report.phase != SmartSelfTestPhase::Running);
    let mut details = div()
        .mt(tokens::SPACE_7)
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(tokens::SPACE_8);
    if let Some(report) = report {
        details = details
            .child(metric(theme, copy(SystemHealthText::Status), phase))
            .child(metric(
                theme,
                copy(SystemHealthText::Progress),
                report
                    .progress_pct
                    .map(|progress| format!("{progress:.0}%"))
                    .unwrap_or_else(|| copy(SystemHealthText::Unavailable)),
            ))
            .child(metric(
                theme,
                copy(SystemHealthText::LifetimeHours),
                report
                    .lifetime_hours
                    .map(|hours| hours.to_string())
                    .unwrap_or_else(|| copy(SystemHealthText::Unavailable)),
            ))
            .child(metric(
                theme,
                copy(SystemHealthText::FirstErrorLba),
                report
                    .first_error_lba
                    .map(|lba| lba.to_string())
                    .unwrap_or_else(|| copy(SystemHealthText::Unavailable)),
            ));
        if let Some(kind) = report.kind {
            details = details.child(metric(
                theme,
                copy(SystemHealthText::SmartSelfTest),
                copy(SystemHealthText::SmartKind(kind)),
            ));
        }
        if let Some(failure) = report.failure {
            details = details.child(metric(
                theme,
                copy(SystemHealthText::Errors),
                copy(SystemHealthText::SmartFailure(failure)),
            ));
        }
    } else {
        details = details.child(metric(theme, copy(SystemHealthText::Status), phase));
    }
    div()
        .mt(tokens::SPACE_9)
        .p(tokens::SPACE_9)
        .rounded(tokens::control_radius(theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar_card_bg)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(tokens::SPACE_6)
                .child(
                    div()
                        .font_weight(tokens::FONT_WEIGHT_HEADER.into())
                        .child(copy(SystemHealthText::SmartSelfTest)),
                )
                .child(badge(
                    theme,
                    copy(SystemHealthText::DeviceStatus(state)),
                    state_color(theme, state),
                )),
        )
        .child(details)
        .child(
            div()
                .mt(tokens::SPACE_8)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(copy(SystemHealthText::ConfirmationRequired)),
        )
        .child(
            div()
                .mt(tokens::SPACE_6)
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_6)
                .child(self_test_action(
                    theme,
                    copy(SystemHealthText::ShortTest),
                    "health-short-test",
                    SmartSelfTestKind::Short,
                    disk,
                    can_request,
                    callbacks,
                ))
                .child(self_test_action(
                    theme,
                    copy(SystemHealthText::ExtendedTest),
                    "health-extended-test",
                    SmartSelfTestKind::Extended,
                    disk,
                    can_request,
                    callbacks,
                )),
        )
}
