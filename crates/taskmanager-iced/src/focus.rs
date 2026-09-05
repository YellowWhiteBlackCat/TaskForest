//! Real Iced focusable controls for the frontend adapter.
//!
//! Iced 0.14 exposes focus traversal operations, but its stock button does
//! not implement `operation::Focusable`. This small wrapper owns only the
//! toolkit edge: the shell still owns modal state and the wrapper publishes a
//! normal [`crate::app::Message`] for activation/focus. It is deliberately
//! reusable so later modal controls do not smuggle focus state into the shell.

use iced::Element;
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;

use crate::app::{FocusTarget, Message};

mod widget;
pub(crate) use widget::*;

#[cfg(test)]
#[path = "../tests/gui/focus/tests.rs"]
mod tests;

/// Stable ID for the current modal focus stop.
pub(crate) const MODAL_CLOSE_ID: &str = "iced-modal-close";

/// Return the stable Iced operation ID for one renderer-local focus target.
#[must_use]
pub(crate) fn focus_id(target: FocusTarget) -> String {
    match target {
        FocusTarget::ModalClose => MODAL_CLOSE_ID.to_owned(),
        FocusTarget::TableRow { page, index } => {
            format!("iced-table-row-{}-{index}", page_key(page))
        }
        FocusTarget::PageTab(page) => format!("iced-page-tab-{}", page_key(page)),
        FocusTarget::SearchTrigger => "iced-search-trigger".to_owned(),
        FocusTarget::SearchClose => "iced-search-close".to_owned(),
        FocusTarget::ProcessColumnsTrigger => "iced-process-columns-trigger".to_owned(),
        FocusTarget::ProcessColumnToggle(column) => {
            format!("iced-process-column-toggle-{}", column.label())
        }
        FocusTarget::ProcessColumnNarrow(column) => {
            format!("iced-process-column-narrow-{}", column.label())
        }
        FocusTarget::ProcessColumnWiden(column) => {
            format!("iced-process-column-widen-{}", column.label())
        }
        FocusTarget::ProcessColumnsClose => "iced-process-columns-close".to_owned(),
        FocusTarget::ServicesSearch => "iced-services-search".to_owned(),
        FocusTarget::SourceRetry(request) => {
            format!("iced-source-retry-{}", refresh_request_key(request))
        }
        FocusTarget::AuthorizeRaplPower => "iced-authorize-rapl-power".to_owned(),
        FocusTarget::AuthorizeMsrReadouts => "iced-authorize-msr-readouts".to_owned(),
        FocusTarget::EndTask => "iced-end-task".to_owned(),
        FocusTarget::OpenProcessLocation => "iced-open-process-location".to_owned(),
        FocusTarget::SearchProcessOnline => "iced-search-process-online".to_owned(),
        FocusTarget::ProcessAffinityOpen => "iced-process-affinity-open".to_owned(),
        FocusTarget::ProcessAffinityCpu(cpu) => format!("iced-process-affinity-cpu-{cpu}"),
        FocusTarget::ProcessAffinityApply => "iced-process-affinity-apply".to_owned(),
        FocusTarget::ProcessNetworkEscalation => "iced-process-net-escalation".to_owned(),
        FocusTarget::DetailsTab(section) => {
            format!("iced-details-tab-{}", section.key())
        }
        FocusTarget::SuspendProcess => "iced-suspend-process".to_owned(),
        FocusTarget::ResumeProcess => "iced-resume-process".to_owned(),
        FocusTarget::KillProcess => "iced-kill-process".to_owned(),
        FocusTarget::ProcessMenuEndTask => "iced-process-menu-end-task".to_owned(),
        FocusTarget::ProcessMenuEndTree => "iced-process-menu-end-tree".to_owned(),
        FocusTarget::ProcessMenuKill => "iced-process-menu-kill".to_owned(),
        FocusTarget::ProcessMenuSuspend => "iced-process-menu-suspend".to_owned(),
        FocusTarget::ProcessMenuResume => "iced-process-menu-resume".to_owned(),
        FocusTarget::ProcessMenuSignalHangup => "iced-process-menu-signal-hangup".to_owned(),
        FocusTarget::ProcessMenuSignalInterrupt => "iced-process-menu-signal-interrupt".to_owned(),
        FocusTarget::ProcessMenuSignalUser1 => "iced-process-menu-signal-user1".to_owned(),
        FocusTarget::ProcessMenuSignalUser2 => "iced-process-menu-signal-user2".to_owned(),
        FocusTarget::ProcessMenuOpenLocation => "iced-process-menu-open-location".to_owned(),
        FocusTarget::ProcessMenuSearchOnline => "iced-process-menu-search-online".to_owned(),
        FocusTarget::ProcessMenuProperties => "iced-process-menu-properties".to_owned(),
        FocusTarget::ProcessMenuCopyName => "iced-process-menu-copy-name".to_owned(),
        FocusTarget::ProcessMenuCopyPid => "iced-process-menu-copy-pid".to_owned(),
        FocusTarget::ProcessMenuCopyCommandLine => "iced-process-menu-copy-command-line".to_owned(),
        FocusTarget::ProcessMenuClose => "iced-process-menu-close".to_owned(),
        FocusTarget::ConfirmEndTask => "iced-confirm-end-task".to_owned(),
        FocusTarget::ConfirmProcessBatch => "iced-confirm-process-batch".to_owned(),
        FocusTarget::CancelEndTask => "iced-cancel-end-task".to_owned(),
        FocusTarget::SessionDisconnect => "iced-session-disconnect".to_owned(),
        FocusTarget::SessionLock => "iced-session-lock".to_owned(),
        FocusTarget::UserRowMenuDisconnect => "iced-user-row-menu-disconnect".to_owned(),
        FocusTarget::UserRowMenuLock => "iced-user-row-menu-lock".to_owned(),
        FocusTarget::UserRowMenuClose => "iced-user-row-menu-close".to_owned(),
        FocusTarget::SettingsTrigger => "iced-settings-trigger".to_owned(),
        FocusTarget::ContainersTrigger => "iced-containers-trigger".to_owned(),
        FocusTarget::HealthTrigger => "iced-health-trigger".to_owned(),
        FocusTarget::AboutTrigger => "iced-about-trigger".to_owned(),
        FocusTarget::Export => "iced-export".to_owned(),
        FocusTarget::WindowCapture => "iced-window-capture".to_owned(),
        FocusTarget::ServiceAction { index, action } => {
            format!("iced-service-action-{index}-{action:?}")
        }
        FocusTarget::ServiceMenuAction { index, action } => {
            format!("iced-service-menu-action-{index}-{action:?}")
        }
        FocusTarget::ServiceMenuClose => "iced-service-menu-close".to_owned(),
        FocusTarget::ServiceLogOpen { index } => format!("iced-service-log-open-{index}"),
        FocusTarget::ServiceDetailsOpen { index } => {
            format!("iced-service-details-open-{index}")
        }
        FocusTarget::ConfirmServiceControl => "iced-confirm-service-control".to_owned(),
        FocusTarget::CancelServiceControl => "iced-cancel-service-control".to_owned(),
        FocusTarget::StartupControl => "iced-startup-control".to_owned(),
        FocusTarget::StartupMenuAction { index, enabled } => {
            format!(
                "iced-startup-menu-action-{index}-{}",
                if enabled { "enable" } else { "disable" }
            )
        }
        FocusTarget::StartupMenuClose => "iced-startup-menu-close".to_owned(),
        FocusTarget::ConfirmStartupControl => "iced-confirm-startup-control".to_owned(),
        FocusTarget::SettingsChoice { section, index } => {
            format!("iced-settings-choice-{section}-{index}")
        }
        FocusTarget::PerfDeviceTab(device) => format!(
            "iced-perf-tab-{}-{}",
            device.key(),
            device.index().unwrap_or(0)
        ),
        FocusTarget::ProcessStatusFilterTab(filter) => {
            format!("iced-process-status-filter-{}", filter.key())
        }
        FocusTarget::ServiceDetailsLogPause => "iced-service-details-log-pause".to_owned(),
        FocusTarget::ServiceDetailsLogLevel => "iced-service-details-log-level".to_owned(),
        FocusTarget::ServiceDetailsLogTime => "iced-service-details-log-time".to_owned(),
        FocusTarget::ServiceDetailsLogCopy => "iced-service-details-log-copy".to_owned(),
        FocusTarget::ServiceDetailsLogRefresh => "iced-service-details-log-refresh".to_owned(),
        FocusTarget::DirectoryUsageScan => "iced-directory-usage-scan".to_owned(),
        FocusTarget::DirectoryUsageCancel => "iced-directory-usage-cancel".to_owned(),
        FocusTarget::DiskSmartOpen { index } => format!("iced-disk-smart-open-{index}"),
        FocusTarget::SmartSelfTestShort { index } => format!("iced-smart-self-test-short-{index}"),
        FocusTarget::SmartSelfTestExtended { index } => {
            format!("iced-smart-self-test-extended-{index}")
        }
        FocusTarget::ConfirmSmartSelfTest => "iced-confirm-smart-self-test".to_owned(),
        FocusTarget::CancelSmartSelfTest => "iced-cancel-smart-self-test".to_owned(),
        FocusTarget::GpuEnginesExpandToggle => "iced-gpu-engines-expand-toggle".to_owned(),
        FocusTarget::GpuEngineRowsToggle => "iced-gpu-engine-rows-toggle".to_owned(),
        FocusTarget::AboutCopyDetails => "iced-about-copy-details".to_owned(),
        FocusTarget::ServiceLogFollow => "iced-service-log-follow".to_owned(),
        FocusTarget::ServiceLogPause => "iced-service-log-pause".to_owned(),
        FocusTarget::ServiceLogLevel => "iced-service-log-level".to_owned(),
        FocusTarget::ServiceLogTime => "iced-service-log-time".to_owned(),
        FocusTarget::ServiceLogCopy => "iced-service-log-copy".to_owned(),
        FocusTarget::ServiceLogExport => "iced-service-log-export".to_owned(),
        FocusTarget::ServiceDetailsRetry => "iced-service-details-retry".to_owned(),
        FocusTarget::SavedViewPreset(id) => format!("iced-saved-view-preset-{id}"),
        FocusTarget::SavedViewSaveCurrent => "iced-saved-view-save-current".to_owned(),
        FocusTarget::SavedViewExport => "iced-saved-view-export".to_owned(),
        FocusTarget::SavedViewImport => "iced-saved-view-import".to_owned(),
        FocusTarget::HistoryReplayToggle => "iced-history-replay-toggle".to_owned(),
        FocusTarget::HistoryReplayWindow(w) => format!("iced-history-replay-window-{w:?}"),
        FocusTarget::HistoryReplayRefresh => "iced-history-replay-refresh".to_owned(),
        FocusTarget::AlertCenterClear => "iced-alert-center-clear".to_owned(),
        FocusTarget::AlertCenterExport => "iced-alert-center-export".to_owned(),
        FocusTarget::ProcessMenuCopyTsv => "iced-process-menu-copy-tsv".to_owned(),
        FocusTarget::ProcessMenuCopyJson => "iced-process-menu-copy-json".to_owned(),
        FocusTarget::RunTaskOpen => "iced-run-task-open".to_owned(),
        FocusTarget::RunTaskCommandInput => "iced-run-task-command-input".to_owned(),
        FocusTarget::RunTaskSubmit => "iced-run-task-submit".to_owned(),
        FocusTarget::RunTaskCancel => "iced-run-task-cancel".to_owned(),
        FocusTarget::AlertsPageTab => "iced-alerts-page-tab".to_owned(),
        FocusTarget::AlertsRuleToggle(index) => format!("iced-alerts-rule-toggle-{index}"),
        FocusTarget::AlertsExport => "iced-alerts-export".to_owned(),
        FocusTarget::AlertsImport => "iced-alerts-import".to_owned(),
        FocusTarget::FirstRunCopy(row) => format!("iced-first-run-copy-{row}"),
        FocusTarget::FirstRunAction(index) => format!("iced-first-run-action-{index}"),
        FocusTarget::ProcessAffinitySelectAll => "iced-process-affinity-select-all".to_owned(),
        FocusTarget::ProcessAffinityClearAll => "iced-process-affinity-clear-all".to_owned(),
        FocusTarget::ProcessAffinityInvert => "iced-process-affinity-invert".to_owned(),
        FocusTarget::ProcessAffinityPCores => "iced-process-affinity-p-cores".to_owned(),
        FocusTarget::ProcessAffinityECores => "iced-process-affinity-e-cores".to_owned(),
        FocusTarget::ProcessTreeExpandAll => "iced-process-tree-expand-all".to_owned(),
        FocusTarget::ProcessTreeCollapseAll => "iced-process-tree-collapse-all".to_owned(),
        FocusTarget::ServiceDetailsJumpToProcess => {
            "iced-service-details-jump-to-process".to_owned()
        }
        FocusTarget::StartupOpenLocation => "iced-startup-open-location".to_owned(),
        FocusTarget::PerformanceGraphPoints(pts) => format!("iced-perf-graph-points-{pts}"),
    }
}

fn refresh_request_key(request: taskmanager_application::RefreshRequest) -> &'static str {
    match request {
        taskmanager_application::RefreshRequest::Services => "services",
        taskmanager_application::RefreshRequest::Startup => "startup",
        taskmanager_application::RefreshRequest::Sessions => "sessions",
        _ => "other",
    }
}

fn page_key(page: taskmanager_application::AppPage) -> &'static str {
    match page {
        taskmanager_application::AppPage::Performance => "performance",
        taskmanager_application::AppPage::Applications => "applications",
        taskmanager_application::AppPage::Services => "services",
        taskmanager_application::AppPage::System => "system",
        taskmanager_application::AppPage::Startup => "startup",
        taskmanager_application::AppPage::Users => "users",
        taskmanager_application::AppPage::AppHistory => "app-history",
    }
}

/// Build one renderer-local button that participates in Iced focus traversal.
pub(crate) fn button<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    label: &'static str,
    on_press: Message,
    destructive: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    button_enabled(theme_snapshot, target, label, on_press, destructive, true)
}

/// Build a process-control button whose pointer and keyboard paths share one
/// runtime availability bit. Disabled controls remain visible for layout and
/// discoverability, but cannot publish a request through either input path.
pub(crate) fn button_enabled<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    label: &'static str,
    on_press: Message,
    destructive: bool,
    enabled: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut inner = iced::widget::button(label)
        .style(move |_theme, status| {
            crate::theme::button_style_status(theme_snapshot, destructive, status)
        })
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)]);
    if enabled {
        inner = inner.on_press(on_press.clone());
    }
    FocusableButton::new(
        focus_id(target),
        inner.into(),
        on_press,
        target,
        // The focused shell draws the shared ring token (see
        // `theme::focus_ring_color` for the focus-visible parity gap).
        crate::theme::focus_ring_color(theme_snapshot, destructive),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .with_enabled(enabled)
    .into()
}

/// A focusable button with a runtime label (settings pills, row actions).
pub(crate) fn dynamic_button<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    label: String,
    on_press: Message,
    destructive: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(iced::widget::text(label))
            .style(move |_theme, status| {
                crate::theme::button_style_status(theme_snapshot, destructive, status)
            })
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            // Inner button owns the pointer path (see `button` for why).
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, destructive),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// Owned-theme variant for row widgets retained inside `iced::lazy` bodies.
/// The ordinary [`dynamic_button`] borrows the frame theme, which is correct
/// for regular view trees; a lazy body must own every captured style input for
/// its `'static` widget tree.
pub(crate) fn dynamic_button_owned(
    theme_snapshot: taskmanager_theme::Theme,
    target: FocusTarget,
    label: String,
    on_press: Message,
    destructive: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(iced::widget::text(label))
            .style(move |_theme, status| {
                crate::theme::button_style_status(&theme_snapshot, destructive, status)
            })
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(&theme_snapshot, destructive),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// A quiet focusable toolbar/secondary button (ghost surface style).
pub(crate) fn ghost_button<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    label: &'static str,
    on_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(iced::widget::text(label))
            .style(move |_theme, status| crate::theme::ghost_button_style(theme_snapshot, status))
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            // Inner button owns the pointer path (see `button` for why).
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// A quiet toolbar button with a semantic SVG icon. The icon is rendered by
/// the Iced adapter; the focus and activation path remains the same as the
/// text-only ghost button.
pub(crate) fn ghost_button_with_icon<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    icon_id: IconId,
    label: &'static str,
    on_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let content = iced::widget::row![
        crate::icons::icon(theme_snapshot, icon_id, 14.0),
        iced::widget::text(label),
    ]
    .spacing(f32::from(tokens::SPACE_4));
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(content)
            .style(move |_theme, status| crate::theme::ghost_button_style(theme_snapshot, status))
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// Owned-theme variant used by viewport-lazy Performance rail cards. The
/// lazy body must retain its style snapshot without borrowing the parent view.
pub(crate) fn device_rail_card_owned(
    theme_snapshot: taskmanager_theme::Theme,
    target: FocusTarget,
    content: Element<'static, Message, iced::Theme, iced::Renderer>,
    selected: bool,
    on_press: Message,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(content)
            .padding(8)
            .width(iced::Length::Fill)
            .style(move |_theme, status| {
                crate::theme::device_row_button_style(&theme_snapshot, selected, status)
            })
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(&theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}
/// A focusable selection pill: accent-filled when selected, ghost otherwise.
/// Used by the settings choosers so the active choice reads immediately.
pub(crate) fn choice_pill<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    label: String,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(iced::widget::text(label))
            .style(move |_theme, status| {
                if selected {
                    crate::theme::button_style_status(theme_snapshot, false, status)
                } else {
                    crate::theme::ghost_button_style(theme_snapshot, status)
                }
            })
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            // Inner button owns the pointer path (see `button` for why).
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// A selection pill with a semantic SVG icon, used by the page navigation
/// strip. Settings selectors keep the text-only variant because their values
/// are not semantic icon identities.
pub(crate) fn choice_pill_with_icon<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: FocusTarget,
    icon_id: IconId,
    label: String,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let content = iced::widget::row![
        crate::icons::icon(theme_snapshot, icon_id, 14.0),
        iced::widget::text(label),
    ]
    .spacing(f32::from(tokens::SPACE_4));
    FocusableButton::new(
        focus_id(target),
        iced::widget::button(content)
            .style(move |_theme, status| {
                if selected {
                    crate::theme::button_style_status(theme_snapshot, false, status)
                } else {
                    crate::theme::ghost_button_style(theme_snapshot, status)
                }
            })
            .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
            .on_press(on_press.clone())
            .into(),
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

/// Build a keyboard-reachable table row that additionally opens a row context
/// menu on right-click (the Users row menu, GPUI parity). `on_right_press`
/// fires only for a right-button press over the row bounds.
pub(crate) fn selectable_row_with_menu<'a>(
    theme_snapshot: &taskmanager_theme::Theme,
    page: taskmanager_application::AppPage,
    index: usize,
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
    on_right_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    selectable_row_base(theme_snapshot, page, index, content, Some(on_right_press))
}

/// Build a keyboard-reachable process row that has no actionable live
/// identity. It remains selectable, but deliberately has no context menu:
/// unavailable identity authority must not be replaced with a PID hint.
pub(crate) fn selectable_row<'a>(
    theme_snapshot: &taskmanager_theme::Theme,
    page: taskmanager_application::AppPage,
    index: usize,
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    selectable_row_base(theme_snapshot, page, index, content, None)
}

/// Wrap an arbitrary interactive control (e.g. the Alerts rule-row checkbox
/// cluster) in the focusable shell. The inner control keeps the pointer path
/// (`activate_on_pointer = false`: a click focuses the stop, then the inner
/// widget handles it); the wrapper owns the keyboard Enter/Space path and
/// publishes `on_press` when focused.
pub(crate) fn focusable_control<'a>(
    theme_snapshot: &taskmanager_theme::Theme,
    target: FocusTarget,
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
    on_press: Message,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        focus_id(target),
        content,
        on_press,
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}

fn selectable_row_base<'a>(
    theme_snapshot: &taskmanager_theme::Theme,
    page: taskmanager_application::AppPage,
    index: usize,
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
    on_right_press: Option<Message>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let palette = theme_snapshot.palette();
    let target = FocusTarget::TableRow { page, index };
    FocusableButton::new(
        focus_id(target),
        content,
        Message::SelectRow(index),
        target,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(palette.control_radius),
        true,
    )
    .with_right_press(on_right_press)
    .with_hover(crate::theme_binding::color(palette.hover))
    .into()
}

/// Build a button that participates in Iced's real focus operation. The label
/// resolves through the shared catalog (`common.close`, the same key the
/// applications page's search-close button uses).
pub(crate) fn modal_close<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    FocusableButton::new(
        MODAL_CLOSE_ID,
        iced::widget::button(t("common.close"))
            .style(move |_theme, _status| crate::theme::button_style(theme_snapshot, false))
            .padding(f32::from(tokens::SPACE_8))
            // Inner button owns the pointer path (see `button` for why).
            .on_press(Message::DismissOverlay)
            .into(),
        Message::DismissOverlay,
        FocusTarget::ModalClose,
        crate::theme::focus_ring_color(theme_snapshot, false),
        f32::from(theme_snapshot.palette().control_radius),
        false,
    )
    .into()
}
