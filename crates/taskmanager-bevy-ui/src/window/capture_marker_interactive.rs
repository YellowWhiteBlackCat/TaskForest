//! Capture readiness for interactive mounted surfaces (BEVY-INTERACTIVE).

use crate::menu_modal::MenuModalPopover;
use crate::tooltip::TooltipOverlay;
use crate::window::AppShellRoot;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, SystemParam};
use bevy::math::Vec2;
use bevy::prelude::Visibility;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use taskmanager_application::i18n::t;

#[derive(SystemParam)]
pub(super) struct InteractiveCaptureAccess<'w, 's> {
    popovers:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<MenuModalPopover>>,
    shell_roots:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<AppShellRoot>>,
    tooltips: Query<'w, 's, (&'static ComputedNode, &'static Visibility), With<TooltipOverlay>>,
}

pub(super) fn process_menu_mounted(access: &InteractiveCaptureAccess<'_, '_>) -> bool {
    let Ok((root, root_transform)) = access.shell_roots.single() else {
        return false;
    };
    let scale = root.inverse_scale_factor();
    let viewport = root.size() * scale;
    let root_center = root_transform.translation * scale;
    // The whole card must be inside the window before the frame is accepted:
    // a popover that flips past an edge clips its verbs, which a pixel receipt
    // alone would otherwise record as a successful capture.
    access.popovers.iter().any(|(node, transform)| {
        let size = node.size() * scale;
        if size.x <= 0.0 || size.y <= 0.0 {
            return false;
        }
        let center = transform.translation * scale;
        let origin = Vec2::new(
            center.x - root_center.x + viewport.x * 0.5 - size.x * 0.5,
            viewport.y * 0.5 - (center.y - root_center.y) - size.y * 0.5,
        );
        origin.x >= -0.5
            && origin.y >= -0.5
            && origin.x + size.x <= viewport.x + 0.5
            && origin.y + size.y <= viewport.y + 0.5
    })
}

pub(super) fn tooltip_mounted(access: &InteractiveCaptureAccess<'_, '_>) -> bool {
    // A hidden or zero-size overlay is not painted evidence.
    access.tooltips.iter().any(|(node, visibility)| {
        *visibility == Visibility::Visible && node.size().x > 0.0 && node.size().y > 0.0
    })
}

pub(super) fn has_all_texts<'w, 's>(
    query: &Query<'w, 's, (&'static Text, &'static ComputedNode)>,
    labels: &[&'static str],
) -> bool {
    labels.iter().all(|l| {
        let translated = t(l);
        query
            .iter()
            .any(|(t, n)| (t.0 == *l || t.0 == translated) && n.size().x > 0.0 && n.size().y > 0.0)
    })
}
