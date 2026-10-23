//! Startup forensics: the failed-boot unit panel pinned to the page top
//! (BEVY-147).
//!
//! The shared boot evidence carries the units that failed during this boot
//! ([`StartupFailedUnit`]: unit, load/active/sub state, description). They
//! are the crashed startup services, and the panel pins them to the TOP of
//! the page — above the inventory table and the timeline — so a broken boot
//! is the first thing the page says instead of a row buried in a list.
//!
//! Each failed unit opens an ACCORDION: the row carries the unit and its
//! failure state, and the expanded row reveals the provider's error detail
//! (load/active/sub state and the failure description). The first reported
//! failure starts open — a crashed boot is why the page is being read — and
//! an explicit user choice stays authoritative afterwards.
//!
//! Evidence-driven: no failed units means no panel at all, and a typed
//! provider failure keeps the page's honest "boot evidence unavailable"
//! caption instead of an invented crash list.

use std::collections::BTreeSet;

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::resource::Resource;
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, UiRect, Val, percent,
    rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::Button;
use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::{StartupBootEvidenceSnapshot, StartupFailedUnit};
use taskmanager_ui_contract::IconId;

use super::inks::{InkRole, StartupInk};
use super::on_startup_forensics_toggled;
use crate::icons::icon_scene;
use crate::palette::{UiPalette, space_2, space_4, space_8, space_12};
use crate::window::{Role, TextRole};

/// Marker on the forensics panel root.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct StartupForensicsPanel;

/// Marker on one failed unit's name: the unit identity the panel shows.
///
/// The `Default` seed only exists for the bsn! template mechanism; a marked
/// node always carries the unit's own name.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupForensicsUnit(pub(crate) String);

/// Marker on one failed unit's accordion toggle control.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupForensicsToggle(pub(crate) String);

/// Marker on one failed unit's expanded error detail.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupForensicsDetail(pub(crate) String);

/// The units whose error details are open.
///
/// The first reported failure opens by default — a crashed boot is the reason
/// the page is being read, so its error detail must be one glance away.
/// Once the user toggles a unit their choice is authoritative: the default
/// never overrides it, and an explicitly closed unit stays closed.
pub(crate) fn effective_forensics_units(
    failed: &[StartupFailedUnit],
    expanded: &StartupForensicsExpanded,
) -> BTreeSet<String> {
    if expanded.touched {
        return expanded.units.clone();
    }
    failed
        .first()
        .map(|unit| BTreeSet::from([unit.unit.clone()]))
        .unwrap_or_default()
}

/// The page-owned accordion state: the opened units and whether the user
/// ever made an explicit choice.
#[derive(Resource, Debug, Default, Clone)]
pub(crate) struct StartupForensicsExpanded {
    /// The failed units whose details are open.
    pub(crate) units: BTreeSet<String>,
    /// Whether the user made an explicit choice; until then the first
    /// reported failure stays open by default.
    pub(crate) touched: bool,
}

impl StartupForensicsExpanded {
    /// Toggle one unit's details and make the choice authoritative.
    pub(crate) fn toggle(&mut self, unit: &str) {
        self.touched = true;
        if !self.units.remove(unit) {
            self.units.insert(unit.to_owned());
        }
    }
}

/// The forensics panel scene, or `None` when this boot reported no failed
/// unit (nothing crashed — the panel stays out of the way).
pub(crate) fn forensics_panel_scene(
    evidence: Option<&StartupBootEvidenceSnapshot>,
    expanded: &StartupForensicsExpanded,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    let evidence = evidence?;
    if evidence.failed_units.is_empty() {
        return None;
    }
    let open = effective_forensics_units(&evidence.failed_units, expanded);
    let count = evidence.failed_units.len();
    let title = format!("{} · {count}", t("startup.failed_units"));

    let rows: Vec<Box<dyn Scene>> = evidence
        .failed_units
        .iter()
        .map(|unit| {
            let disclosure = if open.contains(&unit.unit) {
                Disclosure::Expanded
            } else {
                Disclosure::Collapsed
            };
            failed_unit_scene(unit, disclosure, palette)
        })
        .collect();

    Some(Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        StartupForensicsPanel
        Children [
             Node {
                 width: percent(100),
                 flex_direction: FlexDirection::Row,
                 align_items: AlignItems::Center,
                 column_gap: Val::Px(space_8()),
             }
             Children [
                  @{ icon_scene(IconId::TriangleAlert, palette, palette.danger_color) } --
                  Text(title) TextRole(Role::Body) StartupInk(InkRole::Danger)
                  TextLayout { linebreak: LineBreak::NoWrap }
             ]
            --
             { rows }
        ]
    }))
}

/// The disclosure state of one failed-unit accordion.
///
/// A typed state, not a flag: the panel's "expanded" surface is a contract,
/// and a boolean parameter would let a caller mean "open" by accident.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Disclosure {
    /// The error detail is visible.
    Expanded,
    /// Only the header is visible.
    Collapsed,
}

/// One failed unit: the accordion header plus, when open, its error detail.
fn failed_unit_scene(
    unit: &StartupFailedUnit,
    disclosure: Disclosure,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let name = unit.unit.clone();
    let toggle_name = name.clone();
    let detail_name = name.clone();
    let chevron = if disclosure == Disclosure::Expanded {
        IconId::NavigateUp
    } else {
        IconId::NavigateDown
    };
    let state = format!(
        "{} / {} / {}",
        unit.load_state, unit.active_state, unit.sub_state
    );
    let description = unit.description.clone();
    let header = bsn! {
        Node {
            width: percent(100),
            min_height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            overflow: Overflow::clip_x(),
        }
        BackgroundColor({ palette.content_bg })
        StartupForensicsToggle(toggle_name)
        Button
        on(on_startup_forensics_toggled)
        Children [
             @{ icon_scene(chevron, palette, palette.dim_color) } --
             Text({ name.clone() }) TextRole(Role::Body) StartupForensicsUnit(name)
             StartupInk(InkRole::Danger)
             TextLayout { linebreak: LineBreak::NoWrap }
            --
             Node { flex_grow: 1.0 } --
             Text(state) TextRole(Role::Mono) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    };
    if disclosure == Disclosure::Collapsed {
        return Box::new(header);
    }
    let detail = bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::horizontal(Val::Px(space_8())),
        }
        StartupForensicsDetail(detail_name)
        Children [
             Text(description) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordOrCharacter }
        ]
    };
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
             @{ header } --
             @{ detail }
        ]
    })
}
