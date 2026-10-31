//! The declared container quota and the page's over-limit state.
//!
//! A container runtime declares the ceilings it enforces for a workload in
//! the control group that owns it. The shared contract carries those
//! declarations (`ContainerSummary::quota`); this module is the page's ONLY
//! consumer of them, and it never invents a ceiling of its own.
//!
//! Honesty contract:
//! - The only cap a row is compared against is the one the container
//!   DECLARED. Nothing here derives a limit from the host's memory, from the
//!   row's neighbours, or from the observed usage.
//! - "Not declared" is neither "under the limit" nor "over the limit": a
//!   container without a declared ceiling wears no halo and shows no
//!   percentage.
//! - "Unknown" is not "0%": when the platform could not read the
//!   declarations, the row says the quota is unknown.
//! - An overrun stays visible: the observed share is never clamped to 100%.
//!
//! Two declared dimensions can be crossed:
//! - the soft memory ceiling (`memory.high`): the row is over the limit once
//!   the observed share TOUCHES the declared line (the throttling line);
//! - the declared CPU budget (`cpu.max`): the row is over the limit once the
//!   observed load is ABOVE the declared budget (a bandwidth the workload
//!   may fully use). A `cpu.max` that names no budget ("max") declares no
//!   ceiling at all, so it can never produce an over-limit state.

use bevy::color::Alpha;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::TextColor;
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BorderColor, BorderRadius, Node, Overflow, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use taskforest_core::core::process_telemetry::{ContainerQuota, ContainerSummary};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::palette::{UiPalette, space_2, space_4};
use crate::window::{CustomTextInk, Role, TextRole};

/// The typed state of one container's declared quota.
///
/// The default is the UNKNOWN state: a marker written without a payload must
/// never claim that a declaration exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ContainerQuotaState {
    /// The declarations could not be read: the quota is UNKNOWN.
    #[default]
    Unknown,
    /// The container declares no ceiling in any dimension.
    NotDeclared,
    /// The container declares at least one ceiling.
    Declared,
}

/// The declared dimension whose observation crossed the ceiling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContainerQuotaBreach {
    /// The observed memory share reached the declared soft ceiling.
    Memory {
        /// The observed share of the declared soft ceiling, in percent.
        share_percent: f32,
    },
    /// The observed CPU load exceeded the declared CPU budget.
    Cpu {
        /// The observed load, in whole CPUs.
        observed_cpus: f64,
        /// The declared budget, in whole CPUs.
        budget_cpus: f64,
    },
    /// Both declared dimensions crossed their declared ceiling.
    MemoryAndCpu {
        /// The observed share of the declared soft ceiling, in percent.
        share_percent: f32,
        /// The observed load, in whole CPUs.
        observed_cpus: f64,
        /// The declared budget, in whole CPUs.
        budget_cpus: f64,
    },
}

impl ContainerQuotaBreach {
    /// The observed share of the declared soft ceiling this breach crossed.
    ///
    /// `None` for a CPU-only breach: the CPU budget has no share of a
    /// memory ceiling to report.
    pub(crate) fn share_percent(self) -> Option<f32> {
        match self {
            Self::Memory { share_percent } | Self::MemoryAndCpu { share_percent, .. } => {
                Some(share_percent)
            }
            Self::Cpu { .. } => None,
        }
    }
}

/// What the page knows about one container's declared quota.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContainerQuotaView {
    /// The typed state of the declarations.
    pub(crate) state: ContainerQuotaState,
    /// The observed share of the declared soft memory ceiling, in percent.
    ///
    /// `None` when the declarations are unknown, when no soft ceiling is
    /// declared, or when the usage was not observed. A share above 100 is a
    /// real overrun and is never clamped.
    pub(crate) memory_share_percent: Option<f32>,
    /// The declared soft memory ceiling, in bytes.
    pub(crate) memory_soft_limit_bytes: Option<u64>,
    /// The declared hard memory ceiling, in bytes.
    pub(crate) memory_hard_limit_bytes: Option<u64>,
    /// The declared CPU budget, in whole CPUs.
    pub(crate) cpu_budget_cpus: Option<f64>,
    /// The observed CPU load, in whole CPUs.
    pub(crate) cpu_observed_cpus: Option<f64>,
}

impl ContainerQuotaView {
    /// The observed share of the declared soft ceiling, when it TOUCHED the
    /// declared line.
    ///
    /// The soft ceiling is the throttling line: a workload that reaches it is
    /// already over the declared line, so a share that equals 100 counts.
    fn memory_breach(&self) -> Option<f32> {
        self.memory_share_percent.filter(|share| *share >= 100.0)
    }

    /// The observed CPU load and the declared budget, when the load is ABOVE
    /// the declared budget.
    fn cpu_breach(&self) -> Option<(f64, f64)> {
        match (self.cpu_budget_cpus, self.cpu_observed_cpus) {
            (Some(budget), Some(observed)) if observed > budget => Some((observed, budget)),
            _ => None,
        }
    }

    /// The declared dimension the observation crossed, if any.
    pub(crate) fn breach(&self) -> Option<ContainerQuotaBreach> {
        match (self.memory_breach(), self.cpu_breach()) {
            (Some(share_percent), Some((observed_cpus, budget_cpus))) => {
                Some(ContainerQuotaBreach::MemoryAndCpu {
                    share_percent,
                    observed_cpus,
                    budget_cpus,
                })
            }
            (Some(share_percent), None) => Some(ContainerQuotaBreach::Memory { share_percent }),
            (None, Some((observed_cpus, budget_cpus))) => Some(ContainerQuotaBreach::Cpu {
                observed_cpus,
                budget_cpus,
            }),
            (None, None) => None,
        }
    }
}

/// The marker on the pulsing warning halo of an over-limit container row.
///
/// The payload is the OBSERVED fact that crossed the DECLARED ceiling: the
/// raw share, never clamped, so an overrun stays visible as the percentage it
/// actually reached. `None` never means "within the limit": a halo without a
/// breach payload is a marker that was written empty.
#[derive(Clone, Copy, Component, Debug, Default, PartialEq)]
pub(crate) struct ContainerWarningHalo(pub(crate) Option<ContainerQuotaBreach>);

/// The marker on the quota cell: the typed state of the row's declared
/// quota.
#[derive(Clone, Copy, Component, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContainerQuotaCell(pub(crate) ContainerQuotaState);

/// Read the declared quota of one container row.
///
/// The page's only cap is the one the container DECLARED: nothing here
/// derives a ceiling from the host, from the row's neighbours, or from the
/// observed usage.
pub(crate) fn container_quota_view(summary: &ContainerSummary) -> ContainerQuotaView {
    let quota = &summary.quota;
    let observed_memory = summary.memory_bytes.current_value().copied();
    let observed_cpu = summary
        .cpu_percentage
        .current_value()
        .map(|percent| f64::from(*percent) / 100.0);
    ContainerQuotaView {
        state: quota_state(quota),
        memory_share_percent: quota.soft_memory_usage_percent(observed_memory),
        memory_soft_limit_bytes: quota.memory_soft_limit_bytes,
        memory_hard_limit_bytes: quota.memory_hard_limit_bytes,
        cpu_budget_cpus: quota.cpu.and_then(|cpu| cpu.cpus()),
        cpu_observed_cpus: observed_cpu,
    }
}

/// The typed state of the declared quota.
fn quota_state(quota: &ContainerQuota) -> ContainerQuotaState {
    if quota.failure.is_some() {
        ContainerQuotaState::Unknown
    } else if quota.declares_any_ceiling() {
        ContainerQuotaState::Declared
    } else {
        ContainerQuotaState::NotDeclared
    }
}

/// The row's quota readout.
///
/// The readout names the TYPED state first: an unreadable declaration says
/// "unknown" and an absent declaration says "not declared" — neither ever
/// becomes a percentage. A declared ceiling is shown as the observed share
/// against it, and that share is never clamped: an overrun stays visible as
/// the percentage it actually reached. The share keeps one decimal so a
/// reading just below the ceiling never rounds itself up onto it.
pub(crate) fn quota_readout(view: &ContainerQuotaView) -> String {
    match view.state {
        ContainerQuotaState::Unknown => t("proc_insights.unknown").to_owned(),
        ContainerQuotaState::NotDeclared => t("proc_insights.not_declared").to_owned(),
        ContainerQuotaState::Declared => declared_readout(view),
    }
}

/// The readout of a row whose declarations were read.
fn declared_readout(view: &ContainerQuotaView) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(share) = view.memory_share_percent {
        parts.push(format!("{share:.1}%"));
    }
    if let Some(budget) = view.cpu_budget_cpus {
        let observed = view
            .cpu_observed_cpus
            .map(|observed| format!("{observed:.1}"))
            .unwrap_or_else(missing_value);
        parts.push(format!("{observed}/{budget:.1} {}", t("common.cpu")));
    }
    if parts.is_empty() {
        return view
            .memory_soft_limit_bytes
            .or(view.memory_hard_limit_bytes)
            .map(bytes)
            .unwrap_or_else(|| t("proc_insights.not_declared").to_owned());
    }
    parts.join(", ")
}

/// The pulsing warning halo: the over-limit mark of a container row.
///
/// The halo is drawn ONLY for a row whose observed usage crossed a ceiling
/// the container declared. Its stroke follows the observed share and its
/// opacity is a pure function of the shared projection revision, so the halo
/// breathes once per accepted telemetry fold.
pub(crate) fn warning_halo_scene(
    breach: ContainerQuotaBreach,
    revision: u64,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let ink = palette
        .warning_color
        .with_alpha(warning_halo_alpha(revision));
    let stroke = warning_halo_stroke(breach);
    bsn! {
        Node {
            width: rem(palette.icon_rem),
            height: rem(palette.icon_rem),
            flex_shrink: 0.0,
            border: UiRect::all(Val::Px(stroke)),
            border_radius: BorderRadius::all(Val::Percent(50.0)),
        }
        BorderColor { top: { ink }, right: { ink }, bottom: { ink }, left: { ink } }
        ContainerWarningHalo({ Some(breach) })
    }
}

/// The warning halo's stroke weight for one breach.
///
/// The deeper the overrun, the heavier the halo: a share that just touched
/// the declared ceiling keeps the hairline step, a share that ran past twice
/// the ceiling takes the heavier theme step. The exact percentage stays in
/// the row's readout, so the geometry never has to carry the value.
pub(crate) fn warning_halo_stroke(breach: ContainerQuotaBreach) -> f32 {
    match breach.share_percent() {
        Some(share) if share >= 200.0 => space_4(),
        _ => space_2(),
    }
}

/// The halo's opacity for one projection revision: one full breath every four
/// accepted folds. The phase is a pure function of the shared revision, so
/// the halo is reproducible and never invents its own clock.
pub(crate) fn warning_halo_alpha(revision: u64) -> f32 {
    let phase = (revision % 4) as f32 / 4.0;
    let wave = (phase * std::f32::consts::TAU).sin();
    0.65 + 0.35 * wave
}

/// The quota cell: the row's declared-limit readout in its own bounded slot.
pub(crate) fn quota_cell_scene(
    readout: String,
    basis: f32,
    state: ContainerQuotaState,
    breached: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    // A row that crossed a DECLARED ceiling wears the warning ink on its
    // limit readout — the same shape the TUI reference uses. The ink is
    // declared through the window layer's custom-ink seam, so the role
    // observer does not stamp the body ink over it.
    let ink = if breached {
        palette.warning_color
    } else {
        palette.body_color
    };
    bsn! {
        Node {
            flex_basis: percent(basis),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_width: px(0.0),
            align_items: AlignItems::FlexStart,
            overflow: Overflow::clip_x(),
        }
        ContainerQuotaCell({ state })
        Children [
             Text(readout) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } TextColor({ ink }) CustomTextInk
        ]
    }
}
