//! The filesystem alarm rows: the raised alarm wash and the alarm's own mark.
//!
//! Two laws shape this module:
//!
//! - **the mark states the fact**: a read-only mount wears the shared
//!   padlock (`IconId::Lock`) — it is locked, not broken — while a mount
//!   with reported errors keeps the alert triangle. Neither borrows the
//!   other's glyph, and neither is drawn from a font codepoint.
//! - **the wash is a theme token**: the row's fill is the theme's warning
//!   ink at a low alpha, composited over the card's own fill in the theme's
//!   sRGB space. The row therefore reads as a raised tinted surface, not a
//!   translucent veil that greys the card out (the BEVY-183 colour-space
//!   contract). No hand-picked hue ever reaches a scene.

use bevy::color::Color;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, UiRect, Val, percent,
    px, rem,
};
use bevy::ui::widget::Text;
use taskmanager_ui_contract::IconId;

use crate::icons::icon_scene;
use crate::pages::performance::metrics::MountAlert;
use crate::palette::{UiPalette, composite_over, space_4, with_alpha};
use crate::window::{Role, TextRole};

/// The alpha of the alarm wash over the card's fill: a faint tint that lifts
/// the row off the card without hiding the values it carries.
const ALARM_WASH_ALPHA: f32 = 0.16;

/// The raised alarm wash: the theme's warning ink over the card's own fill,
/// composed in the theme's sRGB space.
pub(crate) fn alarm_wash(palette: &UiPalette) -> Color {
    composite_over(
        palette.panel_fill,
        with_alpha(palette.warning_color, ALARM_WASH_ALPHA),
    )
}

/// The semantic mark one alarm wears.
pub(in crate::pages::performance) fn alarm_icon(alert: &MountAlert) -> IconId {
    match alert {
        // A read-only mount is LOCKED: it wears the padlock, never the alert
        // triangle.
        MountAlert::ReadOnly => IconId::Lock,
        // An erroring mount is not locked: it keeps the alarm mark.
        MountAlert::Errors => IconId::Alert,
    }
}

/// One filesystem alarm row: the raised alarm wash, the alarm's own plate,
/// the shared alarm label and the mount point.
pub(in crate::pages::performance) fn mount_alert_scene(
    alert: &MountAlert,
    mount_point: &str,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let label = alert.label();
    let mount = mount_point.to_owned();
    let ink = palette.warning_color;
    let wash = alarm_wash(palette);
    let plate = icon_scene(alarm_icon(alert), palette, ink);
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ wash })
        Children [
            Node { width: rem(palette.icon_rem), height: rem(palette.icon_rem), flex_shrink: 0.0 }
            Children [ @{ plate } ] --
            Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
            Node { flex_grow: 1.0, min_width: px(0.0), overflow: Overflow::clip_x() }
            Children [ Text(mount) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap } ]
        ]
    })
}
