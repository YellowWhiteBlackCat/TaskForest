//! The sessions page's bubble confirmation card.
//!
//! **One authority, one surface.** The card is a *projection of the shared
//! session-control gate*: it reads the frozen target straight from
//! [`ShellApp::pending_session`] and never re-reads a live row, never keeps a
//! local "pending?" flag, and never builds a platform request. Confirming
//! routes through the shared typed confirm path
//! ([`crate::confirmation::confirm_armed`]) and cancelling through the shared
//! dismiss path ([`ShellApp::dismiss_overlay`]) — the same two calls the
//! shared confirmation modal uses.
//!
//! The card is a compact, start-aligned popover under the session action bar
//! (the "bubble" form): it hugs its content, carries the frozen target in the
//! shared `confirm.session_headline` wording, and repeats the shared
//! provider-body note. The destructive verb therefore always names the exact
//! session that is about to be logged out before anything is submitted.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{
    AlignSelf, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::ConfirmationKind;
use taskmanager_application::i18n::t;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_shell::ShellApp;

use crate::app::FrontendTrack;
use crate::input::PendingEffects;
use crate::palette::{UiPalette, space_4, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

use super::paint;

/// The frozen session target the shared gate holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionBubbleTarget {
    /// The frozen verb.
    pub(crate) action: SessionControlAction,
    /// The frozen session id.
    pub(crate) session_id: String,
    /// The frozen session owner.
    pub(crate) user: String,
}

impl SessionBubbleTarget {
    /// The shared label of the frozen verb.
    pub(crate) fn action_label(&self) -> &'static str {
        match self.action {
            SessionControlAction::Disconnect => t("users.disconnect"),
            SessionControlAction::Lock => t("users.lock"),
        }
    }
}

/// The frozen target of the shared session-control gate, or `None` when no
/// session verb is armed. The gate is the only source: a dismissed or
/// confirmed gate leaves nothing to echo.
pub(crate) fn bubble_target(shell: &ShellApp) -> Option<SessionBubbleTarget> {
    let pending = shell.pending_session()?;
    Some(SessionBubbleTarget {
        action: pending.action,
        session_id: pending.session.id.to_string(),
        user: pending.session.user.clone(),
    })
}

/// Marker on the mounted bubble card.
#[derive(Component, Clone, Default)]
pub(crate) struct SessionBubble;

/// Marker on the bubble's confirm choice.
#[derive(Component, Clone, Default)]
pub(crate) struct SessionBubbleConfirm;

/// Marker on the bubble's dismiss choice.
#[derive(Component, Clone, Default)]
pub(crate) struct SessionBubbleDismiss;

/// The bubble card: a severity accent bar beside the frozen-target copy and
/// the two typed choices.
pub(super) fn bubble_scene(
    target: &SessionBubbleTarget,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let headline = t("confirm.session_headline")
        .replace("{action}", target.action_label())
        .replace("{id}", &target.session_id)
        .replace("{user}", &target.user);
    let body = t("confirm.provider_body").to_owned();
    let confirm = t("common.confirm").to_owned();
    let cancel = t("common.cancel").to_owned();
    // The logout verb is destructive; locking a seat is not.
    let accent = match target.action {
        SessionControlAction::Disconnect => palette.danger_color,
        SessionControlAction::Lock => palette.accent,
    };
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: Val::Auto,
            max_width: percent(70.0),
            align_self: AlignSelf::FlexStart,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
        }
        SessionBubble
        Children [
             Node {
                width: Val::Px(space_4()),
                align_self: AlignSelf::Stretch,
                border_radius: BorderRadius::all(Val::Px(space_4())),
             }
             BackgroundColor({ accent })
             --

                Node {
                    flex_grow: 1.0,
                    min_width: px(0.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_4()),
                    padding: UiRect::all(Val::Px(space_12())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor({ palette.panel_fill })
                Children [
                     Text(headline) TextRole(Role::Body) --
                     Text(body) TextRole(Role::Caption) --

                        Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::FlexEnd,
                            column_gap: Val::Px(space_8()),
                        }
                        Children [

                                Text(confirm)
                                TextRole(Role::Body)
                                SessionBubbleConfirm
                                ControlVisual(ControlTone::Surface, false)
                                Button
                                on(on_bubble_confirm_activated)
                            --

                                Text(cancel)
                                TextRole(Role::Caption)
                                SessionBubbleDismiss
                                ControlVisual(ControlTone::Surface, false)
                                Button
                                on(on_bubble_dismiss_activated)

                        ]
                ]
        ]
    }
}

/// Confirm choice: the frozen request leaves through the shared typed
/// confirm path, exactly as the shared confirmation modal does.
fn on_bubble_confirm_activated(
    activate: On<Activate>,
    confirms: Query<&SessionBubbleConfirm>,
    mut track: NonSendMut<FrontendTrack>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    if confirms.get(activate.event().entity).is_err() {
        return;
    }
    let effect =
        crate::confirmation::confirm_armed(&mut track.shell, ConfirmationKind::SessionControl);
    if let Some(effect) = effect {
        pending.0.push(effect);
    }
    commands.trigger(crate::input::ShellInteractionApplied);
    commands.trigger(paint::RepaintRequested);
}

/// Dismiss choice: the shared gate is cleared and nothing is submitted.
fn on_bubble_dismiss_activated(
    activate: On<Activate>,
    dismisses: Query<&SessionBubbleDismiss>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    if dismisses.get(activate.event().entity).is_err() {
        return;
    }
    track.shell.dismiss_overlay();
    commands.trigger(crate::input::ShellInteractionApplied);
    commands.trigger(paint::RepaintRequested);
}

/// Register the bubble's choice observers.
pub(super) fn register(app: &mut bevy::app::App) {
    app.add_observer(on_bubble_confirm_activated);
    app.add_observer(on_bubble_dismiss_activated);
}
