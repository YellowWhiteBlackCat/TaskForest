//! Default shared alert policy and the actionable in-app alert banner.

use super::{RootView, TopPage};
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::sidebar::SelectedDevice;
use taskmanager_application::i18n;
use taskmanager_core::core::{Alert, AlertMetric, AlertSeverity, SystemSnapshot};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => i18n::t("common.cpu"),
        AlertMetric::MemoryUsagePercent => i18n::t("common.memory"),
        AlertMetric::DiskTemperatureC => i18n::t("alert.disk_temperature"),
        AlertMetric::SmartPercentUsed => i18n::t("alert.smart_wear"),
        AlertMetric::SmartCriticalWarning => i18n::t("alert.smart_critical"),
    }
}

fn severity_label(severity: AlertSeverity) -> &'static str {
    match severity {
        AlertSeverity::Info => i18n::t("alert.info"),
        AlertSeverity::Warning => i18n::t("alert.warning"),
        AlertSeverity::Critical => i18n::t("alert.critical"),
    }
}

fn alert_target_device(alert: &Alert, snapshot: &SystemSnapshot) -> Option<SelectedDevice> {
    match alert.metric {
        AlertMetric::CpuUsagePercent => Some(SelectedDevice::Cpu),
        AlertMetric::MemoryUsagePercent => Some(SelectedDevice::Memory),
        AlertMetric::DiskTemperatureC
        | AlertMetric::SmartPercentUsed
        | AlertMetric::SmartCriticalWarning => snapshot
            .disks
            .iter()
            .position(|disk| disk.name == alert.target || disk.device_id == alert.target)
            .map(SelectedDevice::Disk),
    }
}

fn select_alert_target(view: &mut RootView, alert: &Alert, snapshot: &SystemSnapshot) {
    let Some(device) = alert_target_device(alert, snapshot) else {
        return;
    };
    view.page = TopPage::Performance;
    let device = match alert.metric {
        AlertMetric::CpuUsagePercent => SelectedDevice::Cpu,
        AlertMetric::MemoryUsagePercent => SelectedDevice::Memory,
        AlertMetric::DiskTemperatureC
        | AlertMetric::SmartPercentUsed
        | AlertMetric::SmartCriticalWarning => device,
    };
    view.select_device(device);
}

pub fn render_banner(
    theme: &Theme,
    alert: Alert,
    count: usize,
    snapshot: &SystemSnapshot,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let color = match alert.severity {
        AlertSeverity::Info => theme.accent,
        AlertSeverity::Warning => theme.gpu,
        AlertSeverity::Critical => theme.danger,
    };
    let value = match alert.metric {
        AlertMetric::DiskTemperatureC => format!("{:.0} °C", alert.value),
        AlertMetric::SmartCriticalWarning => i18n::t("alert.triggered").to_string(),
        _ => format!("{:.0}%", alert.value),
    };
    let suffix = if count > 1 {
        format!(" · +{}", count - 1)
    } else {
        String::new()
    };
    let text = format!(
        "{} · {} — {} {}{}",
        severity_label(alert.severity),
        metric_label(alert.metric),
        alert.target,
        value,
        suffix
    );
    let entity = cx.entity();
    let snapshot = snapshot.clone();
    div()
        .id("active-alert-banner")
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .w_full()
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .flex()
        .items_center()
        .justify_between()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_card_bg))
        .border_b_1()
        .border_color(taskmanager_ui::theme_binding::hsla(color))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
        .cursor_pointer()
        .on_click(move |_event, _window, cx| {
            entity.update(cx, |view, cx| {
                select_alert_target(view, &alert, &snapshot);
                cx.notify();
            });
        })
        .child(div().min_w(px(0.0)).child(text))
        .child(
            div()
                .flex_none()
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_SEMIBOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(color))
                .child(i18n::t("alert.view")),
        )
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_alert_ui_tests.rs"]
mod tests;
