//! Formatting and layout helpers for Process Insights in Iced.

use iced::widget::{column, row, text};
use iced::{Element, Length};
use taskmanager_application::{ProcessInsightUnavailable, i18n::t};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics;
use taskmanager_core::core::process_telemetry;
use taskmanager_platform_contract::SubmissionErrorKind;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::presentation::duration;
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::theme;

pub(crate) use metrics::ScalarObservation;
pub(crate) use process_telemetry::{OpenFileEntry, ProcessThreadInfo};

pub(crate) const DASH: &str = MISSING_VALUE;

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
        text("Wait").width(Length::Fixed(72.0)).color(muted),
        text("CPU%").width(Length::Fill).color(muted),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

pub(crate) fn thread_row<'a>(vm: ThreadRowVm) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(vm.tid).width(Length::Fixed(56.0)),
        text(vm.comm).width(Length::Fixed(196.0)),
        text(vm.state).width(Length::Fixed(48.0)),
        text(vm.cpu_time).width(Length::Fixed(72.0)),
        text(vm.wait).width(Length::Fixed(72.0)),
        text(vm.cpu_percent).width(Length::Fill),
    ]
    .spacing(8)
    .padding(2)
    .width(Length::Fill)
    .into()
}

/// Reified thread-row cells: the exact strings [`thread_row`] paints, folded
/// out so the row contract (one row per thread, explicit dash for an
/// unparsed counter) is observable without an `Element` readback. The
/// element builder consumes this VM 1:1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadRowVm {
    pub(crate) tid: String,
    pub(crate) comm: String,
    pub(crate) state: String,
    pub(crate) cpu_time: String,
    pub(crate) wait: String,
    pub(crate) cpu_percent: String,
}

#[must_use]
pub(crate) fn thread_row_vm(thread: &ProcessThreadInfo) -> ThreadRowVm {
    let comm = if thread.comm.is_empty() {
        missing_value()
    } else {
        thread.comm.clone()
    };
    let wait = thread.run_queue_wait_ns.map_or_else(
        || DASH.to_string(),
        |nanos| {
            let kind = thread
                .wait_kind
                .map(process_telemetry::ThreadWaitKind::as_str)
                .unwrap_or("wait");
            format!("{kind} {:.1}ms", nanos as f64 / 1_000_000.0)
        },
    );
    ThreadRowVm {
        tid: thread.tid.to_string(),
        comm,
        state: thread.state.as_short_label().to_string(),
        cpu_time: cpu_time_text(thread.cpu_time_secs),
        wait,
        cpu_percent: cpu_percent_text(thread.cpu_percent),
    }
}

/// The bounded thread rows the facet paints: one row per thread (projection
/// order), capped at `cap` materialized rows. A thread with unparsed CPU
/// counters keeps the explicit dash — never a fabricated `0.0s`/`0.0%`.
#[must_use]
pub(crate) fn thread_rows_vm(threads: &[ProcessThreadInfo], cap: usize) -> Vec<ThreadRowVm> {
    threads.iter().take(cap).map(thread_row_vm).collect()
}

/// The bounded open-file rows the facet paints: `fd → target` per descriptor
/// (projection order), capped at `cap` materialized rows, with an unresolved
/// readlink kept as the typed unreadable marker.
#[must_use]
pub(crate) fn open_file_rows(entries: &[OpenFileEntry], cap: usize) -> Vec<String> {
    entries.iter().take(cap).map(format_open_file_row).collect()
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
        .map(|nanos| duration(*nanos / 1_000_000_000))
        .or_else(|| {
            cycles
                .current_value()
                .map(|value| format!("{value} cycles"))
        })
        .unwrap_or_else(|| DASH.to_string());
    format!("{name}  {usage}  {cumulative}")
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
