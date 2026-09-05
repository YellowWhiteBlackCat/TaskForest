//! Formatting and layout helpers for Process Insights in Iced.

use iced::widget::{column, row, text};
use iced::{Element, Length};
use taskmanager_application::{ProcessInsightUnavailable, i18n::t};
use taskmanager_core::core::failure::FailureKind;
pub(crate) use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process_telemetry::LimitValue;
pub(crate) use taskmanager_core::core::process_telemetry::{OpenFileEntry, ProcessThreadInfo};
pub(crate) use taskmanager_platform_contract::SubmissionErrorKind;
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::theme;

pub(crate) const DASH: &str = taskmanager_shell::presentation::MISSING_VALUE;

pub(crate) fn section_column<'a>(
    theme_snapshot: &'a Theme,
    heading: &str,
    body: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut col = column![section_title(theme_snapshot, heading)];
    for child in body {
        col = col.push(child);
    }
    col.spacing(4).width(Length::Fill).into()
}

pub(crate) fn section_title<'a>(
    theme_snapshot: &'a Theme,
    heading: &str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    text(heading.to_string())
        .size(f32::from(tokens::FONT_13))
        .color(crate::theme_binding::color(theme_snapshot.palette().accent))
        .into()
}

pub(crate) fn muted_text<'a, S: iced::advanced::text::IntoFragment<'a>>(
    theme_snapshot: &'a Theme,
    body: S,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    text(body)
        .size(f32::from(tokens::FONT_12))
        .color(theme::muted_text_color(theme_snapshot))
        .into()
}

pub(crate) fn kv_row<'a, V: iced::advanced::text::IntoFragment<'a>>(
    theme_snapshot: &'a Theme,
    label: String,
    value: V,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        text(label).width(Length::Fixed(150.0)).color(muted),
        text(value).width(Length::Fill),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

pub(crate) fn thread_header<'a>(
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        text("TID").width(Length::Fixed(56.0)).color(muted),
        text("Name").width(Length::Fixed(196.0)).color(muted),
        text("State").width(Length::Fixed(48.0)).color(muted),
        text("CPU-time").width(Length::Fixed(72.0)).color(muted),
        text("CPU%").width(Length::Fill).color(muted),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

pub(crate) fn thread_row<'a>(
    thread: &ProcessThreadInfo,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let comm = if thread.comm.is_empty() {
        missing_value()
    } else {
        thread.comm.clone()
    };
    row![
        text(thread.tid.to_string()).width(Length::Fixed(56.0)),
        text(comm).width(Length::Fixed(196.0)),
        text(thread.state.as_short_label()).width(Length::Fixed(48.0)),
        text(cpu_time_text(thread.cpu_time_secs)).width(Length::Fixed(72.0)),
        text(cpu_percent_text(thread.cpu_percent)).width(Length::Fill),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

pub(crate) fn cpu_time_text(cpu: Option<f64>) -> String {
    cpu.map_or_else(|| DASH.to_string(), |value| format!("{value:.1}s"))
}

pub(crate) fn cpu_percent_text(cpu: Option<f32>) -> String {
    cpu.map_or_else(|| DASH.to_string(), |value| format!("{value:.1}%"))
}

pub(crate) fn format_open_file_row(entry: &OpenFileEntry) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| t("proc_insights.unreadable").to_string());
    format!("fd {} → {}", entry.fd, target)
}

pub(crate) fn format_engine_usage(
    name: &str,
    usage_pct: &ScalarObservation<f32>,
    time_ns: &ScalarObservation<u64>,
    cycles: &ScalarObservation<u64>,
) -> String {
    let usage = usage_pct
        .current_value()
        .map_or_else(|| DASH.to_string(), |value| format!("{value:.1}%"));
    let cumulative = time_ns
        .current_value()
        .map(|nanos| taskmanager_shell::presentation::duration(*nanos / 1_000_000_000))
        .or_else(|| {
            cycles
                .current_value()
                .map(|value| format!("{value} cycles"))
        })
        .unwrap_or_else(|| DASH.to_string());
    format!("{name}  {usage}  {cumulative}")
}

#[allow(dead_code)]
pub(crate) fn format_resource_pair(
    current: Option<String>,
    limit: Option<LimitValue>,
    format_val: impl Fn(u64) -> String,
) -> Option<String> {
    let limit_str = limit.map(|l| match l {
        LimitValue::Unlimited => "∞".to_string(),
        LimitValue::Value(v) => format_val(v),
    });
    match (current, limit_str) {
        (Some(c), Some(m)) => Some(format!("{c} / {m}")),
        (Some(c), None) => Some(c),
        (None, Some(m)) => Some(format!("{DASH} / {m}")),
        (None, None) => None,
    }
}

pub(crate) fn facet_unavailable_text(reason: &ProcessInsightUnavailable) -> String {
    match reason {
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied)
        | ProcessInsightUnavailable::Provider(FailureKind::RequiresEscalation) => {
            "permission denied"
        }
        ProcessInsightUnavailable::Provider(FailureKind::Unsupported)
        | ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            "unsupported"
        }
        _ => "unavailable",
    }
    .to_string()
}
