//! Process-table column chooser (Applications page): an ANCHORED POPOVER.
//!
//! The chooser is a local choice, so it is a local surface: the trigger
//! button owns it, the menu floats next to that trigger, and the table behind
//! it stays visible and live while the user picks columns. GPUI parity: the
//! action bar's column dropdown; TUI parity: the `C` column menu.
//!
//! Ownership: [`ProcessHiddenColumns`] is the shared state and the popover is
//! a view over it. A toggle re-renders the table through the shared
//! shell-interaction seam; the popover itself only refreshes its check marks
//! in place (no surface rebuild, so an open menu never flickers or loses its
//! anchor).
//!
//! The module keeps its historical path and its state type's name: the shared
//! key seam (`crate::input::dispatch`) and the saved-views seam import them
//! from here, and those call sites live outside this page's seam. The SURFACE
//! is a popover — the scrim modal is gone, not restyled.

use std::collections::HashSet;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{AutoFocus, FocusLost, FocusedInput, InputFocus};
use bevy::picking::events::PointerPress;
use bevy::prelude::Visibility;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::px;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, GlobalZIndex, JustifyContent, Node,
    OverrideClip, PositionType, UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::popover::{Popover, PopoverAlign, PopoverPlacement, PopoverSide};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::{
    ColumnResizePolicy, IconId, OverlayDismissCause, OverlayDismissContext, OverlayKind,
    PROCESS_COLUMNS, ProcessColumnSpec, dismiss_outcome,
};

use crate::input::ShellInteractionApplied;
use crate::palette::{UiPalette, space_4, space_8, space_16};
use crate::scene_attach::attach_child;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::widgets::table::resize::{ColumnWidthNode, ColumnWidthOverrides, live_column_width};
use crate::window::{Role, TextRole, WindowPalette};

/// Stores the user's hidden column IDs for the processes table.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessHiddenColumns(pub(crate) HashSet<String>);

/// One open chooser session: the keyboard focus index plus the anchor the
/// popover is positioned against. `anchor == None` means "the chooser's
/// default trigger" — the keyboard chord opens the menu without a pointer
/// anchor, and the mount resolves the mounted trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessColumnsSession {
    pub(crate) focused_index: usize,
    pub(crate) anchor: Option<Entity>,
}

/// Chooser state resource: `None` when closed, `Some(session)` when open.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessColumnsModalState {
    pub(crate) session: Option<ProcessColumnsSession>,
}

impl ProcessColumnsModalState {
    pub(crate) fn is_open(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn open(&mut self) {
        self.session = Some(ProcessColumnsSession {
            focused_index: 0,
            anchor: None,
        });
    }

    /// Open with an explicit pointer anchor: the popover floats next to the
    /// control the user actually pressed.
    pub(crate) fn open_at(&mut self, anchor: Entity) {
        self.session = Some(ProcessColumnsSession {
            focused_index: 0,
            anchor: Some(anchor),
        });
    }

    pub(crate) fn close(&mut self) {
        self.session = None;
    }
}

/// Fired when the chooser opens or closes (the shared key seam triggers it).
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct ProcessColumnsModalChanged;

/// Fired when the hidden-column set changed: an open popover refreshes its
/// check marks in place.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct ProcessColumnsVisibilityChanged;

/// Marker for the anchored popover surface.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnsPopover;

/// Marker for the trigger control that anchors the popover.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnsTrigger;

/// Component attached to a column item button identifying which column it toggles.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnToggleId(pub(crate) &'static str);

/// Marker on the check badge of one column item: which column it reports.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnCheckMark(pub(crate) &'static str);

/// Marker on the "restore every column" row.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct ProcessColumnsReset;

/// Scene for the "Choose columns" trigger mounted in the processes toolbar.
/// The trigger is the popover's anchor: the menu opens NEXT TO the control
/// the user pressed, and never as a page-blocking overlay.
pub(crate) fn choose_columns_button_scene(palette: &UiPalette) -> impl Scene + use<> {
    let label = t("proc.choose_columns");
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            height: rem(palette.control_height_rem),
            padding: UiRect::axes(Val::Px(space_16()), Val::Px(space_4())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.nav_active_bg })
        Button
        ControlVisual(ControlTone::Surface, false)
        ProcessColumnsTrigger
        crate::tooltip::TooltipText({ label.to_string() })
        on(on_open_columns_modal_activated)
        Children [
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// The check badge of one column item: a semantic icon (never a text
/// codepoint — the tofu law), inside a fixed slot so the labels stay aligned
/// whether the column is shown or hidden. A hidden column's badge is hidden
/// (the slot keeps its size), so toggling a column never shifts the labels.
fn column_check_scene(
    column_id: &'static str,
    shown: bool,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    let badge: Box<dyn Scene> = if shown {
        Box::new(bsn! {
            Node {
                width: rem(palette.icon_rem),
                height: rem(palette.icon_rem),
            }
            crate::icons::IconPlate({ IconId::CircleCheck })
            crate::icons::IconInk({ palette.accent })
            ProcessColumnCheckMark({ column_id })
        })
    } else {
        Box::new(bsn! {
            Node {
                width: rem(palette.icon_rem),
                height: rem(palette.icon_rem),
            }
            crate::icons::IconPlate({ IconId::CircleCheck })
            crate::icons::IconInk({ palette.accent })
            ProcessColumnCheckMark({ column_id })
            Visibility::Hidden
        })
    };
    let badge = vec![badge];
    vec![Box::new(bsn! {
        Node {
            width: rem(palette.icon_rem),
            height: rem(palette.icon_rem),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        Children [
             { badge }
        ]
    }) as Box<dyn Scene>]
}

/// One column row: the check badge plus the column's own word.
fn column_item_scene(spec: &ProcessColumnSpec, shown: bool, palette: &UiPalette) -> Box<dyn Scene> {
    let column_id = spec.id;
    let label = spec.id.to_owned();
    let fill = if shown {
        palette.selection_bg
    } else {
        palette.content_bg
    };
    let check = column_check_scene(column_id, shown, palette);
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ fill })
        Button
        ProcessColumnToggleId({ column_id })
        on(on_column_toggle_activated)
        Children [
             { check } --
             Text(label) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// The "restore every column" row at the foot of the chooser.
fn reset_row_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let label = t("common.reset").to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Button
        ProcessColumnsReset
        on(on_columns_reset_activated)
        Children [
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// Declarative scene for the anchored column chooser. The surface carries
/// Bevy's own popover placement, so the menu is positioned against its
/// anchor and flips above the trigger when the window edge leaves no room
/// below — never a page-sized scrim that blocks the table.
pub(crate) fn columns_popover_scene(
    hidden: &HashSet<String>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let mut items: Vec<Box<dyn Scene>> = PROCESS_COLUMNS
        .iter()
        .filter(|spec| spec.hideable)
        .map(|spec| {
            let shown = !hidden.contains(spec.id);
            column_item_scene(spec, shown, palette)
        })
        .collect();
    items.push(reset_row_scene(palette));
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            width: rem(14.0),
            max_height: rem(22.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: Val::Px(space_8()),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
            overflow: bevy::ui::prelude::Overflow::clip_y(),
        }
        BackgroundColor({ palette.panel_fill })
        OverrideClip
        GlobalZIndex({ 100 })
        ProcessColumnsPopover
        Popover {
            positions: { vec![
                PopoverPlacement { side: PopoverSide::Bottom, align: PopoverAlign::Start, gap: 4.0 },
                PopoverPlacement { side: PopoverSide::Top, align: PopoverAlign::Start, gap: 4.0 },
            ] },
            window_margin: { 8.0_f32 },
        }
        Children [
             { items }
        ]
    }
}

/// The chooser's shared observer inputs, bundled to keep every observer
/// under the argument budget.
#[derive(SystemParam)]
pub(crate) struct ChooserSurface<'w, 's> {
    pub(crate) hidden: Option<Res<'w, ProcessHiddenColumns>>,
    pub(crate) palette: Option<Res<'w, WindowPalette>>,
    pub(crate) triggers: Query<'w, 's, Entity, With<ProcessColumnsTrigger>>,
    pub(crate) popovers: Query<'w, 's, Entity, With<ProcessColumnsPopover>>,
    pub(crate) parents: Query<'w, 's, &'static ChildOf>,
}

/// Observer: the trigger opens the chooser anchored at ITSELF — and closes it
/// again when it is already open (a popover is a toggle, not a latch).
pub(crate) fn on_open_columns_modal_activated(
    activate: On<Activate>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    mut commands: Commands,
) {
    if let Some(ref mut state) = state {
        let entity = activate.event().entity;
        let open_at_trigger = state
            .session
            .as_ref()
            .is_some_and(|session| session.anchor == Some(entity));
        if open_at_trigger {
            state.close();
        } else {
            state.open_at(entity);
        }
        commands.trigger(ProcessColumnsModalChanged);
    }
}

/// Observer: a press OUTSIDE the chooser dismisses it. The chooser is a local
/// surface: it must not stay open over the table while the user works
/// elsewhere. A press on the trigger is the toggle's business, and a press
/// inside the popover is one of its own rows.
fn on_press_outside_the_chooser(
    press: On<PointerPress>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    surface: ChooserSurface,
    mut commands: Commands,
) {
    let Some(state) = state.as_deref_mut() else {
        return;
    };
    if !state.is_open() {
        return;
    }
    if surface.triggers.contains(press.entity) {
        return;
    }
    let mut current = press.entity;
    let mut inside = false;
    for _ in 0..16 {
        if surface.triggers.contains(current) {
            inside = true;
            break;
        }
        let Ok(parent) = surface.parents.get(current) else {
            break;
        };
        current = parent.parent();
    }
    if inside {
        return;
    }
    state.close();
    commands.trigger(ProcessColumnsModalChanged);
}

/// Observer: focus leaving the open chooser closes it and returns focus to
/// the trigger. Intra-popover focus moves keep the surface open.
fn on_chooser_focus_lost(
    lost: On<FocusLost>,
    mut state: Option<ResMut<ProcessColumnsModalState>>,
    focus: Res<InputFocus>,
    surface: ChooserSurface,
    mut commands: Commands,
) {
    let Some(state) = state.as_deref_mut() else {
        return;
    };
    if !state.is_open() {
        return;
    }
    let mut current = lost.entity;
    let mut inside = false;
    for _ in 0..16 {
        if surface.popovers.contains(current) {
            inside = true;
            break;
        }
        let Ok(parent) = surface.parents.get(current) else {
            break;
        };
        current = parent.parent();
    }
    if !inside {
        return;
    }
    if let Some(new_focus) = focus.get() {
        let mut current = new_focus;
        for _ in 0..16 {
            if surface.popovers.contains(current) {
                return;
            }
            let Ok(parent) = surface.parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
    let outcome = dismiss_outcome(
        OverlayKind::DropdownMenu,
        OverlayDismissCause::FocusLoss,
        OverlayDismissContext::single(),
    );
    if !outcome.dismiss {
        return;
    }
    state.close();
    commands.trigger(ProcessColumnsModalChanged);
    if outcome.restore_focus
        && let Some(trigger) = surface.triggers.iter().next()
    {
        commands.entity(trigger).insert(AutoFocus);
    }
}

/// Observer: a column row toggles that column's visibility.
pub(crate) fn on_column_toggle_activated(
    activate: On<Activate>,
    items: Query<&ProcessColumnToggleId>,
    mut hidden: Option<ResMut<ProcessHiddenColumns>>,
    mut commands: Commands,
) {
    let Ok(target) = items
        .get(activate.entity)
        .or_else(|_| items.get(activate.event().entity))
    else {
        return;
    };
    if let Some(ref mut hidden) = hidden {
        let col_id = target.0.to_string();
        if !hidden.0.insert(col_id.clone()) {
            hidden.0.remove(&col_id);
        }
        commands.trigger(ProcessColumnsVisibilityChanged);
        commands.trigger(ShellInteractionApplied);
    }
}

/// Observer: the reset row restores every column.
pub(crate) fn on_columns_reset_activated(
    _activate: On<Activate>,
    mut hidden: Option<ResMut<ProcessHiddenColumns>>,
    mut commands: Commands,
) {
    if let Some(ref mut hidden) = hidden {
        hidden.0.clear();
        commands.trigger(ProcessColumnsVisibilityChanged);
        commands.trigger(ShellInteractionApplied);
    }
}

/// Observer: refresh the check badges of an OPEN chooser after a toggle.
/// The badges are updated in place — the surface is never rebuilt, so the
/// menu keeps its anchor and never flickers.
fn on_visibility_changed(
    _changed: On<ProcessColumnsVisibilityChanged>,
    hidden: Option<Res<ProcessHiddenColumns>>,
    mut marks: Query<(Entity, &ProcessColumnCheckMark, &mut Visibility)>,
) {
    for (entity, mark, mut visibility) in &mut marks {
        let shown = hidden
            .as_ref()
            .is_none_or(|hidden| !hidden.0.contains(mark.0));
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        let _ = entity;
    }
}

/// The chooser surfaces that were hidden this frame and are despawned at the
/// end of it.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct ProcessColumnsPendingDespawn(pub(crate) Vec<Entity>);

/// Flush the hidden surfaces: the despawn runs in `Last`, so every observer
/// of this frame has already run against the live child and a rebuild can
/// never hand one a vanished entity.
fn flush_pending_despawns(
    mut pending: ResMut<ProcessColumnsPendingDespawn>,
    mut commands: Commands,
) {
    for entity in pending.0.drain(..) {
        // `try_despawn` never panics on an entity that is already gone.
        commands.entity(entity).try_despawn();
    }
}

/// Observer: mount or unmount the anchored chooser.
///
/// The surface is a CHILD of its anchor, so the menu travels with the trigger
/// and the table below stays interactive. Closing hides the surface now and
/// despawns it at the end of the frame, so a rebuild never races a vanished
/// child.
fn on_modal_state_changed(
    _changed: On<ProcessColumnsModalChanged>,
    state: Option<ResMut<ProcessColumnsModalState>>,
    surface: ChooserSurface,
    mut pending: ResMut<ProcessColumnsPendingDespawn>,
    mut commands: Commands,
) {
    for entity in &surface.popovers {
        crate::scene_attach::insert_into(&mut commands, entity, Visibility::Hidden);
        pending.0.push(entity);
    }
    let Some(state) = state.as_deref() else {
        return;
    };
    if !state.is_open() {
        return;
    }
    let Some(palette) = surface.palette.as_ref() else {
        return;
    };
    let anchor = state
        .session
        .as_ref()
        .and_then(|session| session.anchor)
        .or_else(|| surface.triggers.iter().next());
    let Some(anchor) = anchor else {
        return;
    };
    let empty_hidden = HashSet::new();
    let hidden_set = surface
        .hidden
        .as_ref()
        .map_or(&empty_hidden, |hidden| &hidden.0);
    let scene = columns_popover_scene(hidden_set, &palette.inner);
    let popover = commands.spawn_scene(scene).id();
    attach_child(&mut commands, anchor, popover);
}

/// Register the chooser state and observers on the app.
/// Observer: `-`/`=` on a focused column row steps that column's width by
/// the shared keyboard step. The popover stays open and the live header/body
/// cells update in place, exactly like a drag commit.
pub(crate) fn on_column_chooser_width_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    state: Option<Res<ProcessColumnsModalState>>,
    toggles: Query<&ProcessColumnToggleId>,
    mut overrides: Option<ResMut<ColumnWidthOverrides>>,
    mut nodes: Query<(&ColumnWidthNode, &mut Node)>,
) {
    if !state.is_some_and(|state| state.is_open())
        || key.input.state != ButtonState::Pressed
        || key.input.repeat
    {
        return;
    }
    let steps = match key.input.key_code {
        KeyCode::Minus | KeyCode::NumpadSubtract => -1,
        KeyCode::Equal | KeyCode::NumpadAdd => 1,
        _ => return,
    };
    let Ok(toggle) = toggles.get(key.focused_entity) else {
        return;
    };
    let Some(overrides) = overrides.as_deref_mut() else {
        return;
    };
    let column = toggle.0;
    let width = ColumnResizePolicy::PROCESS.step(live_column_width(column, overrides), steps);
    overrides.widths.insert(column.to_owned(), width);
    for (marker, mut node) in &mut nodes {
        if marker.0 == column {
            node.width = px(width);
        }
    }
    key.propagate(false);
}

pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<ProcessHiddenColumns>();
    app.init_resource::<ProcessColumnsModalState>();
    app.init_resource::<ProcessColumnsPendingDespawn>();
    app.add_observer(on_modal_state_changed);
    app.add_observer(on_visibility_changed);
    app.add_observer(on_press_outside_the_chooser);
    app.add_observer(on_chooser_focus_lost);
    app.add_observer(on_column_chooser_width_key);
    app.add_systems(bevy::app::Last, flush_pending_despawns);
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/process_columns.rs"]
mod tests;
