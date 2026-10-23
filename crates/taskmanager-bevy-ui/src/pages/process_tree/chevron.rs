//! The disclosure chevron and its rotation tween (BEVY-126).
//!
//! One semantic arrow plate serves every expandable row. The plate rests at
//! the collapsed angle (pointing right) and turns a quarter turn to the
//! expanded angle (pointing down) over a short tween, so the affordance reads
//! as a motion instead of a jump. The tween state lives in a resource keyed
//! by the row's semantic key, so a row that is remounted mid-turn keeps
//! turning instead of snapping back.

use std::collections::HashMap;

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::math::Rot2;
use bevy::picking::Pickable;
use bevy::prelude::Visibility;
use bevy::scene::{Scene, bsn};
use bevy::time::Time;
use bevy::ui::prelude::{AlignSelf, Node, UiTransform, rem};

use taskmanager_ui_contract::IconId;

use crate::palette::UiPalette;

/// The tween length of the disclosure turn, in seconds.
const CHEVRON_TWEEN_SECONDS: f32 = 0.12;

/// The angle the collapsed plate rests at: it points right.
const COLLAPSED_CHEVRON_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;

/// The angle the expanded plate rests at: it points down.
const EXPANDED_CHEVRON_ANGLE: f32 = 0.0;

/// Marker naming the row a chevron belongs to and the state it shows.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeChevron {
    /// The row's stable identity: the tween state is keyed by it.
    pub(crate) semantic_key: String,
    /// Whether the row's group is expanded.
    pub(crate) expanded: bool,
}

/// One chevron's tween state.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ChevronAnimation {
    /// The angle the plate shows right now.
    current: f32,
    /// The angle the plate left when the turn started.
    from: f32,
    /// The angle the plate turns to.
    target: f32,
    /// The seconds the current turn has run.
    elapsed: f32,
    /// The expansion state the tween currently describes.
    expanded: bool,
}

impl ChevronAnimation {
    /// The tween state of a plate that already rests at its target.
    fn settled(expanded: bool) -> Self {
        let angle = if expanded {
            EXPANDED_CHEVRON_ANGLE
        } else {
            COLLAPSED_CHEVRON_ANGLE
        };
        Self {
            current: angle,
            from: angle,
            target: angle,
            elapsed: CHEVRON_TWEEN_SECONDS,
            expanded,
        }
    }

    /// Move the plate toward the target angle of the given expansion state.
    fn advance(&mut self, expanded: bool, delta: f32) {
        let target = if expanded {
            EXPANDED_CHEVRON_ANGLE
        } else {
            COLLAPSED_CHEVRON_ANGLE
        };
        if expanded != self.expanded {
            self.expanded = expanded;
            self.from = self.current;
            self.target = target;
            self.elapsed = 0.0;
        }
        self.elapsed += delta;
        let progress = (self.elapsed / CHEVRON_TWEEN_SECONDS).min(1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.current = self.from + (self.target - self.from) * eased;
    }
}

/// The tween state of every chevron the tree has shown, keyed by row.
#[derive(Resource, Default)]
pub(crate) struct ChevronAnimations {
    states: HashMap<String, ChevronAnimation>,
}

impl ChevronAnimations {
    /// The tween state of one row, created settled when the row is new.
    fn state_of(&mut self, semantic_key: &str, expanded: bool) -> &mut ChevronAnimation {
        self.states
            .entry(semantic_key.to_owned())
            .or_insert_with(|| ChevronAnimation::settled(expanded))
    }
}

/// The chevron scene of one row.
///
/// An expandable row draws the arrow plate; a row without children draws an
/// empty slot of the same size, so the labels of the siblings stay aligned.
pub(super) fn chevron_scene(
    semantic_key: &str,
    expanded: bool,
    has_children: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let angle = if expanded {
        EXPANDED_CHEVRON_ANGLE
    } else {
        COLLAPSED_CHEVRON_ANGLE
    };
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if has_children {
        children.push(crate::icons::icon_scene(
            IconId::NavigateDown,
            palette,
            palette.dim_color,
        ));
    }
    Box::new(bsn! {
        Node {
            width: rem(palette.icon_rem),
            height: rem(palette.icon_rem),
            flex_shrink: 0.0,
            align_self: AlignSelf::Center,
        }
        UiTransform { rotation: Rot2::radians(angle) }
        TreeChevron {
            semantic_key: { semantic_key },
            expanded: { expanded },
        }
        Pickable::IGNORE
        Children [{ children }]
    })
}

/// Turn every mounted chevron toward the angle its row asks for.
///
/// A retired row keeps its chevron until the frame's last flush; it is
/// skipped here, so the retired plate never drives the turn of the row that
/// replaced it.
pub(super) fn animate_chevrons(
    time: Res<Time>,
    mut animations: ResMut<ChevronAnimations>,
    mut chevrons: Query<(&TreeChevron, &mut UiTransform, &Visibility)>,
) {
    let delta = time.delta_secs();
    for (chevron, mut transform, visibility) in &mut chevrons {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let animation = animations.state_of(&chevron.semantic_key, chevron.expanded);
        animation.advance(chevron.expanded, delta);
        transform.rotation = Rot2::radians(animation.current);
    }
}
