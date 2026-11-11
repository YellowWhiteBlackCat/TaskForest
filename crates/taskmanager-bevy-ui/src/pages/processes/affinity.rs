//! Process CPU affinity editor modal overlay (Applications page).
//!
//! Ownership line: the shell owns `process_affinity_state` and the requests
//! `request_process_affinity` / `request_process_affinity_control_for`;
//! this module owns the Bevy-local modal surface, the multi-core selection grid,
//! the Apply and Cancel controls, and the event/repaint hooks.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::BorderColor;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, BoxShadow, FlexDirection, JustifyContent, Node,
    Overflow, PositionType, UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::process::FrozenProcessIdentity;
use taskmanager_application::i18n::control_error_detail;
use taskmanager_application::i18n::t;
use taskmanager_application::{PlatformEffect, ProcessAffinityState};
use taskmanager_shell::ShellApp;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::glass::{OpticalGlassCard, noise_overlay_scene};
use crate::input::PendingEffects;
use crate::palette::{UiPalette, space_8, space_12, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::widgets::layout::MODAL_PANEL_WIDTH_REM;
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};
use bevy::text::{LineBreak, TextLayout};
use taskmanager_application::ProcessAffinityRequest;
use taskmanager_theme::GlassSurfaceRole;

/// The CPU button's width, in rem: the widest button label ("CPU 63") plus
/// its air, so every core in the grid is the same size.
const AFFINITY_CPU_BUTTON_WIDTH_REM: f32 = 5.5;

/// The CPU grid's height, in rem: the visible budget of the wrapping core
/// grid before it starts scrolling.
const AFFINITY_CPU_GRID_HEIGHT_REM: f32 = 12.5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AffinitySession {
    pub(crate) target: FrozenProcessIdentity,
    pub(crate) logical_cpu_count: usize,
    pub(crate) selected_mask: Vec<u32>,
    /// Whether a correlated read has ever seeded `selected_mask`. Until it
    /// does, the grid paints nothing selected and the toggles keep the mask
    /// unchanged: an unobserved mask is never fabricated into an editable
    /// default (the Iced editor and the TUI modal fail closed the same way).
    pub(crate) mask_observed: bool,
    /// The shared reason the latest read for `target` failed, if it did. The
    /// status line paints it through the owner's resolver.
    pub(crate) failure: Option<FailureKind>,
}

#[derive(Resource, Default)]
pub(crate) struct ProcessAffinityModalState {
    pub(crate) session: Option<AffinitySession>,
}

impl ProcessAffinityModalState {
    pub(crate) fn open(&mut self, target: FrozenProcessIdentity, logical_cpu_count: usize) {
        let count = logical_cpu_count.max(1);
        self.session = Some(AffinitySession {
            target,
            logical_cpu_count: count,
            selected_mask: Vec::new(),
            mask_observed: false,
            failure: None,
        });
    }

    pub(crate) fn close(&mut self) {
        self.session = None;
    }

    pub(crate) fn toggle_cpu(&mut self, cpu: u32) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !session.is_editable() {
            return;
        }
        if let Some(pos) = session.selected_mask.iter().position(|&c| c == cpu) {
            if session.selected_mask.len() > 1 {
                session.selected_mask.remove(pos);
            }
        } else {
            session.selected_mask.push(cpu);
            session.selected_mask.sort_unstable();
        }
    }

    pub(crate) fn toggle_all(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !session.is_editable() {
            return;
        }
        if session.selected_mask.len() == session.logical_cpu_count {
            session.selected_mask = vec![0];
        } else {
            session.selected_mask = (0..session.logical_cpu_count as u32).collect();
        }
    }

    pub(crate) fn apply(&mut self, shell: &mut ShellApp) -> Option<PlatformEffect> {
        if !self
            .session
            .as_ref()
            .is_some_and(AffinitySession::is_editable)
        {
            return None;
        }
        let session = self.session.take()?;
        shell.request_process_affinity_control_for(session.target, session.selected_mask)
    }
}

impl AffinitySession {
    /// Whether the displayed mask may be edited or submitted: only an observed
    /// mask whose latest read did not fail. A failed read leaves the last-good
    /// mask visible for context but never submits it as current.
    #[must_use]
    pub(crate) fn is_editable(&self) -> bool {
        self.mask_observed && self.failure.is_none()
    }

    /// Fold one shared read outcome into the editor and report whether
    /// anything it paints changed.
    ///
    /// The outcome must be about this session's frozen identity (the caller
    /// checks `ProcessAffinityState::target`), so a recycled pid or another
    /// process's read can never rewrite an open editor (ADR-027). A failed
    /// read drops the mask — the current one is unknown — and carries the
    /// shared reason; an authoritative read seeds the grid once and never
    /// clobbers the user's in-progress edits.
    pub(crate) fn absorb_outcome(&mut self, affinity: &ProcessAffinityState) -> bool {
        let failure = affinity.failure_for_target(&self.target);
        let mut changed = self.failure != failure;
        self.failure = failure;
        if failure.is_some() {
            if self.mask_observed || !self.selected_mask.is_empty() {
                self.mask_observed = false;
                self.selected_mask.clear();
                changed = true;
            }
        } else if !self.mask_observed
            && let Some(cpus) = affinity
                .last_good_for_target(&self.target)
                .map(|ready| ready.cpus.clone())
        {
            self.selected_mask = cpus;
            self.mask_observed = true;
            changed = true;
        }
        changed
    }
}

// ---- markers and events ---------------------------------------------------

#[derive(Component, Clone, Default)]
pub(crate) struct ProcessAffinityOverlay;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AffinityCpuButton(pub(crate) u32);

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityToggleAllButton;

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityApplyButton;

#[derive(Component, Clone, Copy, Default)]
pub(crate) struct AffinityCancelButton;

#[derive(Event)]
pub(crate) struct ProcessAffinityRequested(pub(crate) FrozenProcessIdentity, pub(crate) usize);

#[derive(Event)]
pub(crate) struct ProcessAffinityRepaintRequired;

// ---- scenes ---------------------------------------------------------------

pub(crate) fn affinity_modal_scene(
    session: &AffinitySession,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let title = format!("{} · {}", t("dialog.cpu_affinity"), session.target.name);
    // The status slot reads the shared read outcome: a failed read shows the
    // owner's reason (never a Debug dump), an unobserved mask says so instead
    // of being reported as a count, and only an observed mask is summarized.
    let status = if let Some(failure) = session.failure {
        control_error_detail(failure).to_owned()
    } else if session.mask_observed {
        format!(
            "{}/{} {}",
            session.selected_mask.len(),
            session.logical_cpu_count,
            t("proc.logical_cpus")
        )
    } else {
        t("common.collecting_telemetry").to_owned()
    };
    let subtitle = format!("PID: {} · {status}", session.target.pid);

    let mut cpu_buttons: Vec<Box<dyn Scene>> = Vec::with_capacity(session.logical_cpu_count);
    for cpu in 0..session.logical_cpu_count as u32 {
        let selected = session.selected_mask.contains(&cpu);
        let label = format!("CPU {cpu}");
        let fill = if selected {
            palette.nav_active_bg
        } else {
            palette.content_bg
        };

        cpu_buttons.push(Box::new(bsn! {

                Node {
                    width: rem(AFFINITY_CPU_BUTTON_WIDTH_REM),
                    height: rem(palette.control_height_rem),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor(fill)
                ControlVisual(ControlTone::Surface, selected)
                Button
                on(on_cpu_button_activated)
                AffinityCpuButton(cpu)
                Children [
                     Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
                ]

        }) as Box<dyn Scene>);
    }

    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::default());
    let grid = Box::new(bsn! {
        // The viewport wrapper owns the scroll area AND the floating rail:
        // the rail is a sibling of the scroll area, so it never scrolls with
        // the content and never takes layout space away from it.
        Node {
            width: percent(100.0),
            height: rem(AFFINITY_CPU_GRID_HEIGHT_REM),
            flex_direction: FlexDirection::Column,
        }
        Children [
            Node {
                width: percent(100.0),
                height: rem(AFFINITY_CPU_GRID_HEIGHT_REM),
                flex_direction: FlexDirection::Row,
                flex_wrap: bevy::ui::FlexWrap::Wrap,
                column_gap: Val::Px(space_8()),
                row_gap: Val::Px(space_8()),
                overflow: Overflow::scroll_y(),
                padding: { lane },
            }
            ScrollArea
            Children [
                { cpu_buttons }
            ]
            --
            @{ rail }
        ]
    }) as Box<dyn Scene>;

    let radius_rem = palette.panel_radius_rem();
    let role = GlassSurfaceRole::Dialog;
    let tier = role.tier();
    let grain = Box::new(noise_overlay_scene(tier, palette, radius_rem)) as Box<dyn Scene>;
    let modal_fill = palette.glass_fill(tier);
    let modal_edge = palette.glass_edge(tier);
    let modal_shadow = crate::glass::surface_shadow(role, palette);
    let panel = Box::new(bsn! {
        Node {
            width: rem(MODAL_PANEL_WIDTH_REM),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            border: UiRect::all(Val::Rem(tier.edge_width_rem())),
        }
        BackgroundColor({ modal_fill })
        BorderColor::all(modal_edge)
        BoxShadow({ modal_shadow })
        OpticalGlassCard {
            tier: {tier},
            enabled: true,
        }
        Children [
             @{ grain } --
             Text(title) TextRole(Role::Heading) --
             Text(subtitle) TextRole(Role::Caption) --
             @{ grid } --

                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    column_gap: Val::Px(space_12()),
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [

                        Text({ t("common.all").to_owned() })
                        TextRole(Role::Caption)
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_toggle_all_activated)
                        AffinityToggleAllButton
                    --

                        Text({ t("common.apply").to_owned() })
                        TextRole(Role::Body)
                        ControlVisual(ControlTone::Surface, true)
                        Button
                        on(on_apply_activated)
                        AffinityApplyButton
                    --

                        Text({ t("common.cancel").to_owned() })
                        TextRole(Role::Caption)
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_cancel_activated)
                        AffinityCancelButton

                ]

        ]
    }) as Box<dyn Scene>;

    let scrim = palette.glass_scrim_fill;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        ProcessAffinityOverlay
        Children [
             @{ panel }
        ]
    }) as Box<dyn Scene>
}

// ---- observers ------------------------------------------------------------

pub(crate) fn on_cpu_button_activated(
    activate: On<Activate>,
    buttons: Query<&AffinityCpuButton>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    state.toggle_cpu(button.0);
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_toggle_all_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    state.toggle_all();
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_apply_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    if let Some(effect) = state.apply(&mut track.shell) {
        pending.0.push(effect);
    }
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_cancel_activated(
    _activate: On<Activate>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    state.close();
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_affinity_requested(
    request: On<ProcessAffinityRequested>,
    mut state: ResMut<ProcessAffinityModalState>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let target = request.event().0.clone();
    let cpu_count = request.event().1;
    state.open(target.clone(), cpu_count);
    pending
        .0
        .push(PlatformEffect::ProcessAffinity(ProcessAffinityRequest {
            target,
        }));
    commands.trigger(ProcessAffinityRepaintRequired);
}

pub(crate) fn on_affinity_fold_sync(
    _fold: On<ShellProjectionFolded>,
    track: crate::app::ShellTrack,
    mut state: ResMut<ProcessAffinityModalState>,
    mut commands: Commands,
) {
    let Some(session) = state.session.as_mut() else {
        return;
    };
    // Only an outcome for the identity captured at open time may rewrite this
    // editor; the fold itself is the session's own `absorb_outcome`.
    let affinity = track.shell().process_affinity_state();
    if affinity.target() != Some(&session.target) {
        return;
    }
    if session.absorb_outcome(affinity) {
        commands.trigger(ProcessAffinityRepaintRequired);
    }
}

pub(crate) fn on_affinity_repaint_required(
    _event: On<ProcessAffinityRepaintRequired>,
    mut dirty: ResMut<paint::PaintState>,
) {
    dirty.0 = true;
}

pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<ProcessAffinityModalState>();
    paint::register(app);
    app.add_observer(on_affinity_requested);
    app.add_observer(on_affinity_repaint_required);
    app.add_observer(on_affinity_fold_sync);
}

mod paint;
