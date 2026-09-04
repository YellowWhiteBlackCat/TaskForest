//! The confirmation surface: the shell's armed gate rendered as one modal.
//!
//! The shell gate is the whole safety contract — it freezes the target set at
//! arm time, echoes it in the rendered body, and re-emits exactly that frozen
//! set on confirm — so this module only renders one armed gate and routes the
//! two choices back through the shell's typed confirm/dismiss paths. It never
//! re-reads a live row and never builds a request itself (the double-echo
//! authority is `ShellApp`, not a widget).
//!
//! Scope: the kinds this frontend can actually arm (EndTask via the shared
//! Delete chord, ProcessBatch via tree end) render a full dialog with the
//! same `confirm.*` copy the TUI shows. Unreachable kinds fail closed: they
//! produce no dialog here, and the keyboard's `y`/`n` gate vocabulary in the
//! shell remains the only confirm path for them.
//!
//! Open/dismiss paths (keyboard y/n/Enter/Escape live in [`crate::input`];
//! the two buttons here) all converge on the same shell methods, and every
//! transition republishes [`ConfirmationChanged`] so the overlay mounts and
//! despawns from one authority.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, PositionType,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, ConfirmationKind, PendingConfirmation, PlatformEffect};
use taskmanager_shell::presentation::process_batch_action_label;

use crate::app::FrontendTrack;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, space_8, space_16, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

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
}

impl PendingConfirmationView {
    /// Build the view from the shell's pending gate. `None` for kinds this
    /// frontend cannot arm (no dialog; keyboard `y`/`n` stays authoritative).
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
                })
            }
            PendingConfirmation::SessionControl(pending) => {
                let action_label = match pending.action {
                    taskmanager_core::core::session::SessionControlAction::Disconnect => {
                        t("users.disconnect")
                    }
                    taskmanager_core::core::session::SessionControlAction::Lock => t("users.lock"),
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
                })
            }
        }
    }
}

/// Stable token for the session verb inside a target key.
fn service_action_token_session(
    action: taskmanager_core::core::session::SessionControlAction,
) -> &'static str {
    match action {
        taskmanager_core::core::session::SessionControlAction::Disconnect => "disconnect",
        taskmanager_core::core::session::SessionControlAction::Lock => "lock",
    }
}

/// The shared action word for one service verb (the same `svc.*` fold the
/// TUI's action menu uses).
#[must_use]
pub(crate) fn service_action_label(
    action: taskmanager_core::core::services::ServiceAction,
) -> &'static str {
    match action {
        taskmanager_core::core::services::ServiceAction::Start => t("svc.start"),
        taskmanager_core::core::services::ServiceAction::Stop => t("svc.stop"),
        taskmanager_core::core::services::ServiceAction::Restart => t("svc.restart"),
        taskmanager_core::core::services::ServiceAction::Enable => t("svc.enable"),
        taskmanager_core::core::services::ServiceAction::Disable => t("svc.disable"),
    }
}

/// Stable token for the verb inside a target key.
fn service_action_token(action: taskmanager_core::core::services::ServiceAction) -> &'static str {
    match action {
        taskmanager_core::core::services::ServiceAction::Start => "start",
        taskmanager_core::core::services::ServiceAction::Stop => "stop",
        taskmanager_core::core::services::ServiceAction::Restart => "restart",
        taskmanager_core::core::services::ServiceAction::Enable => "enable",
        taskmanager_core::core::services::ServiceAction::Disable => "disable",
    }
}

/// Stable, order-independent key over the frozen target set.
fn frozen_process_key<'a>(
    targets: impl Iterator<Item = &'a taskmanager_core::core::process::FrozenProcessIdentity>,
) -> String {
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
    shell: &mut taskmanager_shell::ShellApp,
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
pub(crate) fn republish(shell: &taskmanager_shell::ShellApp, commands: &mut Commands) {
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
    commands.entity(root).add_one_related::<ChildOf>(overlay);
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
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        ConfirmationOverlay
        ArmedConfirmation({ Some(armed) })
        Children [
            ( { panel } ),
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
    bsn! {
        Node {
            width: px(460.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( Text(body) TextRole(Role::Body) ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    column_gap: Val::Px(space_16()),
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [
                    (
                        Text(confirm)
                        TextRole(Role::Body)
                        ConfirmChoice
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_confirm_activated)
                    ),
                    (
                        Text(cancel)
                        TextRole(Role::Caption)
                        DismissChoice
                        ControlVisual(ControlTone::Surface, false)
                        Button
                        on(on_dismiss_activated)
                    ),
                ]
            ),
        ]
    }
}

/// Register the confirmation observers on the app composition. Called by the
/// window plugin; the input seam triggers [`ConfirmationChanged`].
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_observer(on_confirmation_changed);
}

#[cfg(test)]
#[path = "../tests/headless/confirmation.rs"]
mod tests;
