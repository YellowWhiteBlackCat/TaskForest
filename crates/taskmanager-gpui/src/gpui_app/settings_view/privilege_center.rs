//! Central authorization surface for optional hardware facts.
//!
//! Performance and System pages only render accepted observations. This
//! module is the single GPUI entry point for the four user-initiated helper
//! lanes that can make those observations available. Each row still submits
//! one typed capability request; the center groups the controls without
//! turning them into a blanket privileged process.

use gpui::{Div, Entity, InteractiveElement, ParentElement, Styled, div};
use taskmanager_application::{
    GpuEngineRowsState, MsrReadoutRequestFailure, MsrReadoutState, RaplPowerRequestFailure,
    RaplPowerState, SmbiosMemoryRequestFailure, SmbiosMemoryState, i18n,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_platform_contract::CapabilityStatus;
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_theme::{Theme, tokens};

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;

/// Immutable render inputs for the central authorization surface.
pub(crate) struct PrivilegeCenterInputs<'a> {
    pub(crate) gpu_engine_state: &'a GpuEngineRowsState,
    pub(crate) gpu_engine_capability: Option<CapabilityStatus>,
    pub(crate) gpu_engine_device_id: Option<DeviceId>,
    pub(crate) gpu_engine_index: Option<usize>,
    pub(crate) smbios_state: &'a SmbiosMemoryState,
    pub(crate) smbios_capability: Option<CapabilityStatus>,
    pub(crate) rapl_state: &'a RaplPowerState,
    pub(crate) rapl_capability: Option<CapabilityStatus>,
    pub(crate) msr_state: &'a MsrReadoutState,
    pub(crate) msr_capability: Option<CapabilityStatus>,
}

#[derive(Clone, Copy)]
enum PrivilegeAction {
    GpuEngines(usize),
    SmbiosMemory,
    RaplPower,
    MsrReadouts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivilegeRowState {
    NeedsAuthorization,
    Authorizing,
    Enabled,
    Denied,
    Unavailable,
    Unsupported,
    Failed,
}

impl PrivilegeRowState {
    const fn label_key(self) -> &'static str {
        match self {
            Self::NeedsAuthorization => "settings.privileges_authorize_hint",
            Self::Authorizing => "settings.privileges_authorizing",
            Self::Enabled => "settings.privileges_enabled",
            Self::Denied => "settings.privileges_denied",
            Self::Unavailable => "settings.privileges_unavailable",
            Self::Unsupported => "settings.privileges_unsupported",
            Self::Failed => "settings.privileges_failed",
        }
    }

    const fn has_action(self) -> bool {
        matches!(self, Self::NeedsAuthorization | Self::Denied)
    }
}

struct PrivilegeRow {
    id: &'static str,
    label_key: &'static str,
    state: PrivilegeRowState,
    action: PrivilegeAction,
}

/// Render the central permission group. A capability absent from the runtime
/// catalog is omitted; an unsupported or missing helper remains visible as a
/// typed status so the user can distinguish "not installed" from "not yet
/// authorized" without any page-local placeholder.
pub(crate) fn render_privilege_center(
    theme: &Theme,
    inputs: &PrivilegeCenterInputs<'_>,
    entity: Entity<RootView>,
) -> Option<Div> {
    let mut rows = Vec::new();
    if let (Some(_device_id), Some(index), Some(state)) = (
        inputs.gpu_engine_device_id.as_ref(),
        inputs.gpu_engine_index,
        gpu_engine_state(inputs),
    ) {
        rows.push(PrivilegeRow {
            id: "gpu-engines",
            label_key: "gpu.per_engine_title",
            state,
            action: PrivilegeAction::GpuEngines(index),
        });
    }
    if let Some(state) = smbios_state(inputs.smbios_state, inputs.smbios_capability) {
        rows.push(PrivilegeRow {
            id: "smbios-memory",
            label_key: "system.memory_inventory",
            state,
            action: PrivilegeAction::SmbiosMemory,
        });
    }
    if let Some(state) = rapl_state(inputs.rapl_state, inputs.rapl_capability) {
        rows.push(PrivilegeRow {
            id: "rapl-power",
            label_key: "cpu.package_power",
            state,
            action: PrivilegeAction::RaplPower,
        });
    }
    if let Some(state) = msr_state(inputs.msr_state, inputs.msr_capability) {
        rows.push(PrivilegeRow {
            id: "msr-readouts",
            label_key: "cpu.msr_readouts",
            state,
            action: PrivilegeAction::MsrReadouts,
        });
    }
    if rows.is_empty() {
        return None;
    }

    let mut panel = div()
        .debug_selector(|| "tm-settings-privilege-center".to_string())
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(theme),
        ))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_card_bg))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .w_full()
        .min_w(gpui::px(0.0))
        .child(
            div()
                .flex_1()
                .min_w(gpui::px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(i18n::t(row.label_key)),
        )
        .child(
            div()
                .flex_none()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(
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
    if state.has_action() {
        line = line.child(authorization_button(theme, row.id, entity, row.action));
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
                entity.update(cx, |view, cx| match action {
                    PrivilegeAction::GpuEngines(index) => view.enable_gpu_engines(index, cx),
                    PrivilegeAction::SmbiosMemory => view.authorize_memory_inventory(cx),
                    PrivilegeAction::RaplPower => view.authorize_package_power(cx),
                    PrivilegeAction::MsrReadouts => view.authorize_msr_readouts(cx),
                });
            },
            |_hovered, _window, _cx| {},
        ))
}

fn gpu_engine_state(inputs: &PrivilegeCenterInputs<'_>) -> Option<PrivilegeRowState> {
    let device_id = inputs.gpu_engine_device_id.as_ref()?;
    match present_gpu_engine_rows(
        inputs.gpu_engine_state,
        device_id,
        inputs.gpu_engine_capability,
    ) {
        GpuEngineRowsPresentation::PermissionRequired => {
            Some(PrivilegeRowState::NeedsAuthorization)
        }
        GpuEngineRowsPresentation::Loading => Some(PrivilegeRowState::Authorizing),
        GpuEngineRowsPresentation::Active(_) => Some(PrivilegeRowState::Enabled),
        GpuEngineRowsPresentation::PermissionDenied => Some(PrivilegeRowState::Denied),
        GpuEngineRowsPresentation::MissingDependency
        | GpuEngineRowsPresentation::AuthorizationUnavailable => {
            Some(PrivilegeRowState::Unavailable)
        }
        GpuEngineRowsPresentation::Unsupported => Some(PrivilegeRowState::Unsupported),
        GpuEngineRowsPresentation::Failed => Some(PrivilegeRowState::Failed),
    }
}

fn smbios_state(
    state: &SmbiosMemoryState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        SmbiosMemoryState::Ready(_) => Some(PrivilegeRowState::Enabled),
        SmbiosMemoryState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        SmbiosMemoryState::Failed(failed) => {
            Some(state_from_failure(smbios_failure_kind(&failed.failure)))
        }
        SmbiosMemoryState::Closed => capability_state(capability),
    }
}

fn rapl_state(
    state: &RaplPowerState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        RaplPowerState::Ready(_) => Some(PrivilegeRowState::Enabled),
        RaplPowerState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        RaplPowerState::Failed(failed) => {
            Some(state_from_failure(rapl_failure_kind(&failed.failure)))
        }
        RaplPowerState::Closed => capability_state(capability),
    }
}

fn msr_state(
    state: &MsrReadoutState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        MsrReadoutState::Ready(_) => Some(PrivilegeRowState::Enabled),
        MsrReadoutState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        MsrReadoutState::Failed(failed) => {
            Some(state_from_failure(msr_failure_kind(&failed.failure)))
        }
        MsrReadoutState::Closed => capability_state(capability),
    }
}

fn capability_state(status: Option<CapabilityStatus>) -> Option<PrivilegeRowState> {
    match status? {
        CapabilityStatus::Available | CapabilityStatus::PermissionRequired => {
            Some(PrivilegeRowState::NeedsAuthorization)
        }
        CapabilityStatus::Degraded(kind) => Some(state_from_failure(kind)),
        CapabilityStatus::Unsupported => Some(PrivilegeRowState::Unsupported),
        CapabilityStatus::MissingDependency
        | CapabilityStatus::TemporarilyUnavailable
        | CapabilityStatus::Stale => Some(PrivilegeRowState::Unavailable),
    }
}

const fn state_from_failure(kind: FailureKind) -> PrivilegeRowState {
    match kind {
        FailureKind::RequiresEscalation => PrivilegeRowState::NeedsAuthorization,
        FailureKind::PermissionDenied => PrivilegeRowState::Denied,
        FailureKind::Unsupported => PrivilegeRowState::Unsupported,
        FailureKind::ProviderFault | FailureKind::Rejected => PrivilegeRowState::Failed,
        FailureKind::MissingDependency
        | FailureKind::TimedOut
        | FailureKind::TemporarilyUnavailable
        | FailureKind::IdentityChanged => PrivilegeRowState::Unavailable,
    }
}

const fn smbios_failure_kind(failure: &SmbiosMemoryRequestFailure) -> FailureKind {
    match failure {
        SmbiosMemoryRequestFailure::Submission(kind) => *kind,
        SmbiosMemoryRequestFailure::Provider(failure) => failure.kind,
    }
}

const fn rapl_failure_kind(failure: &RaplPowerRequestFailure) -> FailureKind {
    match failure {
        RaplPowerRequestFailure::Submission(kind) => *kind,
        RaplPowerRequestFailure::Provider(failure) => failure.kind,
    }
}

const fn msr_failure_kind(failure: &MsrReadoutRequestFailure) -> FailureKind {
    match failure {
        MsrReadoutRequestFailure::Submission(kind) => *kind,
        MsrReadoutRequestFailure::Provider(failure) => failure.kind,
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_settings_privilege_center_tests.rs"]
mod tests;
