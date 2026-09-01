//! Process properties and details modal projection for Iced.

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Element, Length};
use taskmanager_application::ProcessInsightFacetState;
use taskmanager_application::i18n::t;
use taskmanager_application::process_details_vm::{
    DetailValue, ProcessDetailsField, ProcessDetailsRowVm, detail_value,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem, ProcessLiveKey};
use taskmanager_core::core::process_telemetry::{ProcessEnvironment, ProcessEnvironmentEntry};
use taskmanager_core::core::time::LocalTimeRulesObservation;
use taskmanager_core::core::units::UnitPreferences;

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_theme::tokens;

use crate::app::Message;
use crate::focus;
use crate::ui::components::key_value_rows;
use crate::ui::device_chart;

/// One VM value as this overlay's display string: the folded text, or the
/// shared dash for [`DetailValue::Missing`].
fn vm_text(rows: &[ProcessDetailsRowVm], field: ProcessDetailsField) -> String {
    detail_value(rows, field).text_or(MISSING_VALUE).to_owned()
}

/// The CPU readout keeps Iced's right-aligned width-6 cell as a layout
/// convention layered on the VM's `{:.1}%` fold (the number format, units,
/// and missing semantics come from the VM).
fn vm_cpu_cell(rows: &[ProcessDetailsRowVm]) -> String {
    match detail_value(rows, ProcessDetailsField::Cpu) {
        DetailValue::Text(text) => format!("{text:>6}"),
        DetailValue::Missing => MISSING_VALUE.to_owned(),
    }
}

/// Render the shared process-properties overlay as a real details modal.
pub(crate) fn details_overlay<'a>(
    app: &'a crate::IcedApp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let shell = &app.shell;
    let appear = app.modal_appear_progress();
    let Some(target) = properties_target(shell).cloned() else {
        return super::modal_overlay(
            theme_snapshot,
            t("prop.process_details"),
            t("empty.no_process_selected"),
            column![].into(),
            appear,
        );
    };
    let Some(identity) = target.live_key() else {
        return super::modal_overlay(
            theme_snapshot,
            t("prop.process_details"),
            t("prop.frozen_hint"),
            column![].into(),
            appear,
        );
    };

    let tabs: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        crate::app::DetailsSection::ALL
            .into_iter()
            .map(|section| {
                focus::choice_pill(
                    theme_snapshot,
                    crate::app::FocusTarget::DetailsTab(section),
                    details_tab_label(section).to_string(),
                    section == app.details_section(),
                    Message::SelectDetailsSection(section),
                )
            })
            .collect();

    let body: Element<'a, Message, iced::Theme, iced::Renderer> = match app.details_section() {
        crate::app::DetailsSection::Overview => key_value_rows(overview_rows_with_local_time(
            identity,
            shell,
            &app.local_time_rules,
        )),
        crate::app::DetailsSection::Performance => performance_tab(app, identity),
        crate::app::DetailsSection::Command => command_tab(app, identity),
        crate::app::DetailsSection::Insights => {
            crate::ui::insights::insights_block(theme_snapshot, shell, &target)
        }
    };

    let header = row![
        text(target.name).size(f32::from(tokens::FONT_16)),
        text(format!("PID {}", target.pid)).size(f32::from(tokens::FONT_12)),
    ]
    .spacing(8);

    super::modal_overlay(
        theme_snapshot,
        t("prop.process_details"),
        t("prop.frozen_hint"),
        column![
            header,
            row(tabs).spacing(4),
            scrollable(body)
                .height(Length::Fixed(440.0))
                .width(Length::Fill),
        ]
        .spacing(8)
        .into(),
        appear,
    )
}

fn details_tab_label(section: crate::app::DetailsSection) -> &'static str {
    match section {
        crate::app::DetailsSection::Overview => t("prop.overview"),
        crate::app::DetailsSection::Performance => t("prop.performance"),
        crate::app::DetailsSection::Command => t("prop.command"),
        crate::app::DetailsSection::Insights => t("prop.insights"),
    }
}

fn overview_rows_with_local_time(
    identity: ProcessLiveKey,
    shell: &ShellApp,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<(String, String)> {
    let Some(process) = shell.visible_process_by_identity(identity) else {
        return property_rows(identity, shell);
    };
    property_pairs(process, local_time_rules)
        .into_iter()
        .filter(|(field, _, _)| {
            !matches!(
                field,
                ProcessDetailsField::Cmdline | ProcessDetailsField::Exe
            )
        })
        .map(|(_, label, value)| (label, value))
        .collect()
}

fn command_rows_with_local_time(
    identity: ProcessLiveKey,
    shell: &ShellApp,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<(String, String)> {
    let process = shell.visible_process_by_identity(identity);
    let mut rows = Vec::new();
    let Some(process) = process else {
        push_property(&mut rows, t("common.name"), Some(MISSING_VALUE));
        push_property(&mut rows, t("common.status"), Some(MISSING_VALUE));
        return rows;
    };
    let vm = details_vm(process, local_time_rules);
    push_property(
        &mut rows,
        t("common.name"),
        detail_value(&vm, ProcessDetailsField::Name).as_str(),
    );
    push_property(
        &mut rows,
        t("common.executable"),
        detail_value(&vm, ProcessDetailsField::Exe).as_str(),
    );
    let cmdline = vm_text(&vm, ProcessDetailsField::Cmdline);
    push_property(&mut rows, t("prop.command_line"), Some(cmdline.as_str()));
    rows
}

/// The environment facet for the currently open properties target: `None`
/// before the projection arrives, then Pending/Current/Unavailable exactly as
/// the platform adapter reported it. Never fabricates an empty table.
pub(crate) fn environment_facet<'a>(
    shell: &'a ShellApp,
    target: &FrozenProcessIdentity,
) -> Option<&'a ProcessInsightFacetState<ProcessEnvironment>> {
    shell
        .projection()
        .process_insights
        .as_ref()
        .filter(|projection| &projection.target == target)
        .map(|projection| &projection.environment)
}

/// The bounded environment rows after the renderer-local key filter. Pure so
/// the headless tests drive the same seam the modal renders.
#[must_use]
pub(crate) fn filtered_environment_rows<'a>(
    entries: &'a [ProcessEnvironmentEntry],
    filter: &str,
) -> Vec<&'a taskmanager_core::core::process_telemetry::ProcessEnvironmentEntry> {
    let needle = filter.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| needle.is_empty() || entry.key.to_lowercase().contains(&needle))
        .collect()
}

fn command_tab<'a>(
    app: &'a crate::IcedApp,
    identity: ProcessLiveKey,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let shell = &app.shell;
    let mut rows = command_rows_with_local_time(identity, shell, &app.local_time_rules);
    let process = shell.visible_process_by_identity(identity);
    let mut actions = Vec::new();
    if let Some(cmd) = process
        .map(|p| p.cmdline.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        actions.push(focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::AboutCopyDetails,
            t("common.copy_command").to_string(),
            Message::CopyTextToClipboard {
                label: "Command Line".to_string(),
                text: cmd.to_string(),
            },
            false,
        ));
    }
    if let Some(exe) = super::process_details_projection::executable_path(process) {
        actions.push(focus::dynamic_button(
            theme_snapshot,
            crate::app::FocusTarget::AboutCopyDetails,
            t("common.copy_path").to_string(),
            Message::CopyTextToClipboard {
                label: "Executable Path".to_string(),
                text: exe,
            },
            false,
        ));
    }
    if let Some(target) = properties_target(shell) {
        rows.push((
            t("prop.working_directory").to_string(),
            working_directory_value(shell, target),
        ));
    }
    let mut content = column![key_value_rows(rows)].spacing(8);
    if !actions.is_empty() {
        content = content.push(row(actions).spacing(8));
    }
    if let Some(target) = properties_target(shell) {
        content = content.push(environment_section(
            theme_snapshot,
            environment_facet(shell, target),
            app.process_presentation.env_filter.as_str(),
        ));
    }
    content.into()
}

/// The typed working-directory readout for the command tab: collecting while
/// the insight is in flight, the typed reason when the platform cannot read
/// it, the proven path when it can, and a dash only for a proven absence.
#[must_use]
pub(crate) fn working_directory_value(shell: &ShellApp, target: &FrozenProcessIdentity) -> String {
    match environment_facet(shell, target) {
        None | Some(ProcessInsightFacetState::Pending) => t("proc_insights.collecting").to_string(),
        Some(ProcessInsightFacetState::Unavailable(reason)) => {
            crate::ui::insights::facet_unavailable_text(reason)
        }
        Some(ProcessInsightFacetState::Current(environment)) => environment
            .working_directory
            .as_deref()
            .and_then(|path| path.to_str())
            .filter(|path| !path.is_empty())
            .map_or_else(|| MISSING_VALUE.to_string(), str::to_string),
    }
}

/// The environment variables block: a bounded key/value table with a key
/// filter, per-row copy, copy-all, and an honest Pending / Unavailable state
/// when the platform cannot (or has not yet) provided the data.
fn environment_section<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: Option<&ProcessInsightFacetState<ProcessEnvironment>>,
    filter: &str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = crate::theme::muted_text_color(theme_snapshot);
    let (subheading, body): (
        Option<String>,
        Element<'a, Message, iced::Theme, iced::Renderer>,
    ) = match state {
        None | Some(ProcessInsightFacetState::Pending) => (
            None,
            text(t("proc_insights.collecting"))
                .size(f32::from(tokens::FONT_12))
                .color(muted)
                .into(),
        ),
        Some(ProcessInsightFacetState::Unavailable(reason)) => (
            None,
            text(crate::ui::insights::facet_unavailable_text(reason))
                .size(f32::from(tokens::FONT_12))
                .color(muted)
                .into(),
        ),
        Some(ProcessInsightFacetState::Current(environment)) => {
            let visible = filtered_environment_rows(&environment.entries, filter);
            let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = visible
                .iter()
                .map(|entry| {
                    row![
                        text(entry.key.clone())
                            .width(Length::Fixed(220.0))
                            .size(f32::from(tokens::FONT_12)),
                        text(entry.value.clone())
                            .width(Length::Fill)
                            .size(f32::from(tokens::FONT_12))
                            .color(muted),
                        focus::dynamic_button(
                            theme_snapshot,
                            crate::app::FocusTarget::AboutCopyDetails,
                            t("common.copy").to_string(),
                            Message::CopyTextToClipboard {
                                label: format!("Environment {}", entry.key),
                                text: format!("{}={}", entry.key, entry.value),
                            },
                            false,
                        ),
                    ]
                    .spacing(8)
                    .padding(2)
                    .width(Length::Fill)
                    .into()
                })
                .collect();
            if visible.is_empty() && !environment.entries.is_empty() {
                rows.push(
                    text(t("prop.environment_no_match"))
                        .size(f32::from(tokens::FONT_12))
                        .color(muted)
                        .into(),
                );
            }
            if environment.entries.is_empty() && environment.truncated_count == 0 {
                rows.push(
                    text(t("prop.environment_empty"))
                        .size(f32::from(tokens::FONT_12))
                        .color(muted)
                        .into(),
                );
            }
            let mut subheading = visible.len().to_string();
            if environment.truncated_count > 0 {
                subheading.push_str(&format!(
                    " · {}",
                    t("prop.environment_truncated").replacen(
                        "{}",
                        &environment.truncated_count.to_string(),
                        1
                    )
                ));
            }
            let copy_all = (!visible.is_empty()).then(|| {
                focus::dynamic_button(
                    theme_snapshot,
                    crate::app::FocusTarget::AboutCopyDetails,
                    t("system_about.copy_all").to_string(),
                    Message::CopyTextToClipboard {
                        label: "Environment".to_string(),
                        text: visible
                            .iter()
                            .map(|entry| format!("{}={}", entry.key, entry.value))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    },
                    false,
                )
            });
            let filter_input: Element<'a, Message, iced::Theme, iced::Renderer> =
                text_input(t("prop.environment_filter"), filter)
                    .on_input(Message::EnvironmentFilterChanged)
                    .width(Length::Fixed(220.0))
                    .into();
            let mut block =
                column![row![filter_input].spacing(8), column(rows).spacing(1),].spacing(4);
            if let Some(copy_all) = copy_all {
                block = block.push(row![copy_all].spacing(8));
            }
            (
                Some(subheading),
                container(block).width(Length::Fill).into(),
            )
        }
    };
    let heading = match subheading {
        Some(subheading) => format!("{} · {subheading}", t("prop.environment")),
        None => t("prop.environment").to_string(),
    };
    container(column![
        text(heading).size(f32::from(tokens::FONT_13)),
        body,
    ])
    .style(move |_| crate::theme::card_style(theme_snapshot))
    .padding(8)
    .width(Length::Fill)
    .into()
}

fn performance_tab<'a>(
    app: &'a crate::IcedApp,
    identity: ProcessLiveKey,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let smooth = true;
    let process = app.shell.visible_process_by_identity(identity);
    let local_time_rules = &app.local_time_rules;
    let history = app.process_perf_series();
    let cpu = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.cpu),
    );
    let memory = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.memory),
    );
    let read = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.disk_read),
    );
    let write = history.as_ref().map_or_else(
        || std::rc::Rc::from([].as_slice()),
        |snapshot| std::rc::Rc::clone(&snapshot.disk_write),
    );

    let cpu_caption = process.map_or_else(
        || t("common.cpu").to_string(),
        |process| {
            let vm = details_vm(process, local_time_rules);
            match detail_value(&vm, ProcessDetailsField::Cpu) {
                DetailValue::Text(_) => {
                    format!("{}  {}", t("common.cpu"), vm_cpu_cell(&vm))
                }
                DetailValue::Missing => format!("{}  {}", t("common.cpu"), MISSING_VALUE),
            }
        },
    );
    let memory_caption = process.map_or_else(
        || t("common.memory").to_string(),
        |process| {
            format!(
                "{}  {}",
                t("common.memory"),
                vm_text(
                    &details_vm(process, local_time_rules),
                    ProcessDetailsField::Memory
                )
            )
        },
    );
    let read_caption = process.map_or_else(
        || t("proc.disk_read").to_string(),
        |process| {
            format!(
                "{}  {}",
                t("proc.disk_read"),
                vm_text(
                    &details_vm(process, local_time_rules),
                    ProcessDetailsField::DiskReadRate
                )
            )
        },
    );
    let write_caption = process.map_or_else(
        || t("proc.disk_write").to_string(),
        |process| {
            format!(
                "{}  {}",
                t("proc.disk_write"),
                vm_text(
                    &details_vm(process, local_time_rules),
                    ProcessDetailsField::DiskWriteRate
                )
            )
        },
    );

    let palette = theme_snapshot.palette();
    column![
        device_chart::device_mini_graph(
            cpu,
            device_chart::DeviceMetricScale::Percent,
            crate::theme_binding::color(palette.accent),
            cpu_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            memory,
            device_chart::DeviceMetricScale::AutoPeak,
            crate::theme_binding::color(palette.success),
            memory_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            read,
            device_chart::DeviceMetricScale::AutoPeak,
            crate::theme_binding::color(theme_snapshot.disk),
            read_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
        device_chart::device_mini_graph(
            write,
            device_chart::DeviceMetricScale::AutoPeak,
            crate::theme_binding::color(theme_snapshot.disk),
            write_caption,
            theme_snapshot,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: false,
            },
        ),
    ]
    .spacing(8)
    .into()
}

pub(crate) fn properties_target(shell: &ShellApp) -> Option<&FrozenProcessIdentity> {
    shell.process_properties_target()
}

#[must_use]
pub(crate) fn property_rows(identity: ProcessLiveKey, shell: &ShellApp) -> Vec<(String, String)> {
    property_rows_with_local_time(
        identity,
        shell,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    )
}

fn property_rows_with_local_time(
    identity: ProcessLiveKey,
    shell: &ShellApp,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<(String, String)> {
    let process = shell.visible_process_by_identity(identity);
    let Some(process) = process else {
        let mut rows = Vec::new();
        push_property(&mut rows, t("common.name"), Some(MISSING_VALUE));
        push_property(
            &mut rows,
            t("common.status"),
            Some(t("feedback.process_gone")),
        );
        return rows;
    };
    property_pairs(process, local_time_rules)
        .into_iter()
        .map(|(_, label, value)| (label, value))
        .collect()
}

/// The pure per-field fold for one live process: `(field, label, value)`
/// rows straight off the neutral process-details VM (the single typed
/// observation→display fold shared by every frontend). Iced's historical
/// row-omission policy stays presentational: an unavailable CPU / parent
/// PID / executable drops the row, every other missing observation renders
/// the shared dash.
#[must_use]
fn property_pairs(
    process: &ProcessItem,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<(ProcessDetailsField, String, String)> {
    let vm = details_vm(process, local_time_rules);
    let row =
        |field: ProcessDetailsField, label: &str| (field, label.to_owned(), vm_text(&vm, field));
    let mut pairs = vec![
        row(ProcessDetailsField::Name, t("common.name")),
        row(ProcessDetailsField::Pid, "PID"),
        row(ProcessDetailsField::User, t("common.user")),
        row(ProcessDetailsField::Status, t("common.status")),
    ];
    if let DetailValue::Text(_) = detail_value(&vm, ProcessDetailsField::Cpu) {
        pairs.push((
            ProcessDetailsField::Cpu,
            t("common.cpu").to_owned(),
            vm_cpu_cell(&vm),
        ));
    }
    pairs.extend([
        row(ProcessDetailsField::Memory, t("common.memory")),
        row(ProcessDetailsField::Threads, t("common.threads")),
        row(ProcessDetailsField::Fds, t("proc.fds")),
        row(ProcessDetailsField::Nice, t("proc.nice")),
    ]);
    if let DetailValue::Text(_) = detail_value(&vm, ProcessDetailsField::ParentPid) {
        pairs.push(row(ProcessDetailsField::ParentPid, t("prop.parent_pid")));
    }
    pairs.extend([
        row(ProcessDetailsField::StartTime, t("prop.start_time")),
        row(ProcessDetailsField::CpuTime, t("proc.cpu_time")),
        row(ProcessDetailsField::DiskReadTotal, t("proc.disk_read")),
        row(ProcessDetailsField::DiskWriteTotal, t("proc.disk_write")),
        row(ProcessDetailsField::Cmdline, t("prop.command_line")),
    ]);
    if let DetailValue::Text(_) = detail_value(&vm, ProcessDetailsField::Exe) {
        pairs.push(row(ProcessDetailsField::Exe, t("common.executable")));
    }
    pairs
}

fn details_vm(
    process: &ProcessItem,
    local_time_rules: &LocalTimeRulesObservation,
) -> Vec<ProcessDetailsRowVm> {
    taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        process,
        &UnitPreferences::default(),
        local_time_rules,
    )
}

fn push_property(rows: &mut Vec<(String, String)>, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        rows.push((label.to_string(), value.to_string()));
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/overlays/process_details_vm_parity_tests.rs"]
mod vm_parity_tests;
