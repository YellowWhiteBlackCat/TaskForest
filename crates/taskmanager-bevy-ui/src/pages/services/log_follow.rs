//! Service log follow lock — the "return to latest" affordance.
//!
//! **Ownership**: the shell owns the open stream and its follow flag; this
//! module owns the VIEW contract around it: whether the panel is pinned to
//! the newest entry, and the floating capsule that appears when it is not.
//!
//! **Behavior**: scrolling the log away from the newest entry releases the
//! lock (the shell's follow flag follows the view, so the panel stops
//! chasing new lines), and scrolling back to the newest entry restores it.
//! The capsule is the explicit way back: one activation re-pins the stream.
//!
//! **One authority**: the pinned state IS the shell's follow flag — the page
//! never keeps a second copy of it. The panel reads the flag when it paints
//! the capsule, and the scroll seam flips the flag through the shell's own
//! toggle entry.
//!
//! **Typed seam**: the scroll observation is a typed event
//! ([`ServiceLogScrolled`]), not a pixel measurement in the widget — the
//! panel's scroll state is observed after layout, and the lock transition is
//! a pure state change a test can drive without a pointer.

use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, JustifyContent, Node, PositionType,
    ScrollPosition, UiRect, Val, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};

use taskmanager_application::i18n::t;

use crate::app::FrontendTrack;
use crate::palette::{UiPalette, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

/// The scroll slack, in logical pixels, that still counts as "at the bottom".
const BOTTOM_SLACK: f32 = 1.0;

/// The observed scroll state of the bounded log list.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct ServiceLogScrollState {
    /// The offset the panel last observed on the list.
    pub(crate) observed_offset: f32,
    /// The offset the panel last pinned the list to.
    pub(crate) pinned_offset: f32,
    /// Whether the list currently rests at its newest entry.
    pub(crate) at_bottom: bool,
}

/// The typed scroll seam: the list reports whether it rests at the newest
/// entry.
#[derive(Event)]
pub(crate) struct ServiceLogScrolled {
    /// `true` when the newest entry is in view.
    pub(crate) at_bottom: bool,
}

/// Marker on the "return to latest" capsule.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServiceLogReturnLatestButton;

/// Scroll tail: the view's lock follows the scroll position. Leaving the
/// newest entry releases the lock (and the shell's follow flag with it);
/// returning to the newest entry restores both.
pub(crate) fn on_log_scrolled(
    scroll: On<ServiceLogScrolled>,
    mut track: NonSendMut<FrontendTrack>,
    mut state: ResMut<ServiceLogScrollState>,
    mut commands: Commands,
) {
    let at_bottom = scroll.event().at_bottom;
    state.at_bottom = at_bottom;
    let following = track
        .shell
        .service_log
        .as_ref()
        .is_some_and(|open| open.feed.follow);
    if following != at_bottom {
        track.shell.toggle_service_log_follow();
    }
    commands.trigger(super::log_panel::LogPanelRepaintRequired);
}

/// The capsule activation: re-pin the stream and return to the newest entry.
pub(crate) fn on_return_latest_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceLogReturnLatestButton>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    if buttons.get(activate.event().entity).is_err() {
        return;
    }
    let following = track
        .shell
        .service_log
        .as_ref()
        .is_some_and(|open| open.feed.follow);
    if !following {
        track.shell.toggle_service_log_follow();
    }
    commands.trigger(super::log_panel::LogPanelRepaintRequired);
}

/// The floating capsule: the explicit way back to the newest entry. It is
/// anchored to the bottom-right of the bounded log list, so it never moves
/// the log content itself.
pub(crate) fn return_latest_capsule_scene(palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            position_type: PositionType::Absolute,
            right: Val::Rem(0.5),
            bottom: Val::Rem(0.5),
        }
        BackgroundColor({ palette.nav_active_bg })
        ControlVisual(ControlTone::Surface, true)
        Button
        on(on_return_latest_activated)
        ServiceLogReturnLatestButton
        Children [
             Text({ t("common.latest").to_owned() })
             TextRole(Role::Caption)
             TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// The scroll observation: after layout, compare the list's scroll offset
/// with its content height and publish the typed scroll seam.
///
/// While the lock is pinned the panel keeps the list at the newest entry, so
/// streaming lines never push the view away. A scroll that leaves the
/// newest entry releases the lock; returning to the newest entry restores
/// it.
pub(crate) fn observe_log_scroll(
    mut track: NonSendMut<FrontendTrack>,
    mut state: ResMut<ServiceLogScrollState>,
    mut lists: Query<
        (&mut ScrollPosition, &ComputedNode),
        With<super::log_panel::LogEntriesViewport>,
    >,
    mut commands: Commands,
) {
    let following = track
        .shell
        .service_log
        .as_ref()
        .is_some_and(|open| open.feed.follow);
    for (mut position, computed) in &mut lists {
        let scale = computed.inverse_scale_factor;
        let visible = computed.size().y * scale;
        let content = computed.content_size().y * scale;
        let max = (content - visible).max(0.0);
        // Only a real scroll move is a reader decision; a frame that merely
        // repaints the list never touches the lock.
        let moved = (position.y - state.observed_offset).abs() > BOTTOM_SLACK;
        state.observed_offset = position.y;
        if moved {
            let at_bottom = max <= BOTTOM_SLACK || position.y >= max - BOTTOM_SLACK;
            if at_bottom != following {
                track.shell.toggle_service_log_follow();
                state.at_bottom = at_bottom;
                commands.trigger(ServiceLogScrolled { at_bottom });
            }
            continue;
        }
        // The list did not move but the content grew: a pinned view follows
        // the newest entry, a released one stays where the reader left it.
        if following && position.y < max {
            position.y = max;
            state.observed_offset = position.y;
            state.pinned_offset = position.y;
        }
    }
}
