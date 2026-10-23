//! The confirmation surface: the shell's armed gate rendered as one modal.
//!
//! The shell gate is the whole safety contract — it freezes the target set at
//! arm time, echoes it in the rendered body, and re-emits exactly that frozen
//! set on confirm — so this module only renders one armed gate and routes the
//! two choices back through the shell's typed confirm/dismiss paths. It never
//! re-reads a live row and never builds a request itself (the double-echo
//! authority is `ShellApp`, not a widget).
//!
//! Every current shared gate kind (EndTask, ProcessBatch, ServiceControl,
//! StartupControl, SessionControl, and SmartSelfTest) has a typed dialog view
//! and a matching shell confirmation route. The shell freezes the target and
//! owns the confirm semantics; this renderer neither re-reads live rows nor
//! constructs platform requests.
//!
//! Open/dismiss paths (keyboard y/n/Enter/Escape live in [`crate::input`];
//! the two buttons here) all converge on the same shell methods, and every
//! transition republishes `ConfirmationChanged` so the overlay mounts and
//! despawns from one authority.

use crate::first_run_modal::{FirstRunCommand, SetupState};
use crate::pages::system::diagnostic_modal::DiagnosticCommand;
use crate::system_information_modal::SystemInformationCommand;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, Res, ResMut, SystemParam};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::ComputedNode;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FixedNode, FlexDirection, JustifyContent, Node,
    Overflow, UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::first_run::FirstRunController;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, ConfirmationKind, PendingConfirmation, PlatformEffect};
use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_shell::presentation::process_batch_action_label;

use crate::app::FrontendTrack;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, space_8, space_16, space_24};
use crate::scene_attach::attach_child;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::widgets::layout::MODAL_PANEL_WIDTH_REM;
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessBatchIntent};
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_shell::ShellApp;

/// Renderer-neutral view of one armed gate: the exact copy the dialog shows,
/// resolved once so the scene adapter stays dumb. `kind` routes the typed
/// confirm path; `body` already carries the frozen-target echo (the first
/// half of the double-echo contract — the second half is the shell's frozen
/// intent re-emitted by [`confirm_armed`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingConfirmationView {
    pub(crate) kind: ConfirmationKind,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) confirm_label: String,
    pub(crate) cancel_label: String,
    /// Stable identity of the frozen target set, for tests and semantic
    /// surfaces; never re-derived from live rows after arming.
    pub(crate) target_key: String,
    /// The FROZEN collateral set of a tree action: the affected root and
    /// every descendant below it, with the depth that draws the indentation.
    ///
    /// The rows are frozen with the intent. A later refresh of the live
    /// process table cannot change them, and the surface never re-reads the
    /// live rows to build them.
    pub(crate) collateral: Vec<FrozenCollateralRow>,
}

/// One row of the frozen collateral list: an affected process and its depth
/// below the frozen root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenCollateralRow {
    /// The process name frozen at capture time.
    pub(crate) name: String,
    /// The process id.
    pub(crate) pid: u32,
    /// The nesting depth below the frozen root (`0` is the root itself).
    pub(crate) depth: u32,
}

/// The frozen collateral rows of an armed tree intent.
///
/// The rows come from the intent the shell froze at arming time; a batch
/// that was not frozen from a tree root (a plain selection) has no
/// hierarchy and carries no rows.
fn frozen_collateral_rows(intent: &ProcessBatchIntent) -> Vec<FrozenCollateralRow> {
    intent
        .collateral_rows()
        .iter()
        .map(|member| FrozenCollateralRow {
            name: member.name().to_owned(),
            pid: member.pid(),
            depth: member.depth(),
        })
        .collect()
}

impl PendingConfirmationView {
    /// Build the view from one shared pending gate. Every current gate kind
    /// returns a typed Bevy dialog; callers handle shell absence in the outer
    /// `Option` returned by `pending_confirmation`.
    #[must_use]
    pub(crate) fn from_pending(pending: &PendingConfirmation) -> Option<Self> {
        match pending {
            PendingConfirmation::EndTask(target) => {
                let headline = t("confirm.end_headline")
                    .replace("{name}", &target.name)
                    .replace("{pid}", &target.pid.to_string());
                Some(Self {
                    kind: ConfirmationKind::EndTask,
                    title: t("confirm.process_title").to_owned(),
                    body: format!("{headline}\n{}", t("confirm.recheck_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: target.live_key().map_or_else(
                        || format!("process:pid:{}:unknown", target.pid),
                        |identity| format!("process:{}", identity.stable_key()),
                    ),
                    collateral: Vec::new(),
                })
            }
            PendingConfirmation::ProcessBatch(intent) => {
                let targets = &intent.targets;
                let scope = if targets.len() <= 1 {
                    targets.first().map_or_else(
                        || t("confirm.selected_process").to_owned(),
                        |target| format!("{} ({})", target.name, target.pid),
                    )
                } else {
                    format!("{} {}", targets.len(), t("proc.process_count"))
                };
                let action = process_batch_action_label(intent.action);
                let headline = t("confirm.action_headline")
                    .replace("{action}", &action)
                    .replace("{target}", &scope);
                Some(Self {
                    kind: ConfirmationKind::ProcessBatch,
                    title: t("proc.batch_confirm_title")
                        .replace("{count}", &targets.len().to_string()),
                    body: format!("{headline}\n{}", t("confirm.frozen_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: format!("batch:{}", frozen_process_key(targets.iter())),
                    collateral: frozen_collateral_rows(intent),
                })
            }
            PendingConfirmation::ServiceControl(target) => {
                let headline = t("confirm.action_headline")
                    .replace("{action}", service_action_label(target.action))
                    .replace("{target}", target.service_id.as_str());
                Some(Self {
                    kind: ConfirmationKind::ServiceControl,
                    title: t("confirm.service_title").to_owned(),
                    body: format!("{headline}\n{}", t("confirm.provider_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: format!(
                        "service:{}:{}",
                        target.service_id.as_str(),
                        service_action_token(target.action)
                    ),
                    collateral: Vec::new(),
                })
            }
            PendingConfirmation::StartupControl(request) => {
                let verb = if request.enabled {
                    t("startup.enable")
                } else {
                    t("startup.disable")
                };
                let headline = t("confirm.action_headline")
                    .replace("{action}", verb)
                    .replace("{target}", &request.entry.name);
                Some(Self {
                    kind: ConfirmationKind::StartupControl,
                    title: t("confirm.startup_title").to_owned(),
                    body: format!("{headline}\n{}", t("confirm.provider_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: format!(
                        "startup:{}:{}",
                        request.entry.name,
                        if request.enabled { "enable" } else { "disable" }
                    ),
                    collateral: Vec::new(),
                })
            }
            PendingConfirmation::SessionControl(pending) => {
                let action_label = match pending.action {
                    SessionControlAction::Disconnect => t("users.disconnect"),
                    SessionControlAction::Lock => t("users.lock"),
                };
                let headline = t("confirm.session_headline")
                    .replace("{action}", action_label)
                    .replace("{id}", pending.session.id.as_str())
                    .replace("{user}", &pending.session.user);
                Some(Self {
                    kind: ConfirmationKind::SessionControl,
                    title: t("confirm.session_title").to_owned(),
                    body: format!("{headline}\n{}", t("confirm.provider_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: format!(
                        "session:{}:{}",
                        pending.session.id,
                        service_action_token_session(pending.action)
                    ),
                    collateral: Vec::new(),
                })
            }
            PendingConfirmation::SmartSelfTest(intent) => {
                let headline = format!(
                    "{:?} SMART self-test · {}",
                    intent.kind, intent.display_name
                );
                Some(Self {
                    kind: ConfirmationKind::SmartSelfTest,
                    title: "SMART self-test".to_owned(),
                    body: format!("{headline}\n{}", t("confirm.provider_body")),
                    confirm_label: t("common.confirm").to_owned(),
                    cancel_label: t("common.cancel").to_owned(),
                    target_key: format!(
                        "smart-self-test:{}:{:?}",
                        intent.device_id.as_str(),
                        intent.kind
                    ),
                    collateral: Vec::new(),
                })
            }
        }
    }
}

/// Stable token for the session verb inside a target key.
fn service_action_token_session(action: SessionControlAction) -> &'static str {
    match action {
        SessionControlAction::Disconnect => "disconnect",
        SessionControlAction::Lock => "lock",
    }
}

/// The shared action word for one service verb (the same `svc.*` fold the
/// TUI's action menu uses).
#[must_use]
pub(crate) fn service_action_label(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => t("svc.start"),
        ServiceAction::Stop => t("svc.stop"),
        ServiceAction::Restart => t("svc.restart"),
        ServiceAction::Enable => t("svc.enable"),
        ServiceAction::Disable => t("svc.disable"),
        ServiceAction::ReloadDaemon => t("svc.reload_daemon"),
    }
}

/// Stable token for the verb inside a target key.
fn service_action_token(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => "start",
        ServiceAction::Stop => "stop",
        ServiceAction::Restart => "restart",
        ServiceAction::Enable => "enable",
        ServiceAction::Disable => "disable",
        ServiceAction::ReloadDaemon => "reload-daemon",
    }
}

/// Stable, order-independent key over the frozen target set.
fn frozen_process_key<'a>(targets: impl Iterator<Item = &'a FrozenProcessIdentity>) -> String {
    let mut identities: Vec<String> = targets
        .map(|target| {
            target.live_key().map_or_else(
                || format!("pid:{}:unknown", target.pid),
                |identity| identity.stable_key(),
            )
        })
        .collect();
    identities.sort_unstable();
    identities
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("-")
}

/// Emit the armed gate's frozen request through the shell's typed confirm
/// path. Mirrors the shell's own gate vocabulary branch-for-branch.
pub(crate) fn confirm_armed(
    shell: &mut ShellApp,
    kind: ConfirmationKind,
) -> Option<PlatformEffect> {
    match kind {
        ConfirmationKind::EndTask => shell.confirm_end_task(),
        ConfirmationKind::ProcessBatch => shell.confirm_process_batch(),
        ConfirmationKind::ServiceControl => shell.apply_action(AppAction::ConfirmServiceControl),
        ConfirmationKind::StartupControl => shell.confirm_startup_control(),
        ConfirmationKind::SessionControl => shell.confirm_session_control(),
        ConfirmationKind::SmartSelfTest => shell.confirm_smart_self_test(),
    }
}

/// Publishes the gate transition: `Some(view)` mounts the modal, `None`
/// despawns it. Triggered by the input seam (keyboard) and the button
/// observers below — the only two entry points that mutate the gate.
#[derive(Event)]
pub(crate) struct ConfirmationChanged(pub(crate) Option<PendingConfirmationView>);

/// Republish the gate state from the shell after any choice path ran. Shared
/// with the services action menu (any gate-arming path must republish so the
/// modal mounts from one authority).
pub(crate) fn republish(shell: &ShellApp, commands: &mut Commands) {
    let view = shell
        .pending_confirmation()
        .and_then(PendingConfirmationView::from_pending);
    commands.trigger(ConfirmationChanged(view));
}

/// Marker on the one mounted modal (the bsn! bare-name form needs a
/// `Default` seed, which a unit marker honestly has).
#[derive(Component, Clone, Default)]
pub(crate) struct ConfirmationOverlay;

/// The armed view carried by the mounted overlay. `Option` gives the bsn!
/// template seed an honest `Default` (empty = nothing armed) without
/// inventing a default confirmation.
#[derive(Component, Clone, Default)]
pub(crate) struct ArmedConfirmation(pub(crate) Option<PendingConfirmationView>);

/// Marker on the confirm button.
#[derive(Component, Clone, Default)]
pub(crate) struct ConfirmChoice;

/// Marker on the cancel button.
#[derive(Component, Clone, Default)]
pub(crate) struct DismissChoice;

/// Marker on the frozen collateral list of a tree confirmation.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ConfirmationCollateral;

/// Marker on one row of the frozen collateral list: the nesting depth below
/// the frozen root and the affected process id.
///
/// The fields are positional for the scene's component syntax: the depth,
/// then the process id.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConfirmationCollateralRow {
    /// The nesting depth below the frozen root (`0` is the root itself).
    pub(crate) depth: usize,
    /// The affected process id.
    pub(crate) pid: u32,
}

/// The height of a collateral row, relative to the standard control height.
///
/// The collateral list is a compact list: its rows are denser than the
/// process table's rows so a whole subtree stays readable in one glance.
const COLLATERAL_ROW_SCALE: f32 = 0.8;

/// A capture must show the frozen gate and both usable choices after layout.
#[derive(SystemParam)]
pub(crate) struct ConfirmationCapture<'w, 's> {
    track: Option<NonSend<'w, FrontendTrack>>,
    gates: Query<'w, 's, (&'static ArmedConfirmation, &'static ComputedNode)>,
    confirm: Query<'w, 's, &'static ComputedNode, With<ConfirmChoice>>,
    dismiss: Query<'w, 's, &'static ComputedNode, With<DismissChoice>>,
}
pub(crate) fn capture_ready(access: &ConfirmationCapture) -> bool {
    let pending = access
        .track
        .as_ref()
        .and_then(|track| track.shell.pending_confirmation())
        .and_then(PendingConfirmationView::from_pending);
    let Some(pending) = pending else {
        return false;
    };
    let mounted = access.gates.iter().any(|(armed, node)| {
        armed.0.as_ref() == Some(&pending) && node.size().x > 0.0 && node.size().y > 0.0
    });
    let confirm = access
        .confirm
        .iter()
        .any(|node| node.size().x > 0.0 && node.size().y > 0.0);
    let dismiss = access
        .dismiss
        .iter()
        .any(|node| node.size().x > 0.0 && node.size().y > 0.0);
    mounted && confirm && dismiss
}

/// Observer: mount/despawn the overlay under the app shell root so it stacks
/// above the routed page and survives page remounts.
fn on_confirmation_changed(
    changed: On<ConfirmationChanged>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<ConfirmationOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(view) = changed.event().0.as_ref() else {
        return;
    };
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let overlay = commands
        .spawn_scene(overlay_scene(view, &palette.inner))
        .id();
    attach_child(&mut commands, root, overlay);
}

/// Confirm button: route the frozen request through the shell's typed path.
/// Entity-scoped to the `ConfirmChoice` button by the scene's `on(...)`.
fn on_confirm_activated(
    _activate: On<Activate>,
    armed: Query<&ArmedConfirmation>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let Ok(confirmation) = armed.single() else {
        return;
    };
    let Some(view) = confirmation.0.as_ref() else {
        return;
    };
    let effect = confirm_armed(&mut track.shell, view.kind);
    if let Some(effect) = effect {
        pending.0.push(effect);
    }
    republish(&track.shell, &mut commands);
}

/// Cancel button: dismissal never produces a platform effect — it only clears
/// the armed gate. Entity-scoped to the `DismissChoice` button.
fn on_dismiss_activated(
    _activate: On<Activate>,
    armed: Query<&ArmedConfirmation>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(_confirmation) = armed.single() else {
        return;
    };
    track.shell.dismiss_overlay();
    republish(&track.shell, &mut commands);
}

/// The full-screen modal root: dim scrim over the whole window, panel
/// centered. Absolute positioning + last-child mount keep it above the page.
/// The armed component's field value is set positionally by the bsn! macro.
fn overlay_scene(view: &PendingConfirmationView, palette: &UiPalette) -> impl Scene + use<> {
    let panel = panel_scene(view, palette);
    let scrim = palette.scrim;
    let armed = view.clone();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        // Viewport-anchored modal layer: `FixedNode` makes the overlay a root
        // node — no inherited layout, clipping or transform from the shell —
        // replacing the absolute-position idiom.
        FixedNode
        BackgroundColor({ scrim })
        ConfirmationOverlay
        ArmedConfirmation({ Some(armed) })
        Children [
             @{ panel }
        ]
    }
}

/// The confirmation panel: title, echoed body, and the two typed choice
/// buttons.
fn panel_scene(view: &PendingConfirmationView, palette: &UiPalette) -> impl Scene + use<> {
    let title = view.title.clone();
    let body = view.body.clone();
    let confirm = view.confirm_label.clone();
    let cancel = view.cancel_label.clone();
    let radius = palette.panel_radius_px;
    // The frozen collateral list rides under the echoed body: the danger
    // confirmation shows WHICH processes the action reaches, in the
    // hierarchy that was frozen at arming time. A plain selection batch has
    // no hierarchy, so the list is absent and the flat echo stays.
    let collateral: Option<Box<dyn Scene>> =
        (!view.collateral.is_empty()).then(|| collateral_list_scene(&view.collateral, palette));
    bsn! {
        Node {
            width: rem(MODAL_PANEL_WIDTH_REM),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
             Text(title) TextRole(Role::Heading) --
             Text(body) TextRole(Role::Body) --
             @{ collateral } --

                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    column_gap: Val::Px(space_16()),
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [

                        Text(confirm)
                        TextRole(Role::Body)
                        ConfirmChoice
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_confirm_activated)
                    --

                        Text(cancel)
                        TextRole(Role::Caption)
                        DismissChoice
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_dismiss_activated)

                ]

        ]
    }
}

/// The frozen collateral list: the dense, indented rows of the processes an
/// action on the frozen tree would reach.
///
/// The rows carry the depth the intent froze, so the list reads as the tree
/// it is. The list is ONE whole group: it is admitted with its rows, never
/// half of them.
fn collateral_list_scene(rows: &[FrozenCollateralRow], palette: &UiPalette) -> Box<dyn Scene> {
    let children: Vec<Box<dyn Scene>> = rows
        .iter()
        .map(|row| collateral_row_scene(row, palette))
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(0.0),
            overflow: Overflow::clip_x(),
        }
        ConfirmationCollateral
        Children [{ children }]
    })
}

/// One dense row of the frozen collateral list.
fn collateral_row_scene(row: &FrozenCollateralRow, palette: &UiPalette) -> Box<dyn Scene> {
    // The indentation rides the depth the intent froze: one spacing step
    // per level below the root.
    let indent = space_8() * (row.depth as f32 + 1.0);
    let label = format!("{} · {}", row.name, row.pid);
    let depth = row.depth as usize;
    let pid = row.pid;
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem * COLLATERAL_ROW_SCALE),
            padding: UiRect::left(Val::Px(indent)),
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            overflow: Overflow::clip_x(),
        }
        ConfirmationCollateralRow { depth: { depth }, pid: { pid } }
        Children [
            Text({ label }) TextRole(Role::Caption)
        ]
    })
}

/// Register the confirmation observers on the app composition. Called by the
/// window plugin; the input seam triggers [`ConfirmationChanged`].
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_observer(on_confirmation_changed);
    app.add_systems(bevy::app::Startup, init_capture_confirmation);
}

pub(crate) fn init_capture_confirmation(
    track: Option<NonSend<FrontendTrack>>,
    mut setup: ResMut<SetupState>,
    mut commands: Commands,
) {
    let Some(track) = track else { return };
    if let Some(view) = track
        .shell
        .pending_confirmation()
        .and_then(PendingConfirmationView::from_pending)
    {
        commands.trigger(ConfirmationChanged(Some(view)));
    }
    if let Some(target) = crate::capture::capture_scenario_target() {
        if target == "about" {
            commands.trigger(crate::about_modal::AboutCommand::Open);
        } else if target == "system-about" {
            commands.trigger(SystemInformationCommand::Open);
        } else if target == "first-run" {
            setup.0 = FirstRunController::from_observation(Some(setup_script_info()));
            commands.trigger(FirstRunCommand::Open);
        } else if target == "diagnostic-preview" {
            commands.trigger(DiagnosticCommand::Open);
        } else if target == "diagnostic-failure" {
            commands.trigger(DiagnosticCommand::Failure(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            )));
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/confirmation.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/headless/pages/contract_fed_parity.rs"]
mod contract_fed_parity_tests;
