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

use crate::palette::{UiPalette, space_2, space_4};
use crate::window::{CustomTextInk, Role, TextRole};
use taskmanager_shell::presentation::container_quota::{ContainerQuotaBreach, ContainerQuotaState};

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
