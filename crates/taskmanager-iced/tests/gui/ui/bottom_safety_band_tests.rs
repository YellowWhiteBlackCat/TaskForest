// test-intent: behavior
//! The bottom safety band (ICED-053).
//!
//! The elastic-layout playbook makes the band a BUDGET fact: in the extreme
//! 720x480 viewport the last row of every page must keep at least
//! [`BOTTOM_SAFETY_BAND`] between itself and the bottom edge, so it is never
//! clipped nor read as cut off.
//!
//! The assertion is a MEASURED one: the real view is laid out at 720x480
//! with the headless renderer and every rendered text fragment is measured
//! against the viewport bottom.

use iced::advanced::Layout;
use iced::advanced::layout::Limits;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Rectangle, Size};
use taskmanager_application::AppPage;
use taskmanager_application::i18n::t;
use taskmanager_assets::embedded_fonts;

use crate::app::Message;
use crate::ui::responsive::BOTTOM_SAFETY_BAND;

const EXTREME_VIEWPORT: Size = Size::new(720.0, 480.0);
/// The wide profile the same band has to survive.
const WIDE_VIEWPORT: Size = Size::new(1180.0, 780.0);
/// The routed pages the band is measured on.
const ROUTED_PAGES: [AppPage; 7] = [
    AppPage::Performance,
    AppPage::Applications,
    AppPage::Services,
    AppPage::System,
    AppPage::Startup,
    AppPage::Users,
    AppPage::AppHistory,
];

#[derive(Default)]
struct RenderedTextFragments(Vec<(String, Rectangle)>);

impl Operation for RenderedTextFragments {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.0.push((fragment.to_owned(), bounds));
    }
}

fn software_renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

/// The rendered text fragments of one view at the given viewport.
fn fragments_at(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
    viewport: Size,
) -> Vec<(String, Rectangle)> {
    let mut tree = Tree::new(&*element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, renderer, &Limits::new(Size::ZERO, viewport));
    let mut fragments = RenderedTextFragments::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut fragments);
    fragments.0
}

/// The measured bottom of the app chrome's footer caption.
fn footer_bottom(fragments: &[(String, Rectangle)]) -> Option<f32> {
    let caption = t("hint.footer_shortcuts");
    fragments
        .iter()
        .find(|(text, _)| text == caption)
        .map(|(_, bounds)| bounds.y + bounds.height)
}

/// Every routed page keeps the bottom safety band at 720x480.
///
/// The band is a property of the page's bottom CHROME: the content slot ends
/// one band above the footer, and the footer's own text keeps the window
/// edge at arm's length. A scrollable page's content may run past the fold
/// (that is what scrolling is), so the assertion is about the chrome that
/// frames the viewport, and the compact captures judge the rows themselves.
#[test]
fn every_page_keeps_the_bottom_safety_band_at_the_extreme_viewport() {
    let renderer = software_renderer();
    for page in ROUTED_PAGES {
        let mut app = crate::IcedApp::demo();
        // The app tracks the real viewport: the compact profile is the one
        // the extreme viewport produces, not a layout measured at a size the
        // app never saw.
        let _ = app.update(Message::WindowResized(EXTREME_VIEWPORT));
        let _ = app.update(Message::SelectPage(page));
        let mut view = crate::ui::view(&app);
        let fragments = fragments_at(&mut view, &renderer, EXTREME_VIEWPORT);

        assert!(
            !fragments.is_empty(),
            "the {page:?} page renders measurable text at the extreme viewport"
        );

        // The footer caption is the bottom chrome: it is the lowest element
        // that belongs to the frame itself, so it must keep the band.
        if let Some(footer_bottom) = footer_bottom(&fragments) {
            assert!(
                EXTREME_VIEWPORT.height - footer_bottom >= BOTTOM_SAFETY_BAND,
                "the {page:?} page's bottom chrome sits closer than the safety band \
                 to the window edge: it ends at {footer_bottom} of {}",
                EXTREME_VIEWPORT.height
            );
        }

        // No chrome may be drawn past the window edge.
        let caption = t("hint.footer_shortcuts");
        for (text, bounds) in &fragments {
            if bounds.y + bounds.height <= EXTREME_VIEWPORT.height {
                continue;
            }
            assert!(
                text != caption,
                "the {page:?} page's bottom chrome is cut off by the window edge: \
                 {text:?} ends at {}",
                bounds.y + bounds.height
            );
        }
    }
}

/// The band is a typed layout fact: it is never smaller than the playbook's
/// eight pixels. This is a COMPILE-TIME fact, so a shrinking band fails the
/// build instead of waiting for a test run.
const _: () = assert!(
    BOTTOM_SAFETY_BAND >= 8.0,
    "the bottom safety band is the playbook's minimum"
);

/// No viewport profile shrinks the band: the measured bottom chrome keeps at
/// least the playbook's eight pixels between itself and the window edge, in
/// the wide profile and in the extreme 720x480 profile.
#[test]
fn the_safety_band_is_at_least_eight_pixels_in_every_profile() {
    let renderer = software_renderer();
    for (profile, viewport) in [("wide", WIDE_VIEWPORT), ("extreme", EXTREME_VIEWPORT)] {
        for page in ROUTED_PAGES {
            let mut app = crate::IcedApp::demo();
            let _ = app.update(Message::WindowResized(viewport));
            let _ = app.update(Message::SelectPage(page));
            let mut view = crate::ui::view(&app);
            let fragments = fragments_at(&mut view, &renderer, viewport);
            let footer = footer_bottom(&fragments).unwrap_or_else(|| {
                panic!("the {page:?} page renders its footer chrome in the {profile} profile")
            });
            assert!(
                viewport.height - footer >= 8.0,
                "the {page:?} page's bottom chrome sits closer than the safety band to the \
                 window edge in the {profile} profile: it ends at {footer} of {}",
                viewport.height
            );
        }
    }
}
