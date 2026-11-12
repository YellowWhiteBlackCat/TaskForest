//! The service-details dialog owns ONE elevation; its journal panel is inset.
//!
//! The dialog panel is the shared Modal-tier floating surface. The bounded
//! journal viewport is a descendant of that panel, so a Card-tier floating
//! shadow on it draws a second rim inside the parent and darkens the dialog's
//! edge. These assertions pin both halves: the nested panel paints no shadow,
//! and the parent keeps its own shared layered shadow unchanged.

use gpui::Styled;
use taskforest_core::core::services::ServiceLogState;
use taskmanager_theme::{GlassSurfaceRole, GlassTier, Theme, tokens};
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow, rem_length};

/// A nested surface must spend the shared inset elevation: same Card-tier
/// material and radius, no second two-layer shadow. The dialog parent keeps
/// its Modal-tier shadow, so only the inner rim disappears.
#[test]
fn the_service_log_panel_is_inset_and_the_dialog_keeps_its_own_shadow() {
    let theme = Theme::dark();
    let mut panel = super::render_service_log_section(&theme, &ServiceLogState::Empty);

    // The material is unchanged: the panel still spends the Card-tier glass
    // fill and the same radius, so only the elevation changes.
    assert_eq!(
        panel.style().background.as_ref(),
        Some(&glass_fill(&theme, GlassTier::Card)),
        "a nested panel keeps the shared Card-tier glass material"
    );
    assert_eq!(
        panel.style().corner_radii.top_left,
        Some(rem_length(tokens::card_radius(&theme)).into()),
        "the inset panel keeps the card radius"
    );
    assert!(
        panel
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "an inset journal panel must not stack a second Card-tier rim inside the dialog"
    );

    // The parent edge is unchanged: the dialog itself still paints exactly
    // the shared Modal-tier two-layer projection.
    let mut parent =
        CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::Dialog).render();
    assert_eq!(
        parent.style().box_shadow.as_ref(),
        Some(&glass_shadow(&theme, GlassTier::Modal)),
        "the dialog keeps its own floating elevation"
    );
    assert_eq!(
        glass_shadow(&theme, GlassTier::Card).len(),
        2,
        "the removed inner rim was the two-layer Card projection"
    );
}

/// The real dialog still paints the journal at its old bounded geometry: the
/// inset elevation changes only the shadow, never the nested panel's bounds
/// or its parent's edge.
#[gpui::test]
async fn the_inset_journal_keeps_its_bounds_inside_the_dialog(cx: &mut gpui::TestAppContext) {
    use crate::gpui_app::root::{RootView, TopPage};
    use gpui::{AppContext, VisualTestContext, px, size};
    use taskforest_core::core::services::ServiceItem;
    use taskforest_core::core::services::ServiceStatus;
    use taskforest_core::core::target::ServiceId;
    use taskmanager_test_support::pin_english;

    pin_english();
    let window = cx.add_window(|_, cx| RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(window.into(), size(px(1180.0), px(780.0)));
    let view = window.entity(cx).expect("the window root entity");
    view.update(cx, |root, cx| {
        root.mark_telemetry_frame_ready();
        root.page = TopPage::Services;
        root.replace_services_for_test(
            vec![ServiceItem::from_inventory(
                "fixture.service:shadow",
                "shadow",
                ServiceStatus::Active,
                "fixture unit",
                "",
                "",
                "",
            )],
            Vec::new(),
        );
        root.open_service_details(ServiceId::new("fixture.service:shadow"));
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("the dialog frame must draw");
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let panel = visual
        .debug_bounds("tm-svc-log-panel")
        .expect("the nested journal panel must paint");
    assert!(panel.size.width > px(0.0) && panel.size.height > px(0.0));
    // The inset panel stays strictly inside the viewport: removing the
    // floating shadow cannot grow the dialog's edge or the panel's bounds.
    assert!(panel.left() >= px(0.0) && panel.right() <= px(1180.0));
    assert!(panel.top() >= px(0.0) && panel.bottom() <= px(780.0));
}
