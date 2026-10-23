//! Systemd unit-type filter capsules.
//!
//! **Ownership**: the unit type is a property of the provider-issued unit
//! id — this module derives it, filters the inventory rows with it, and
//! renders the capsule bar. It never invents a type: an id without a known
//! systemd suffix belongs to [`ServiceUnitType::Other`].
//!
//! **Layout contract**: the capsule bar is ONE horizontally scrolling row
//! that never wraps. In a narrow window the capsules slide sideways instead
//! of folding into a second line, so the action toolbar above keeps its
//! single row and the page budget stays flat. The bar therefore owns a
//! `min_width: 0` scroll viewport and forbids wrapping explicitly.
//!
//! **Capsule names** are systemd unit-type identifiers — platform vocabulary
//! rendered verbatim (like the device acronyms), not translatable prose.

use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, FlexWrap, JustifyContent, Node,
    Overflow, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};

use taskmanager_application::i18n::t;

use crate::pages::services::ServiceRowModel;
use crate::palette::{UiPalette, space_8, space_12};
use crate::scene_attach::attach_child;
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

/// The systemd unit type a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ServiceUnitType {
    /// A `.service` unit.
    Service,
    /// A `.socket` unit.
    Socket,
    /// A `.timer` unit.
    Timer,
    /// A `.target` unit.
    Target,
    /// A `.mount` unit.
    Mount,
    /// A `.scope` unit.
    Scope,
    /// A unit whose type the id does not name.
    Other,
}

impl ServiceUnitType {
    /// The capsule label: the systemd unit-type identifier.
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Socket => "socket",
            Self::Timer => "timer",
            Self::Target => "target",
            Self::Mount => "mount",
            Self::Scope => "scope",
            Self::Other => "other",
        }
    }
}

/// The unit types the filter bar offers, in a stable order.
const UNIT_TYPES: [ServiceUnitType; 7] = [
    ServiceUnitType::Service,
    ServiceUnitType::Socket,
    ServiceUnitType::Timer,
    ServiceUnitType::Target,
    ServiceUnitType::Mount,
    ServiceUnitType::Scope,
    ServiceUnitType::Other,
];

/// One capsule of the unit-type filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum UnitFilterChoice {
    /// Every unit type.
    #[default]
    All,
    /// One concrete unit type.
    Unit(ServiceUnitType),
}

/// The marker on one capsule: the choice it stands for.
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ServiceUnitFilterChoice(pub(crate) UnitFilterChoice);

/// The active unit-type filter of the services list.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceUnitFilter {
    /// The selected capsule.
    pub(crate) choice: UnitFilterChoice,
}

impl ServiceUnitFilter {
    /// Whether one row passes the filter.
    #[must_use]
    pub(crate) fn admits(self, unit_type: ServiceUnitType) -> bool {
        match self.choice {
            UnitFilterChoice::All => true,
            UnitFilterChoice::Unit(selected) => selected == unit_type,
        }
    }
}

/// The unit type of a provider-issued unit id, derived from the systemd
/// unit-name suffix. An id without a known suffix is `Other` — never a
/// guess.
#[must_use]
pub(crate) fn unit_type_of(id: &str) -> ServiceUnitType {
    match id.rsplit_once('.').map(|(_, suffix)| suffix) {
        Some("service") => ServiceUnitType::Service,
        Some("socket") => ServiceUnitType::Socket,
        Some("timer") => ServiceUnitType::Timer,
        Some("target") => ServiceUnitType::Target,
        Some("mount") => ServiceUnitType::Mount,
        Some("scope") => ServiceUnitType::Scope,
        _ => ServiceUnitType::Other,
    }
}

/// Keep only the rows whose unit type passes the filter.
#[must_use]
pub(crate) fn filter_unit_rows(
    rows: Vec<ServiceRowModel>,
    filter: ServiceUnitFilter,
) -> Vec<ServiceRowModel> {
    rows.into_iter()
        .filter(|row| filter.admits(unit_type_of(row.target.as_str())))
        .collect()
}

/// One capsule of the filter bar.
fn capsule_scene(
    choice: UnitFilterChoice,
    label: String,
    active: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let radius = palette.control_radius_px;
    Box::new(bsn! {
        Node {
            height: rem(palette.control_height_rem),
            flex_shrink: 0.0,
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({
            if active { palette.nav_active_bg } else { palette.content_bg }
        })
        ControlVisual(ControlTone::Surface, active)
        Button
        on(on_unit_filter_activated)
        ServiceUnitFilterChoice(choice)
        Children [
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    })
}

/// The filter bar: one horizontally scrolling row that never wraps.
pub(crate) fn unit_filter_scene(filter: ServiceUnitFilter, palette: &UiPalette) -> Box<dyn Scene> {
    let mut capsules: Vec<Box<dyn Scene>> = Vec::new();
    capsules.push(capsule_scene(
        UnitFilterChoice::All,
        t("common.all").to_owned(),
        filter.choice == UnitFilterChoice::All,
        palette,
    ));
    for unit_type in UNIT_TYPES {
        let choice = UnitFilterChoice::Unit(unit_type);
        capsules.push(capsule_scene(
            choice,
            unit_type.label().to_owned(),
            filter.choice == choice,
            palette,
        ));
    }
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene_horizontal(palette);
    let lane_h = crate::widgets::scrollbar::rail_lane_horizontal(UiRect::default());
    Box::new(bsn! {
        // The viewport wrapper owns the scroll area AND the floating rail:
        // the rail is a sibling of the scroll area, so it never scrolls with
        // the content and never takes layout space away from it. This strip
        // scrolls sideways, so its rail rides the bottom edge.
        Node {
            width: percent(100.0),
            min_width: px(0.0),
            flex_direction: FlexDirection::Column,
        }
        Children [
            Node {
                width: percent(100.0),
                min_width: px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::NoWrap,
                column_gap: Val::Px(space_8()),
                overflow: Overflow::scroll_x(),
                padding: { lane_h },
            }
            ScrollArea
            Children [
                { capsules }
            ]
            --
            @{ rail }
        ]
    })
}

/// The page slot that hosts the filter bar.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServiceUnitFilterSlot;

/// The repaint trigger of the filter bar.
#[derive(Event)]
pub(crate) struct UnitFilterRepaintRequired;

/// Capsule tail: select one unit type and repaint the list and the bar.
pub(crate) fn on_unit_filter_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceUnitFilterChoice>,
    mut filter: ResMut<ServiceUnitFilter>,
    mut commands: Commands,
) {
    let Ok(choice) = buttons.get(activate.event().entity) else {
        return;
    };
    if filter.choice == choice.0 {
        return;
    }
    filter.choice = choice.0;
    commands.trigger(UnitFilterRepaintRequired);
    commands.trigger(super::paint::RepaintRequested);
}

/// The filter bar's own painter: the bar is repainted only when its
/// selection changes, so a data refresh never resets the scroll position the
/// user set.
pub(super) fn on_unit_filter_repaint_required(
    _event: On<UnitFilterRepaintRequired>,
    mut dirty: ResMut<PaintState>,
) {
    dirty.0 = true;
}

/// The bar's first mount asks for a paint.
pub(super) fn on_unit_filter_slot_added(
    _event: On<Add<ServiceUnitFilterSlot>>,
    mut dirty: ResMut<PaintState>,
) {
    dirty.0 = true;
}

/// The coalesced paint state of the filter bar.
#[derive(Resource, Default)]
pub(super) struct PaintState(pub(super) bool);

/// Publish the filter bar into its slot.
pub(super) fn paint(
    mut dirty: ResMut<PaintState>,
    filter: Res<ServiceUnitFilter>,
    palette: Res<crate::window::WindowPalette>,
    slots: Query<(Entity, Option<&Children>), With<ServiceUnitFilterSlot>>,
    mut commands: Commands,
) {
    if !dirty.0 {
        return;
    }
    let Some((slot, children)) = slots.iter().next() else {
        return;
    };
    dirty.0 = false;
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(*child).despawn();
        }
    }
    let fresh = commands
        .spawn_scene(unit_filter_scene(*filter, &palette.inner))
        .id();
    attach_child(&mut commands, slot, fresh);
}

/// Register the filter bar's painter and observers.
pub(super) fn register(app: &mut App) {
    app.init_resource::<ServiceUnitFilter>()
        .init_resource::<PaintState>()
        .add_systems(
            PostUpdate,
            paint
                .in_set(crate::widgets::scene_paint::ScenePaint)
                .before(bevy::ui::UiSystems::Prepare),
        )
        .add_observer(on_unit_filter_repaint_required)
        .add_observer(on_unit_filter_slot_added);
}
