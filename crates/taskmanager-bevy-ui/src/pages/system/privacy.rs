//! Hardware serial masking: the one control that hides real device serials.
//!
//! The System page states the host identity facts. A system serial, a board
//! serial and a module serial are unique hardware identifiers: useful to
//! their owner, unhelpful in a shared screenshot. The masking toggle
//! replaces exactly those values with a redaction mark; every other fact
//! stays readable and the toggle is the only thing the control changes.
//!
//! The toggle lives in the page's own action row, so its entity survives a
//! repaint: the page body is rebuilt around it, never through it.

use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::prelude::{Node, UiRect, px, rem};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::SmbiosMemorySnapshot;

use super::paint::SystemPaint;
use super::{SystemFactGroup, SystemFactRow};
use crate::palette::{UiPalette, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};

/// Whether the page states the real device serials or masks them.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SerialDisclosure {
    /// The observed serial is stated as the platform reported it.
    #[default]
    Revealed,
    /// The serial is replaced by the redaction mark.
    Masked,
}

/// The page's one masking toggle.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SerialMaskToggle;

/// The redaction mark a masked serial is replaced with.
pub(crate) const MASKED_SERIAL: &str = "********";

/// Whether the disclosure state masks the serial values.
pub(crate) fn masks_serials(disclosure: SerialDisclosure) -> bool {
    matches!(disclosure, SerialDisclosure::Masked)
}

/// The identity facts the SMBIOS lane supplied.
///
/// The rows follow the shared System vocabulary: a fact the platform did not
/// state is omitted (never a dash pretending to be an absence), and the
/// system serial is the only value the masking toggle replaces.
pub(crate) fn identity_fact_rows(
    smbios: Option<&SmbiosMemorySnapshot>,
    disclosure: SerialDisclosure,
) -> Vec<SystemFactRow> {
    let Some(identity) = smbios.and_then(|snapshot| snapshot.identity.as_ref()) else {
        return Vec::new();
    };
    let masked = masks_serials(disclosure);
    let mut rows = Vec::new();
    for (key, value, serial) in [
        (
            "system.field.system_serial",
            identity.system_serial.as_deref(),
            true,
        ),
        (
            "system.field.product_uuid",
            identity.system_uuid.as_deref(),
            false,
        ),
        (
            "system.field.asset_tag",
            identity.board_asset_tag.as_deref(),
            false,
        ),
        (
            "system.field.system_sku",
            identity.system_sku.as_deref(),
            false,
        ),
    ] {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        rows.push(SystemFactRow {
            group: SystemFactGroup::Hardware,
            label: t(key).to_owned(),
            value: if serial && masked {
                MASKED_SERIAL.to_owned()
            } else {
                value.to_owned()
            },
        });
    }
    rows
}

/// The module serial rows of the accepted memory inventory.
///
/// A module serial is a unique hardware identifier exactly like the system
/// serial, so the same masking toggle replaces it.
pub(crate) fn module_serial_rows(
    smbios: Option<&SmbiosMemorySnapshot>,
    disclosure: SerialDisclosure,
) -> Vec<SystemFactRow> {
    let Some(snapshot) = smbios else {
        return Vec::new();
    };
    let masked = masks_serials(disclosure);
    snapshot
        .modules
        .iter()
        .filter_map(|module| {
            let serial = module
                .serial_number
                .as_deref()
                .map(str::trim)
                .filter(|serial| !serial.is_empty())?;
            let locator = module
                .locator
                .as_deref()
                .map(str::trim)
                .filter(|locator| !locator.is_empty())
                .map_or_else(
                    || format!("{} {}", t("system.slot"), module.slot),
                    |locator| locator.to_owned(),
                );
            Some(SystemFactRow {
                group: SystemFactGroup::MemoryInventory,
                label: format!("{locator} · {}", t("system.memory_serial")),
                value: if masked {
                    MASKED_SERIAL.to_owned()
                } else {
                    serial.to_owned()
                },
            })
        })
        .collect()
}

/// The masking toggle control.
pub(crate) fn mask_button_scene(
    disclosure: SerialDisclosure,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node { min_height: rem(palette.control_height_rem), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 }
        Button
        SerialMaskToggle
        ControlVisual(ControlTone::Surface, masks_serials(disclosure))
        on(activate)
        Children [ Text({ t(if masks_serials(disclosure) { "system.unmask_serials" } else { "system.mask_serials" }).to_owned() }) TextRole(Role::Caption) ]
    }
}

/// Flip the disclosure state and ask the page for a repaint.
fn activate(
    event: On<Activate>,
    toggles: Query<&SerialMaskToggle>,
    mut disclosure: ResMut<SerialDisclosure>,
    mut paint: ResMut<SystemPaint>,
) {
    if toggles.get(event.entity).is_err() {
        return;
    }
    *disclosure = match *disclosure {
        SerialDisclosure::Revealed => SerialDisclosure::Masked,
        SerialDisclosure::Masked => SerialDisclosure::Revealed,
    };
    paint.dirty = true;
}

/// Keep the toggle's selected bit in step with the disclosure state.
///
/// The toggle lives in the page's own action row, so its entity survives a
/// repaint; the selected bit is the only thing the control publishes.
fn sync_toggle(
    disclosure: Res<SerialDisclosure>,
    mut toggles: Query<&mut ControlVisual, With<SerialMaskToggle>>,
) {
    let selected = masks_serials(*disclosure);
    for mut visual in &mut toggles {
        if visual.1 != selected {
            visual.1 = selected;
        }
    }
}

/// Register the masking toggle's state and its control wiring.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SerialDisclosure>()
        .add_systems(PostUpdate, sync_toggle);
}
