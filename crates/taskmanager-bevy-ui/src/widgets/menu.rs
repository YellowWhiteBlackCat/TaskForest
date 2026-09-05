//! Context menu: typed interaction core + themed bsn! render adapter.
//!
//! **Why not the `bevy_ui_widgets` menu primitives** (`MenuButton`/
//! `MenuPopup`/`MenuItem`): that machinery is pointer- and input-focus-driven
//! — popups dismiss on focus loss, open on menu-button clicks, and navigate
//! through `TabGroup` focus, all of which ride the picking/IME queues only a
//! windowed composition registers (see the `FrontendWindowPlugin` note in
//! [`crate::window`]). This frontend's menu contract is keyboard-first with
//! typed confirm/cancel outcomes that the destructive-action wiring can gate
//! behind the shell's confirmation seam, and it must stay exercisable in the
//! headless composition. The official `Checkbox`/`RadioButton` primitives
//! remain the state vocabulary where they fit (see the pages); the menu is
//! the documented own-composition case.
//!
//! Split per the widget-layer contract: the [`MenuState`] core is plain data
//! with zero bevy deps (the headless-test surface); [`menu_scene_at`] is the
//! bsn! adapter themed exclusively through [`crate::palette`] tokens. The
//! widget never talks to the platform and never mutates shell state —
//! activation flows back to the caller as [`MenuOutcome`].

use bevy::color::Color;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_ui_contract::IconId;

use crate::palette::{UiPalette, no_wrap_text, space_8};
use crate::window::{Role, TextRole};

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
    let radius = palette.control_radius_px;
    let rows = menu_rows(spec, state.selection, palette);
    bsn! {
        Node {
            width: px(220.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px({ space_8() / 4.0 }),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Caption) ),
            { rows }
        ]
    }
}

fn menu_rows(spec: &MenuSpec, selection: usize, palette: &UiPalette) -> Vec<impl Scene + use<>> {
    spec.items
        .iter()
        .enumerate()
        .map(|(index, item)| menu_row_scene(item, index == selection, palette))
        .collect()
}

fn menu_row_scene(item: &MenuItem, highlighted: bool, palette: &UiPalette) -> impl Scene + use<> {
    let label = item.label.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let fill = menu_row_background(highlighted, palette);
    // Dim ink for the honest unavailable state; body ink otherwise. The role
    // marker drives the ink — no literal color at the call site.
    let role = if item.enabled {
        Role::Body
    } else {
        Role::Caption
    };
    bsn! {
        Node {
            width: percent(100),
            height: px(height),
            align_items: AlignItems::Center,
            padding: UiRect::left(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(fill)
        Children [
            ( Text(label) TextRole(role) ),
        ]
    }
}

/// Typed presence state for an anchored dropdown menu surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DropdownPresence {
    /// Dropdown popup is closed; only the trigger is interactive.
    #[default]
    Closed,
    /// Dropdown popup is open and anchored below the trigger.
    Open,
}

/// State of an anchored dropdown menu: whether the popup is open, plus the
/// cursor state over the item list when open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct DropdownMenuState {
    pub(crate) presence: DropdownPresence,
    pub(crate) menu_state: MenuState,
}

#[allow(dead_code)]
impl DropdownMenuState {
    pub(crate) fn new(presence: DropdownPresence, selection: usize) -> Self {
        Self {
            presence,
            menu_state: MenuState { selection },
        }
    }

    #[must_use]
    pub(crate) fn is_open(&self) -> bool {
        matches!(self.presence, DropdownPresence::Open)
    }

    pub(crate) fn toggle(&mut self) {
        self.presence = match self.presence {
            DropdownPresence::Open => DropdownPresence::Closed,
            DropdownPresence::Closed => DropdownPresence::Open,
        };
    }

    pub(crate) fn open(&mut self) {
        self.presence = DropdownPresence::Open;
    }

    pub(crate) fn close(&mut self) {
        self.presence = DropdownPresence::Closed;
    }

    /// Advance dropdown navigation or selection:
    /// - If closed: Confirm opens it, other keys are ignored.
    /// - If open: Cancel closes and yields `MenuOutcome::Canceled`;
    ///   Confirm delegates to `menu_state.advance`, closing on success;
    ///   Up / Down delegates navigation without closing.
    pub(crate) fn advance(&mut self, items: &[MenuItem], input: MenuInput) -> Option<MenuOutcome> {
        if !self.is_open() {
            if input == MenuInput::Confirm {
                self.open();
            }
            return None;
        }

        match input {
            MenuInput::Cancel => {
                self.close();
                Some(MenuOutcome::Canceled)
            }
            MenuInput::Confirm => {
                let outcome = self.menu_state.advance(items, input);
                if outcome.is_some() {
                    self.close();
                }
                outcome
            }
            MenuInput::Up | MenuInput::Down => {
                self.menu_state.advance(items, input);
                None
            }
        }
    }
}

/// An anchored dropdown menu attached to a trigger control.
/// Renders a trigger button with chevron affordance and, when open, mounts
/// the anchored popup menu scene directly beneath it.
#[allow(dead_code)]
pub(crate) fn dropdown_menu_scene(
    trigger_label: String,
    spec: &MenuSpec,
    state: &DropdownMenuState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let open = state.is_open();
    let trigger_bg = if open {
        palette.nav_active_bg
    } else {
        palette.content_bg
    };
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let chevron_icon = if open {
        IconId::NavigateUp
    } else {
        IconId::NavigateDown
    };
    let chevron_scene = crate::icons::icon_scene(chevron_icon, 12.0, palette.dim_color);
    let popup_scenes: Vec<Box<dyn Scene>> = if open {
        vec![Box::new(menu_scene_at(spec, &state.menu_state, palette))]
    } else {
        Vec::new()
    };

    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
        }
        Children [
            (
                Node {
                    width: px(220.0),
                    height: px(height),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_8() / 4.0)),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor(trigger_bg)
                Button
                Children [
                    ( Text(trigger_label) TextRole(Role::Body) template_value(no_wrap_text()) ),
                    ( { chevron_scene } ),
                ]
            ),
            { popup_scenes }
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/widgets_interaction.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/widgets/dropdown.rs"]
mod dropdown_tests;
