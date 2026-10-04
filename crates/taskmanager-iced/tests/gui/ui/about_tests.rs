//! test-intent: behavior

use super::*;
use crate::app::{LocalSurface, LocalSurfaceKind};
use taskmanager_assets::embedded_fonts;
use taskmanager_assets::product;

#[test]
fn about_and_system_information_have_independent_explicit_entries() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::OpenAbout);
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::About));
    drop(render(&app));
    let _ = app.update(Message::OpenRepository);
    let _ = app.update(Message::OpenSystemInformation);
    let Some(LocalSurface::SystemInformation(facts)) = app.local_surface() else {
        panic!("independent system information");
    };
    assert!(
        facts.iter().any(|group| group
            .rows
            .iter()
            .any(|row| row.label_key == "system_about.hostname"
                && row.value == "taskforest-workstation"))
    );
    let _ = app.update(Message::OpenRepository);
    assert_eq!(
        app.local_surface_kind(),
        Some(LocalSurfaceKind::SystemInformation)
    );
    let _ = app.update(Message::OpenAbout);
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::About));
    assert!(
        metadata(
            env!("CARGO_PKG_VERSION"),
            product::LICENSE_SPDX,
            product::REPOSITORY_URL
        )
        .details_text()
        .contains(product::LICENSE_SPDX)
    );
}

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Scrollable};
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Pixels, Rectangle, Size, Vector};
use taskmanager_shell::presentation::system_information::{
    SystemInformationGroup, SystemInformationRow,
};

#[derive(Default)]
struct Bounds {
    controls: Vec<Rectangle>,
    scrolls: Vec<(Rectangle, Rectangle)>,
}
impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        self.controls.push(bounds);
    }
    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.scrolls.push((bounds, content));
    }
}

#[test]
fn information_reviews_keep_all_actions_in_frame_and_scroll_only_the_body() {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");
    let facts = vec![SystemInformationGroup {
        title_key: "system_about.hardware",
        rows: (0..24)
            .map(|index| SystemInformationRow {
                label_key: "system_about.cpu",
                value: format!(
                    "Observed device {index}: {}",
                    "long complete hardware description ".repeat(12)
                ),
            })
            .collect(),
    }];
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 400.0),
        (720.0, 960.0),
    ] {
        let mut app = crate::IcedApp::demo();
        let size = Size::new(width, height);
        let _ = app.update(Message::WindowResized(size));
        for system_information in [false, true] {
            let mut view = if system_information {
                crate::ui::system_information::render(&app, &facts)
            } else {
                render(&app)
            };
            let mut tree = Tree::new(&view);
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
            let mut bounds = Bounds::default();
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
            assert_eq!(
                bounds.controls.len(),
                if system_information { 2 } else { 5 }
            );
            let mut close = focus::modal_close(app.theme());
            let mut close_tree = Tree::new(&close);
            let natural_close = close
                .as_widget_mut()
                .layout(&mut close_tree, &renderer, &Limits::new(Size::ZERO, size))
                .size();
            assert!(
                bounds.controls.last().expect("Close").width >= natural_close.width,
                "Close must retain its full intrinsic label and padding"
            );
            for control in bounds.controls {
                assert!(control.width > 0.0 && control.height > 0.0);
                assert!(control.x >= 0.0 && control.y >= 0.0);
                assert!(
                    control.x + control.width <= width && control.y + control.height < height,
                    "action {control:?} exceeds {size:?}"
                );
            }
            assert_eq!(bounds.scrolls.len(), 1, "only review metadata scrolls");
            if system_information {
                let (viewport, content) = bounds.scrolls[0];
                assert!(viewport.width > 0.0 && viewport.height > 0.0);
                assert!(
                    viewport.x + viewport.width <= width && viewport.y + viewport.height < height
                );
                assert!(content.width <= viewport.width && content.height > viewport.height);
            }
        }
    }
}

#[test]
fn native_boot_observes_desktop_appearance_once_and_information_uses_the_response() {
    use taskmanager_core::core::appearance::{
        DesktopAppearance, DesktopFamily, PreferredColorScheme,
    };
    use taskmanager_test_support::desktop_appearance::platform;
    let expected = DesktopAppearance {
        family: DesktopFamily::Kde,
        color_scheme: PreferredColorScheme::Dark,
        high_contrast: Some(false),
    };
    let (platform, recorder) = platform(expected);
    let mut app = crate::IcedApp::new(Some(platform));
    assert_eq!(recorder.submissions().expect("requests").len(), 1);
    assert!(
        app.observed_appearance.is_none(),
        "submission alone cannot invent an observation"
    );
    let _ = app.update(Message::Tick);
    let _ = app.update(Message::Tick);
    assert_eq!(recorder.submissions().expect("requests").len(), 1);
    assert_eq!(app.observed_appearance, Some(expected));
    let _ = app.update(Message::OpenSystemInformation);
    let Some(LocalSurface::SystemInformation(facts)) = app.local_surface() else {
        panic!("native information");
    };
    assert!(
        facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.value == "KDE Plasma")
    );
    assert!(
        facts
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| row.label_key == "system_about.color_scheme")
    );
}
