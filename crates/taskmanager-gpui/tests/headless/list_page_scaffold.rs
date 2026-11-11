//! Headless scaffold proof: the whole list header owns one inset glass surface.

use super::*;
use gpui::{AppContext, Styled, TestAppContext, VisualTestContext, px, size};
use taskmanager_theme::{GlassTier, Theme};
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui::theme_binding::{glass_fill, glass_shadow};

#[test]
fn the_page_header_surface_is_inset_and_shares_the_card_tier_material() {
    let theme = Theme::dark();
    let mut painted =
        CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::PageHeader).render();
    assert_eq!(
        painted.style().background.as_ref(),
        Some(&glass_fill(&theme, GlassTier::Card)),
        "the page header must bind the shared Card-tier glass fill"
    );
    assert!(
        painted
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "an inset page header must not stack the floating two-layer shadow"
    );
    assert_eq!(
        glass_shadow(&theme, GlassTier::Card).len(),
        2,
        "the shared shadow ladder stays layered for surfaces that do float"
    );
}

#[gpui::test]
async fn the_scaffold_keeps_one_header_and_bounded_body_at_the_minimum_window(
    cx: &mut TestAppContext,
) {
    let theme = Theme::dark();
    let window = cx.add_window(|_window, _cx| TestRoot {
        theme,
        budget: ListPageBudget {
            rows: 2,
            gap: tokens::SPACE_8.0,
            table_height: 260.0,
            admit_optional_group: false,
        },
    });
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("scaffold window must draw");
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let surface = visual
        .debug_bounds(LIST_PAGE_HEADER_SURFACE_SELECTOR)
        .expect("the one header surface must render");
    let body = visual
        .debug_bounds("tm-test-body")
        .expect("the bounded body must render");
    assert!(surface.right() <= px(720.0));
    assert!(surface.bottom() <= px(480.0));
    assert!(body.right() <= px(720.0));
    assert!(body.bottom() <= px(480.0));
    assert!(body.origin.y >= surface.bottom());
    assert!(
        body.size.height > px(0.0),
        "the bounded body still grows into the remaining page slot"
    );
}

struct TestRoot {
    theme: Theme,
    budget: ListPageBudget,
}

impl gpui::Render for TestRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        ListPageScaffold::new(
            &self.theme,
            div()
                .debug_selector(|| "tm-test-header".to_string())
                .h(px(28.0)),
            div()
                .debug_selector(|| "tm-test-body".to_string())
                .size_full(),
        )
        .budget(self.budget)
        .render()
    }
}
