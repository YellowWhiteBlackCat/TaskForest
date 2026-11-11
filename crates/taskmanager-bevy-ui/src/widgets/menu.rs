//! Context menu: typed interaction core + themed bsn! render adapter.
//!
//! **Why not the `bevy_ui_widgets` menu primitives** (`MenuButton`/
//! `MenuPopup`/`MenuItem`): that machinery is pointer- and input-focus-driven
//! — popups dismiss on focus loss, open on menu-button clicks, and navigate
//! through `TabGroup` focus, all of which ride the picking/IME queues only a
//! windowed composition registers (see the `FrontendWindowPlugin` note in
//! `crate::window`). This frontend's menu contract is keyboard-first with
//! typed confirm/cancel outcomes that the destructive-action wiring can gate
//! behind the shell's confirmation seam, and it must stay exercisable in the
//! headless composition. The official `Checkbox`/`RadioButton` primitives
//! remain the state vocabulary where they fit (see the pages); the menu is
//! the documented own-composition case.
//!
//! Split per the widget-layer contract: the `MenuState` core is plain data
//! with zero bevy deps (the headless-test surface); `menu_scene_at` is the
//! bsn! adapter themed exclusively through [`crate::palette`] tokens. The
//! widget never talks to the platform and never mutates shell state —
//! activation flows back to the caller as `MenuOutcome`.

use bevy::color::Color;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, BoxShadow, FlexDirection, Node,
    Overflow, ShadowStyle, UiRect, Val, percent, rem, vh,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::ScrollArea;

use crate::glass::{OpticalGlassCard, surface_material, surface_shadow};
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use taskmanager_theme::GlassSurfaceRole;

/// The menu panel's width, in rem — the type ladder's own step.
const MENU_WIDTH_REM: f32 = 13.75;

/// The menu body's height cap, as a share of the window height.
///
/// GPUI's popup caps its scroll body at 360 px and lets the list scroll
/// (`taskmanager-ui/src/overlays/popup.rs`). The Applications menu carries 13
/// verbs, which is taller than a compact window's whole content region; an
/// uncapped card therefore flips above its anchor and clips its first verbs
/// off the top edge. A viewport-relative cap keeps the card inside every
/// supported window size (the compact 720x480 matrix included) and makes the
/// overflow reachable by scrolling instead of unreachable.
const MENU_BODY_MAX_HEIGHT_VH: f32 = 45.0;

/// One menu entry. `enabled == false` renders a dimmed, non-activatable row
/// (honest unavailable state — never hidden, never faked as clickable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuItem {
    pub(crate) label: String,
    pub(crate) enabled: bool,
}

/// A menu's full neutral description: title line plus entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MenuSpec {
    pub(crate) title: String,
    pub(crate) items: Vec<MenuItem>,
}

/// Keyboard input vocabulary the wiring feeds into [`MenuState::advance`].
/// Mirrors the shared terminal bindings (Up/Down move, Enter confirms, Esc
/// cancels) so every frontend's context menu behaves chord-for-chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuInput {
    Up,
    Down,
    Confirm,
    Cancel,
}

/// One activation outcome: the index of the confirmed entry (into the same
/// `MenuSpec::items` slice the state navigates) or an explicit cancel. Pure
/// moves answer `None` — the menu stays open and nothing is authorized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuOutcome {
    Confirmed(usize),
    Canceled,
}

/// Cursor state over an item list. Navigation clamps at both ends (TUI menu
/// parity — `saturating_add_signed` + `min`), never wraps: a destructive
/// action must not skate past the end of the list by accident. A disabled
/// entry can hold the cursor but never confirms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MenuState {
    selection: usize,
}

impl MenuState {
    /// Pure transition: apply one input, answer the activation outcome if
    /// this input completes the interaction. `Confirm` on a disabled or
    /// missing entry answers `None` (the menu stays open; an unavailable
    /// action is never authorized).
    pub(crate) fn advance(&mut self, items: &[MenuItem], input: MenuInput) -> Option<MenuOutcome> {
        match input {
            MenuInput::Up => {
                self.selection = self.selection.saturating_sub(1);
                None
            }
            MenuInput::Down => {
                self.selection = (self.selection + 1).min(items.len().saturating_sub(1));
                None
            }
            MenuInput::Confirm => items
                .get(self.selection)
                .filter(|item| item.enabled)
                .map(|_| MenuOutcome::Confirmed(self.selection)),
            MenuInput::Cancel => Some(MenuOutcome::Canceled),
        }
    }
}

/// Row-fill model: the highlighted row paints the elevated sidebar-card
/// surface (the same two-surface highlight vocabulary the nav rail uses);
/// every other row paints nothing (`Color::NONE`) so the panel fill shows
/// through. Pure so the model is testable without a world.
pub(crate) fn menu_row_background(highlighted: bool, palette: &UiPalette) -> Color {
    if highlighted {
        palette.nav_active_bg
    } else {
        Color::NONE
    }
}

/// The menu's drop shadow: a wide ambient layer plus a tight contact layer.
///
/// A menu floats over a list, so it needs to read as a raised surface. One
/// flat shadow looks like a border; two layers (a wide soft one and a small
/// dark one) read as depth. The offsets ride the rem axis, so the depth
/// scales with the user's reading size.
///
/// GLASS-01: the shared menu role resolves to the floating Card tier.
pub(crate) fn menu_shadow(palette: &UiPalette) -> Vec<ShadowStyle> {
    surface_shadow(GlassSurfaceRole::Menu, palette)
}

/// Full render: panel + title + one row per entry, the cursor row filled by
/// [`menu_row_background`], disabled rows caption-dim. Highlight is fill-only
/// — no marker glyph text, so the text census stays title + one line per
/// entry regardless of cursor position.
pub(crate) fn menu_scene_at(
    spec: &MenuSpec,
    state: &MenuState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = spec.title.clone();
    let radius = palette.panel_radius_rem();
    let row_gap = palette.rem_ratio(space_8() / 4.0);
    let padding = palette.rem_ratio(space_8());
    let rows = menu_rows(spec, state.selection, palette);
    let shadow = menu_shadow(palette);
    let role = GlassSurfaceRole::Menu;
    let tier = role.tier();
    let material = surface_material(role, palette);
    let fill = material.fill;
    let edge = material.edge;
    let grain = material.grain;
    bsn! {
        Node {
            // The menu's width rides the rem axis, so it re-scales with the
            // user's reading size.
            width: rem(MENU_WIDTH_REM),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Rem(row_gap),
            padding: UiRect::all(Val::Rem(padding)),
            border: UiRect::all(Val::Rem(tier.edge_width_rem())),
            border_radius: BorderRadius::all(Val::Rem(radius)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        BoxShadow({ shadow })
        OpticalGlassCard {
            tier: {tier},
            enabled: true,
        }
        Children [
            @{ grain } --
             Text(title) TextRole(Role::Caption) --
            Node {
                width: percent(100),
                max_height: vh(MENU_BODY_MAX_HEIGHT_VH),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
            }
            ScrollArea MenuScrollBody
            Children [
                { rows }
            ]
        ]
    }
}

/// Marker on the menu's scrolling verb list, so a capture can prove the list
/// (not the card) owns the overflow.
#[derive(bevy::ecs::component::Component, Clone, Copy, Debug, Default)]
pub(crate) struct MenuScrollBody;

/// Marker on a rendered menu item row, holding its index in the menu spec.
#[derive(bevy::ecs::component::Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MenuItemIndex(pub(crate) usize);

fn menu_rows(spec: &MenuSpec, selection: usize, palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    spec.items
        .iter()
        .enumerate()
        .map(|(index, item)| menu_row_scene(index, item, index == selection, palette))
        .collect()
}

fn menu_row_scene(
    index: usize,
    item: &MenuItem,
    highlighted: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let label = item.label.clone();
    let radius = palette.control_radius_px;
    let fill = menu_row_background(highlighted, palette);
    // Dim ink for the honest unavailable state; body ink otherwise. The role
    // marker drives the ink — no literal color at the call site.
    let role = if item.enabled {
        Role::Body
    } else {
        Role::Caption
    };
    if item.enabled {
        Box::new(bsn! {
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                align_items: AlignItems::Center,
                padding: UiRect::left(Val::Px(space_8())),
                border_radius: BorderRadius::all(Val::Px(radius)),
            }
            BackgroundColor(fill)
            bevy::ui_widgets::Button
            MenuItemIndex(index)
            Children [
                 Text(label) TextRole(role)
            ]
        }) as Box<dyn Scene>
    } else {
        Box::new(bsn! {
            Node {
                width: percent(100),
                height: rem(palette.control_height_rem),
                align_items: AlignItems::Center,
                padding: UiRect::left(Val::Px(space_8())),
                border_radius: BorderRadius::all(Val::Px(radius)),
            }
            BackgroundColor(fill)
            Children [
                 Text(label) TextRole(role)
            ]
        }) as Box<dyn Scene>
    }
}

#[cfg(test)]
#[path = "../../tests/headless/widgets_interaction.rs"]
mod tests;
