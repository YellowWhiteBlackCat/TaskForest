//! Presentation-only filesystem, SMART self-test, and sensor health surfaces.
//!
//! This module consumes typed snapshots and emits typed confirmation requests.
//! It performs no collection, filesystem access, or SMART command execution.

use gpui::{App, Div, ParentElement, ScrollHandle, Stateful, Styled, Window, div, px, relative};
use std::rc::Rc;
use taskmanager_application::SourceStateKind;

use crate::gpui_app::root::responsive::{SystemPageBudget, SystemSurfacePresentation};
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_core::core::{
    DeviceGeneration, DeviceId, DeviceState, DeviceStatus, FilesystemHealth,
    FilesystemHealthSnapshot, FilesystemHealthStatus, SensorCenterSnapshot, SensorQuantity,
    SensorReading, SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase,
    SmartSelfTestReport,
};
use taskmanager_theme::Color;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::scroll_region_with_rail;
use taskmanager_ui::primitives::card_surface::CardSurface;

mod capture;
pub use capture::{SystemHealthCaptureFixture, capture_english_text, capture_fixture};
mod localized;
pub use localized::localized_text;
mod self_test;
mod stats;
use self_test::self_test_card;
use stats::{filesystem_capacity, sensor_value_vm};
use taskmanager_theme::Theme;

/// Copy identifiers are resolved by the caller so this isolated component can
/// land before its final RootView/i18n integration without embedding production
/// English strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorGroup {
    Temperature,
    FanSpeed,
    Power,
}

impl SensorGroup {
    fn quantity(self) -> SensorQuantity {
        match self {
            Self::Temperature => SensorQuantity::Temperature,
            Self::FanSpeed => SensorQuantity::FanSpeed,
            Self::Power => SensorQuantity::Power,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemHealthText {
    StorageHealth,
    Filesystems,
    Space,
    Free,
    Inodes,
    ReadOnly,
    Errors,
    Source,
    SmartSelfTest,
    ShortTest,
    ExtendedTest,
    ConfirmationRequired,
    SensorCenter,
    NoFilesystems,
    NoReadings,
    Unavailable,
    Yes,
    No,
    Status,
    Progress,
    LifetimeHours,
    FirstErrorLba,
    SensorGroup(SensorGroup),
    DeviceStatus(DeviceStatus),
    FilesystemStatus(FilesystemHealthStatus),
    SmartPhase(SmartSelfTestPhase),
    SmartKind(SmartSelfTestKind),
    SmartFailure(SmartSelfTestFailure),
}

/// A non-executing request for the owning view to present a confirmation step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartSelfTestConfirmationRequest {
    pub device_id: DeviceId,
    pub device_generation: DeviceGeneration,
    pub disk_name: String,
    pub disk_label: String,
    pub kind: SmartSelfTestKind,
}

type ConfirmationCallback =
    dyn Fn(SmartSelfTestConfirmationRequest, &mut Window, &mut App) + 'static;

/// Callbacks deliberately stop at the confirmation boundary. RootView can later
/// own the pending intent and send a confirmed plan to its worker.
#[derive(Clone)]
pub struct SystemHealthCallbacks {
    request_confirmation: Rc<ConfirmationCallback>,
}

impl SystemHealthCallbacks {
    pub fn new(
        request_confirmation: impl Fn(SmartSelfTestConfirmationRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            request_confirmation: Rc::new(request_confirmation),
        }
    }
}

pub(crate) fn state_color(theme: &Theme, status: DeviceStatus) -> Color {
    source_state_color(theme, SourceStateKind::from_device_status(status))
}

/// Kind → tone color for every source-state badge on this page. The neutral
/// fold (`SourceStateKind::from_device_status`) owns status→kind; this map is
/// the page's only kind→palette decision, so the section badge, the SMART
/// badge, and future Source lines can never drift apart.
fn source_state_color(theme: &Theme, state: SourceStateKind) -> Color {
    match state {
        SourceStateKind::Ok => theme.cpu,
        SourceStateKind::Degraded | SourceStateKind::Stale => theme.disk,
        SourceStateKind::Failed => theme.danger,
        SourceStateKind::Unknown => theme.fg_dim,
    }
}

fn filesystem_color(theme: &Theme, status: FilesystemHealthStatus) -> Color {
    match status {
        FilesystemHealthStatus::Healthy => theme.cpu,
        FilesystemHealthStatus::ReadOnly => theme.disk,
        FilesystemHealthStatus::ErrorsReported => theme.danger,
    }
}

pub(crate) fn badge(theme: &Theme, label: String, color: Color) -> Div {
    div()
        .px(tokens::SPACE_7)
        .py(tokens::SPACE_3)
        .rounded(tokens::control_radius(theme))
        .bg(theme.shade)
        .text_size(tokens::FONT_11)
        .text_color(color)
        .child(label)
}

pub(crate) fn metric(theme: &Theme, label: String, value: String) -> Div {
    div()
        .flex_1()
        .min_w(px(118.0))
        .child(
            div()
                .text_size(tokens::FONT_10)
                .text_color(theme.fg_dim)
                .child(label),
        )
        .child(
            div()
                .mt(tokens::SPACE_2)
                .text_size(tokens::FONT_12)
                .text_color(theme.fg)
                .child(value),
        )
}

fn format_bytes(units: UnitPreferences, bytes: u64) -> String {
    units.format_quantity(bytes, QuantityFamily::Drive, false)
}

fn filesystem_row(
    theme: &Theme,
    filesystem: &FilesystemHealth,
    disk: Option<&DiskMetrics>,
    copy: &dyn Fn(SystemHealthText) -> String,
    units: UnitPreferences,
) -> Div {
    let unavailable = || copy(SystemHealthText::Unavailable);
    let source = filesystem
        .source
        .as_ref()
        .map(|source| source.display().to_string())
        .unwrap_or_else(&unavailable);
    let read_only = match filesystem.read_only {
        Some(true) => copy(SystemHealthText::Yes),
        Some(false) => copy(SystemHealthText::No),
        None => unavailable(),
    };
    let errors = filesystem
        .error_count
        .map(|count| count.to_string())
        .unwrap_or_else(&unavailable);
    let capacity = filesystem_capacity(filesystem, disk)
        .map(|(used_pct, available)| {
            format!(
                "{used_pct:.1}% · {} {}",
                format_bytes(units, available),
                copy(SystemHealthText::Free)
            )
        })
        .unwrap_or_else(&unavailable);
    div()
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
                        .min_w(px(0.0))
                        .font_weight(tokens::FONT_WEIGHT_HEADER.into())
                        .child(filesystem.mount_point.display().to_string()),
                )
                .child(badge(
                    theme,
                    copy(SystemHealthText::FilesystemStatus(filesystem.status)),
                    filesystem_color(theme, filesystem.status),
                )),
        )
        .child(
            div()
                .mt(tokens::SPACE_3)
                .text_size(tokens::FONT_10)
                .text_color(theme.fg_dim)
                .child(format!(
                    "{}: {} · {}",
                    copy(SystemHealthText::Source),
                    source,
                    filesystem.fs_type
                )),
        )
        .child(
            div()
                .mt(tokens::SPACE_7)
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(tokens::SPACE_8)
                .child(metric(theme, copy(SystemHealthText::Space), capacity))
                .child(metric(theme, copy(SystemHealthText::Inodes), unavailable()))
                .child(metric(theme, copy(SystemHealthText::ReadOnly), read_only))
                .child(metric(theme, copy(SystemHealthText::Errors), errors)),
        )
}

/// Storage-section data bundle: the health snapshot with its optional
/// selected-disk pair, grouped so the builder stays under the argument
/// ratchet once unit preferences joined the parameter list.
struct StorageSectionData<'a> {
    filesystems: &'a FilesystemHealthSnapshot,
    disk: Option<&'a DiskMetrics>,
    report: Option<&'a SmartSelfTestReport>,
}

fn storage_section(
    theme: &Theme,
    data: StorageSectionData<'_>,
    layout: SystemPageBudget,
    copy: &dyn Fn(SystemHealthText) -> String,
    callbacks: &SystemHealthCallbacks,
    units: UnitPreferences,
) -> Div {
    let mut rows = div()
        .mt(tokens::SPACE_8)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_7);
    if data.filesystems.filesystems.is_empty() {
        rows = rows.child(
            div()
                .py(tokens::SPACE_14)
                .text_color(theme.fg_dim)
                .child(copy(SystemHealthText::NoFilesystems)),
        );
    } else {
        for filesystem in &data.filesystems.filesystems {
            rows = rows.child(filesystem_row(theme, filesystem, data.disk, copy, units));
        }
    }
    section_shell(
        theme,
        copy(SystemHealthText::StorageHealth),
        data.filesystems.state,
        layout,
        rows.child(self_test_card(
            theme,
            data.disk,
            data.report,
            copy,
            callbacks,
        )),
        copy,
    )
}

fn sensor_group(
    theme: &Theme,
    group: SensorGroup,
    readings: &[SensorReading],
    copy: &dyn Fn(SystemHealthText) -> String,
) -> Div {
    let mut rows = div()
        .mt(tokens::SPACE_6)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_5);
    let mut count = 0;
    let quantity = group.quantity();
    for reading in readings
        .iter()
        .filter(|reading| reading.quantity() == &quantity)
    {
        count += 1;
        let value = sensor_value_vm(reading, copy);
        rows = rows.child(
            div()
                .p(tokens::SPACE_8)
                .rounded(tokens::control_radius(theme))
                .bg(theme.sidebar_card_bg)
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(tokens::SPACE_6)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(120.0))
                        .text_size(tokens::FONT_12)
                        .child(reading.label().to_owned()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .text_color(if value.present {
                            theme.fg
                        } else {
                            state_color(theme, reading.state().status)
                        })
                        .child(value.text),
                )
                .child(badge(
                    theme,
                    copy(SystemHealthText::DeviceStatus(reading.state().status)),
                    state_color(theme, reading.state().status),
                )),
        );
    }
    if count == 0 {
        rows = rows.child(
            div()
                .py(tokens::SPACE_8)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(copy(SystemHealthText::NoReadings)),
        );
    }
    div()
        .p(tokens::SPACE_8)
        .rounded(tokens::control_radius(theme))
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .font_weight(tokens::FONT_WEIGHT_HEADER.into())
                .child(copy(SystemHealthText::SensorGroup(group))),
        )
        .child(rows)
}

fn sensor_section(
    theme: &Theme,
    snapshot: &SensorCenterSnapshot,
    layout: SystemPageBudget,
    copy: &dyn Fn(SystemHealthText) -> String,
) -> Div {
    let mut groups = div()
        .mt(tokens::SPACE_8)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_7);
    for group in [
        SensorGroup::Temperature,
        SensorGroup::FanSpeed,
        SensorGroup::Power,
    ] {
        groups = groups.child(sensor_group(theme, group, &snapshot.readings, copy));
    }
    section_shell(
        theme,
        copy(SystemHealthText::SensorCenter),
        snapshot.state,
        layout,
        groups,
        copy,
    )
}

fn section_shell(
    theme: &Theme,
    title: String,
    state: DeviceState,
    layout: SystemPageBudget,
    content: Div,
    copy: &dyn Fn(SystemHealthText) -> String,
) -> Div {
    CardSurface::new(theme.palette())
        .background(theme.card_surface())
        .padding(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => tokens::SPACE_9,
            SystemSurfacePresentation::MultiColumn => tokens::SPACE_12,
        })
        .radius(tokens::card_radius(theme))
        .bordered(true)
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
                        .text_size(tokens::FONT_18)
                        .font_weight(tokens::FONT_WEIGHT_STRONG.into())
                        .text_color(theme.fg)
                        .child(title),
                )
                .child(badge(
                    theme,
                    copy(SystemHealthText::DeviceStatus(state.status)),
                    state_color(theme, state.status),
                )),
        )
        .child(content)
        .render()
        .w(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => relative(1.0),
            SystemSurfacePresentation::MultiColumn => relative(0.49),
        })
        .min_w(px(match layout.surfaces {
            SystemSurfacePresentation::SingleColumn => 0.0,
            SystemSurfacePresentation::MultiColumn => 320.0,
        }))
}

/// Render the two responsive health surfaces. The parent constrains this element;
/// the stateful outer container keeps all rows reachable at 720×480.
/// All straight-through system-health render inputs (design-debt #1 props
/// consolidation).
pub struct SystemHealthViewProps<'a> {
    pub theme: &'a Theme,
    pub scroll: &'a ScrollHandle,
    pub filesystems: &'a FilesystemHealthSnapshot,
    pub sensors: &'a SensorCenterSnapshot,
    pub selected_disk: Option<&'a DiskMetrics>,
    pub smart_report: Option<&'a SmartSelfTestReport>,
    pub layout: SystemPageBudget,
    pub copy: &'a dyn Fn(SystemHealthText) -> String,
    pub callbacks: &'a SystemHealthCallbacks,
    /// Presentation unit preferences for the capacity readouts.
    pub units: UnitPreferences,
}

pub fn render_system_health(props: SystemHealthViewProps<'_>) -> Stateful<Div> {
    let SystemHealthViewProps {
        theme,
        scroll,
        filesystems,
        sensors,
        selected_disk,
        smart_report,
        layout,
        copy,
        callbacks,
        units,
    } = props;
    scroll_region_with_rail(
        "system-health-scroll",
        "tm-system-health-scroll",
        "system-health-scrollbar",
        "tm-system-health-scrollbar",
        scroll.clone(),
        theme.palette(),
        div()
            .w_full()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap(tokens::SPACE_10)
            .child(storage_section(
                theme,
                StorageSectionData {
                    filesystems,
                    disk: selected_disk,
                    report: smart_report,
                },
                layout,
                copy,
                callbacks,
                units,
            ))
            .child(sensor_section(theme, sensors, layout, copy)),
    )
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/system_health_view/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_system_health_view_source_state_parity_tests.rs"]
mod source_state_parity_tests;
